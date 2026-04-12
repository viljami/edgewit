use crate::index_manager::IndexManager;
use crate::registry::IndexRegistry;
use crate::schema::definition::FieldType;
use crate::wal::IngestEvent;
use std::collections::HashMap;
use std::ops::Bound;
use std::time::Duration;
use tantivy::Term;
use tantivy::{IndexWriter, TantivyDocument};
use tracing::{error, info, warn};

/// Event forwarded from the WAL thread to the Indexer.
pub struct IndexerEvent {
    pub event: IngestEvent,
    /// WAL byte-offset after this event was written; used for crash-recovery metadata.
    pub wal_offset: u64,
}

/// Sent by the retention worker to purge documents older than `cutoff`.
pub struct PurgeCommand {
    pub index_name: String,
    pub cutoff: chrono::DateTime<chrono::Utc>,
}

/// Background task that drains the indexer channel and writes documents
/// into their respective Tantivy indexes.
///
/// One [`IndexWriter`] is maintained per logical index name.
pub struct IndexerActor {
    manager: IndexManager,
    registry: IndexRegistry,
    receiver: tokio::sync::mpsc::Receiver<IndexerEvent>,
    purge_rx: tokio::sync::mpsc::Receiver<PurgeCommand>,
    writers: HashMap<String, IndexWriter>,
    /// Tracks the latest WAL offset written but not yet committed, per index.
    dirty_writers: HashMap<String, u64>,
    index_memory_mb: usize,
}

impl IndexerActor {
    pub fn new(
        manager: IndexManager,
        registry: IndexRegistry,
        receiver: tokio::sync::mpsc::Receiver<IndexerEvent>,
        index_memory_mb: usize,
        purge_rx: tokio::sync::mpsc::Receiver<PurgeCommand>,
    ) -> Self {
        Self {
            manager,
            registry,
            receiver,
            purge_rx,
            writers: HashMap::new(),
            dirty_writers: HashMap::new(),
            index_memory_mb,
        }
    }

    /// Runs the indexing loop. Spawn this as a Tokio task.
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
        let mut docs_since_commit: usize = 0;

        info!(
            "Indexer started: commit every {commit_interval_secs}s or {commit_interval_docs} docs."
        );

