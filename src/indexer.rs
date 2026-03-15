use crate::index_manager::IndexManager;
use crate::partition::resolve_partition;
use crate::registry::IndexRegistry;
use crate::wal::IngestEvent;
use std::collections::HashMap;
use std::time::Duration;
use tantivy::{IndexWriter, TantivyDocument};
use tracing::{error, info};

/// Message sent from the WAL thread to the Indexer thread
pub struct IndexerEvent {
    pub event: IngestEvent,
    /// The offset in the WAL *after* this event was completely written.
    /// Used for crash recovery.
    pub wal_offset: u64,
}

/// The background task responsible for reading from the WAL channel,
/// routing documents to specific partition IndexWriters, and committing segments.
pub struct IndexerActor {
    manager: IndexManager,
    registry: IndexRegistry,
    receiver: tokio::sync::mpsc::Receiver<IndexerEvent>,
    writers: HashMap<(String, String), IndexWriter>,
    dirty_writers: HashMap<(String, String), u64>, // Maps (index, partition) to the latest wal_offset written
    index_memory_mb: usize,
}

impl IndexerActor {
    pub fn new(
        manager: IndexManager,
        registry: IndexRegistry,
        receiver: tokio::sync::mpsc::Receiver<IndexerEvent>,
        index_memory_mb: usize,
    ) -> Self {
        Self {
            manager,
            registry,
            receiver,
            writers: HashMap::new(),
            dirty_writers: HashMap::new(),
            index_memory_mb,
        }
    }

    /// Starts the asynchronous indexing loop.
    /// This should be spawned as a Tokio task.
    pub async fn run(mut self) {
        let commit_interval_secs: u64 = std::env::var("EDGEWIT_COMMIT_INTERVAL_SECS")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);
        let commit_interval_docs: usize = std::env::var("EDGEWIT_COMMIT_INTERVAL_DOCS")
            .unwrap_or_else(|_| "10000".to_string())
            .parse()
            .unwrap_or(10_000);

        let mut interval = tokio::time::interval(Duration::from_secs(commit_interval_secs));
        let mut docs_since_commit = 0;

        info!(
            "Indexer thread started with {}s / {} docs commit interval.",
            commit_interval_secs, commit_interval_docs
        );

        loop {
            tokio::select! {
                Some(req) = self.receiver.recv() => {
                    let wal_offset = req.wal_offset;

                    if let Err(e) = self.process_event(req.event, wal_offset) {
                        error!("Failed to index document: {e}");
                    } else {
                        docs_since_commit += 1;
                    }

                    // Adaptive batching constraint: flush segments when we reach configured limit
                    if docs_since_commit >= commit_interval_docs {
                        self.commit_all();
                        docs_since_commit = 0;
                    }
                }
                // Time constraint: flush segments if the time interval elapses and we have uncommitted data
                _ = interval.tick() => {
                    if !self.dirty_writers.is_empty() {
                        self.commit_all();
                        docs_since_commit = 0;
                    }
                }
            }
        }
    }

    fn process_event(&mut self, event: IngestEvent, wal_offset: u64) -> Result<(), String> {
        let def = self
            .registry
            .get(&event.index)
            .ok_or_else(|| format!("Index not found: {}", event.index))?;

        // Parse the incoming payload as a JSON value
        let source_val: serde_json::Value = serde_json::from_slice(&event.payload)
            .map_err(|e| format!("Invalid JSON payload: {e}"))?;

        if !source_val.is_object() {
            return Err("Payload must be a JSON object".to_string());
        }

        // Determine the correct partition based on the schema and timestamp field
        let partition = resolve_partition(&def, &source_val)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "default".to_string());

        let key = (event.index.clone(), partition.clone());

        // Ensure we have an open writer for this (index, partition)
        if !self.writers.contains_key(&key) {
            let index = self.manager.get_or_create_index(&event.index, &partition)?;

            let merge_min_segments: usize = std::env::var("EDGEWIT_MERGE_MIN_SEGMENTS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10);

            // Open a new writer for this specific partition.
            // Note: In an extreme multi-partition scenario on small RAM, we may need to cap the total memory
            // shared across all writers, or evict old writers from the pool. For now, we assign the budget per writer.
            let writer = index
                .writer(self.index_memory_mb * 1_000_000)
                .map_err(|e| e.to_string())?;

            let mut merge_policy = tantivy::merge_policy::LogMergePolicy::default();
            merge_policy.set_min_num_segments(merge_min_segments);
            writer.set_merge_policy(Box::new(merge_policy));

            self.writers.insert(key.clone(), writer);
        }

        let writer = self.writers.get_mut(&key).unwrap();
        let index = self.manager.get_or_create_index(&event.index, &partition)?;
        let schema = index.schema();

        // Construct the root JSON document matching the Tantivy schema
        let mut root_doc = serde_json::Map::new();

        // Map explicitly defined fields to the top level for Tantivy to index
        if let serde_json::Value::Object(map) = &source_val {
            for (k, v) in map {
                if def.fields.contains_key(k) && !v.is_null() {
                    root_doc.insert(k.clone(), v.clone());
                }
            }
        }

        // Edgewit expects the full original JSON payload in `_source`
        root_doc.insert("_source".to_string(), source_val);

        let doc_str = serde_json::to_string(&root_doc).map_err(|e| e.to_string())?;

        let doc = TantivyDocument::parse_json(&schema, &doc_str)
            .map_err(|e| format!("Failed to parse document: {e}"))?;

        writer
            .add_document(doc)
            .map_err(|e| format!("Tantivy write error: {e}"))?;

        // Mark this writer as dirty and store the WAL offset
        self.dirty_writers.insert(key, wal_offset);

        Ok(())
    }

    /// Flushes segments and records the latest WAL offsets for all dirty partition writers.
    fn commit_all(&mut self) {
        for (key, offset) in self.dirty_writers.drain() {
            if let Some(writer) = self.writers.get_mut(&key) {
                match writer.prepare_commit() {
                    Ok(mut commit) => {
                        // Embed the WAL offset into the partition segment metadata for crash recovery
                        commit.set_payload(&offset.to_string());
                        match commit.commit() {
                            Ok(_) => info!(
                                "Committed index segment for {}/{} at WAL offset {}",
                                key.0, key.1, offset
                            ),
                            Err(e) => error!(
                                "Failed to commit index segment for {}/{}: {e}",
                                key.0, key.1
                            ),
                        }
                    }
                    Err(e) => error!(
                        "Failed to prepare index commit for {}/{}: {e}",
                        key.0, key.1
                    ),
                }
            }
        }
    }
}