        loop {
            tokio::select! {
                Some(req) = self.receiver.recv() => {
                    let offset = req.wal_offset;
                    if let Err(e) = self.process_event(req.event, offset) {
                        error!("Failed to index document: {e}");
                    } else {
                        docs_since_commit += 1;
                    }
                    if docs_since_commit >= commit_interval_docs {
                        self.commit_all();
                        docs_since_commit = 0;
                    }
                }
                Some(cmd) = self.purge_rx.recv() => {
                    if let Err(e) = self.purge_old_docs(&cmd.index_name, cmd.cutoff) {
                        error!("Retention purge failed for '{}': {e}", cmd.index_name);
                    }
                }
                _ = interval.tick() => {
                    if !self.dirty_writers.is_empty() {
                        self.commit_all();
                        docs_since_commit = 0;
                    }
                }
            }
        }
    }

    // ── Writer management ────────────────────────────────────────────────────

    /// Opens (or returns the cached) `IndexWriter` for `index_name`.
    fn ensure_writer(&mut self, index_name: &str) -> Result<(), String> {
        if self.writers.contains_key(index_name) {
            return Ok(());
        }
        let index = self.manager.get_or_create_index(index_name)?;
        let merge_min: usize = std::env::var("EDGEWIT_MERGE_MIN_SEGMENTS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap_or(10);

        let writer = index
            .writer(self.index_memory_mb * 1_000_000)
            .map_err(|e| e.to_string())?;

        let mut policy = tantivy::merge_policy::LogMergePolicy::default();
        policy.set_min_num_segments(merge_min);
        writer.set_merge_policy(Box::new(policy));

        self.writers.insert(index_name.to_string(), writer);
        Ok(())
    }

    // ── Ingest ───────────────────────────────────────────────────────────────

    fn process_event(&mut self, event: IngestEvent, wal_offset: u64) -> Result<(), String> {
        let def = self
            .registry
            .get(&event.index)
            .ok_or_else(|| format!("Unknown index: '{}'", event.index))?;

        let source: serde_json::Value = serde_json::from_slice(&event.payload)
            .map_err(|e| format!("Invalid JSON payload: {e}"))?;

        if !source.is_object() {
            return Err("Payload must be a JSON object".to_string());
        }

        self.ensure_writer(&event.index)?;

        let index = self.manager.get_or_create_index(&event.index)?;
        let schema = index.schema();

        // Build top-level document: explicit schema fields + full payload in `_source`
        let mut root = serde_json::Map::new();
        if let serde_json::Value::Object(map) = &source {
            for (k, v) in map {
                if def.fields.contains_key(k) && !v.is_null() {
                    root.insert(k.clone(), v.clone());
                }
            }
        }
        root.insert("_source".to_string(), source);

        let doc_str = serde_json::to_string(&root).map_err(|e| e.to_string())?;
        let doc = TantivyDocument::parse_json(&schema, &doc_str)
            .map_err(|e| format!("Failed to parse tantivy doc: {e}"))?;

        self.writers
            .get_mut(&event.index)
            .unwrap()
            .add_document(doc)
            .map_err(|e| format!("Tantivy write error: {e}"))?;

        self.dirty_writers.insert(event.index, wal_offset);
        Ok(())
    }

    // ── Retention ────────────────────────────────────────────────────────────

    /// Deletes all documents in `index_name` whose timestamp field is older than `cutoff`.
    ///
    /// The deletion is staged via `delete_query` and will be flushed to disk on the
    /// next `commit_all()` call (within `commit_interval_secs` seconds at most).
    fn purge_old_docs(
        &mut self,
        index_name: &str,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), String> {
        let def = self
            .registry
            .get(index_name)
            .ok_or_else(|| format!("Index '{index_name}' not found in registry"))?;

        // Validate that the timestamp field is usable for a range deletion
        let ts_field_def = def.fields.get(&def.timestamp_field).ok_or_else(|| {
            format!(
                "Timestamp field '{}' is not declared in index '{}' — add it to fields: to enable retention",
                def.timestamp_field, index_name
            )
        })?;

        if !matches!(ts_field_def.field_type, FieldType::Datetime) {
            return Err(format!(
                "Field '{}' must be type: datetime for retention to work in '{index_name}'",
                def.timestamp_field
            ));
        }

        if !ts_field_def.fast {
            // Deletion will still work but uses a full scan instead of columnar lookup
            warn!(
                "Field '{}' in '{index_name}' lacks fast: true — retention purge will be slow. \
                 Add fast: true to that field definition.",
                def.timestamp_field
            );
        }

        self.ensure_writer(index_name)?;

        let index = self.manager.get_or_create_index(index_name)?;
        let schema = index.schema();
        let ts_field = schema.get_field(&def.timestamp_field).map_err(|_| {
            format!(
                "Field '{}' missing from Tantivy schema",
                def.timestamp_field
            )
        })?;

        let cutoff_tantivy = tantivy::DateTime::from_timestamp_secs(cutoff.timestamp());
        let upper_term = Term::from_field_date_for_search(ts_field, cutoff_tantivy);
        let range_query =
            tantivy::query::RangeQuery::new(Bound::Unbounded, Bound::Excluded(upper_term));

        self.writers
            .get_mut(index_name)
            .unwrap()
            .delete_query(Box::new(range_query))
            .map_err(|e| format!("delete_query failed for '{index_name}': {e}"))?;

        // Mark as dirty so commit_all() will flush the staged deletions.
        // `or_insert(0)` preserves any real WAL offset that's already pending.
        self.dirty_writers
            .entry(index_name.to_string())
            .or_insert(0);

        info!("Retention: staged deletion of docs older than {cutoff} in '{index_name}'.");
        Ok(())
    }

    // ── Commit ───────────────────────────────────────────────────────────────

    /// Commits all dirty writers, embedding the WAL offset in each segment's metadata.
    fn commit_all(&mut self) {
        for (index_name, offset) in self.dirty_writers.drain() {
            if let Some(writer) = self.writers.get_mut(&index_name) {
                match writer.prepare_commit() {
                    Ok(mut commit) => {
                        commit.set_payload(&offset.to_string());
                        match commit.commit() {
                            Ok(_) => {
                                info!("Committed '{index_name}' at WAL offset {offset}.")
                            }
                            Err(e) => error!("Commit failed for '{index_name}': {e}"),
                        }
                    }
                    Err(e) => error!("Prepare commit failed for '{index_name}': {e}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;
    use tantivy::schema::{DateOptions, Schema, TEXT};
    use tantivy::{DateTime as TantivyDateTime, Index, Term, doc};

    /// Verifies that the Tantivy `delete_query` + `RangeQuery` mechanism used
    /// by `purge_old_docs` correctly removes documents older than the cutoff.
    #[test]
    fn test_purge_deletes_old_documents() {
        let mut schema_builder = Schema::builder();
        let ts_field = schema_builder
            .add_date_field("timestamp", DateOptions::default().set_fast().set_indexed());
        let msg_field = schema_builder.add_text_field("message", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let mut writer = index.writer(15_000_000).unwrap();

        // Old document: 2020
        let old_ts = TantivyDateTime::from_timestamp_secs(1577836800); // 2020-01-01T00:00:00Z
        writer
            .add_document(doc!(ts_field => old_ts, msg_field => "old doc"))
            .unwrap();

        // Recent document: 2024
        let new_ts = TantivyDateTime::from_timestamp_secs(1704067200); // 2024-01-01T00:00:00Z
        writer
            .add_document(doc!(ts_field => new_ts, msg_field => "new doc"))
            .unwrap();

        writer.commit().unwrap();

        // Sanity: both docs visible before purge
        let reader = index.reader().unwrap();
        assert_eq!(reader.searcher().num_docs(), 2);

        // Delete everything before 2023-01-01
        let cutoff = TantivyDateTime::from_timestamp_secs(1672531200); // 2023-01-01T00:00:00Z
        let upper_term = Term::from_field_date_for_search(ts_field, cutoff);
        let range_query =
            tantivy::query::RangeQuery::new(Bound::Unbounded, Bound::Excluded(upper_term));
        writer.delete_query(Box::new(range_query)).unwrap();
        writer.commit().unwrap();

        // After purge: only the 2024 doc remains
        reader.reload().unwrap();
        assert_eq!(
            reader.searcher().num_docs(),
            1,
            "old document should have been purged"
        );
    }

    /// Verifies that a purge with a cutoff in the past (before all documents)
    /// deletes nothing.
    #[test]
    fn test_purge_with_past_cutoff_deletes_nothing() {
        let mut schema_builder = Schema::builder();
        let ts_field = schema_builder
            .add_date_field("timestamp", DateOptions::default().set_fast().set_indexed());
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let mut writer = index.writer(15_000_000).unwrap();

        let ts = TantivyDateTime::from_timestamp_secs(1704067200); // 2024-01-01
        writer.add_document(doc!(ts_field => ts)).unwrap();
        writer.commit().unwrap();

        // Cutoff is 2000 — nothing should be deleted
        let cutoff = TantivyDateTime::from_timestamp_secs(946684800); // 2000-01-01
        let upper_term = Term::from_field_date_for_search(ts_field, cutoff);
        let range_query =
            tantivy::query::RangeQuery::new(Bound::Unbounded, Bound::Excluded(upper_term));
        writer.delete_query(Box::new(range_query)).unwrap();
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        assert_eq!(reader.searcher().num_docs(), 1);
    }
}
