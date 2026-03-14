use crate::wal::IngestEvent;
use std::path::Path;
use std::time::Duration;
use tantivy::schema::{JsonObjectOptions, Schema, TEXT};
use tantivy::{Index, IndexWriter, TantivyDocument};
use tracing::{error, info};

/// Message sent from the WAL thread to the Indexer thread
pub struct IndexerEvent {
    pub event: IngestEvent,
    /// The offset in the WAL *after* this event was completely written.
    /// Used for crash recovery.
    pub wal_offset: u64,
}

/// Builds the simple OpenSearch-like schema for M2
pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    // The `_source` field acts as our dynamic JSON catch-all, similar to OpenSearch.
    // Setting TEXT tokenizes the values for full-text search.
    // Setting STORED allows us to return the raw original JSON on query matches.
    let json_options = JsonObjectOptions::from(TEXT).set_stored().set_fast(None);
    builder.add_json_field("_source", json_options);

    // Explicitly track the index name so we can filter by `_index` in queries
    builder.add_text_field("_index", tantivy::schema::STRING | tantivy::schema::STORED);

    builder.build()
}

/// Opens an existing Tantivy index or creates a new one at the specified path
pub fn setup_index(data_dir: &Path) -> Result<Index, String> {
    let index_dir = data_dir.join("index");
    std::fs::create_dir_all(&index_dir).map_err(|e| format!("Failed to create index dir: {e}"))?;

    let schema = build_schema();

    // We use MmapDirectory for edge performance; Linux gracefully handles paging it into memory
    let dir = tantivy::directory::MmapDirectory::open(&index_dir)
        .map_err(|e| format!("Failed to open directory: {e}"))?;

    Index::open_or_create(dir, schema).map_err(|e| format!("Failed to open index: {e}"))
}

/// Parses the raw HTTP payload and adds it to the Tantivy memory buffer
pub fn add_to_index(
    writer: &mut IndexWriter,
    schema: &Schema,
    event: IngestEvent,
) -> Result<(), String> {
    // Parse the incoming payload as a JSON value
    let source_val: serde_json::Value =
        serde_json::from_slice(&event.payload).map_err(|e| format!("Invalid JSON payload: {e}"))?;

    if !source_val.is_object() {
        return Err("Payload must be a JSON object".to_string());
    }

    // Construct the root JSON document matching our schema
    let mut root_doc = serde_json::Map::new();
    root_doc.insert("_index".to_string(), serde_json::Value::String(event.index));
    root_doc.insert("_source".to_string(), source_val);

    let doc_str = serde_json::to_string(&root_doc).map_err(|e| e.to_string())?;

    let doc = TantivyDocument::parse_json(schema, &doc_str)
        .map_err(|e| format!("Failed to parse document: {e}"))?;

    writer
        .add_document(doc)
        .map_err(|e| format!("Tantivy write error: {e}"))?;

    Ok(())
}

/// The background task responsible for reading from the WAL channel,
/// updating the Tantivy index, and committing segments to disk.
pub struct IndexerActor {
    writer: IndexWriter,
    schema: Schema,
    receiver: tokio::sync::mpsc::Receiver<IndexerEvent>,
}

impl IndexerActor {
    pub fn new(
        writer: IndexWriter,
        schema: Schema,
        receiver: tokio::sync::mpsc::Receiver<IndexerEvent>,
    ) -> Self {
        Self {
            writer,
            schema,
            receiver,
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
        let mut latest_wal_offset = 0;

        info!(
            "Indexer thread started with {}s / {} docs commit interval.",
            commit_interval_secs, commit_interval_docs
        );

        loop {
            tokio::select! {
                Some(req) = self.receiver.recv() => {
                    latest_wal_offset = req.wal_offset;

                    if let Err(e) = add_to_index(&mut self.writer, &self.schema, req.event) {
                        error!("Failed to index document: {e}");
                    } else {
                        docs_since_commit += 1;
                    }

                    // Adaptive batching constraint: flush segments when we reach configured limit
                    if docs_since_commit >= commit_interval_docs {
                        self.commit_segment(latest_wal_offset);
                        docs_since_commit = 0;
                    }
                }
                // Time constraint: flush segments if the time interval elapses and we have uncommitted data
                _ = interval.tick() => {
                    if docs_since_commit > 0 {
                        self.commit_segment(latest_wal_offset);
                        docs_since_commit = 0;
                    }
                }
            }
        }
    }

    /// Internal helper to finalize a segment and save the WAL offset metadata.
    fn commit_segment(&mut self, offset: u64) {
        match self.writer.prepare_commit() {
            Ok(mut commit) => {
                // We embed the WAL offset directly into the Tantivy segment metadata!
                // If the Pi crashes, we read this payload on startup to resume the WAL replay.
                commit.set_payload(&offset.to_string());
                match commit.commit() {
                    Ok(_) => info!("Committed index segment at WAL offset {}", offset),
                    Err(e) => error!("Failed to commit index segment: {e}"),
                }
            }
            Err(e) => error!("Failed to prepare index commit: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_schema() {
        let schema = build_schema();
        assert!(schema.get_field("_source").is_ok());
        assert!(schema.get_field("_index").is_ok());
    }

    #[test]
    fn test_setup_index() {
        let temp_dir = TempDir::new().unwrap();
        let index_res = setup_index(temp_dir.path());
        assert!(index_res.is_ok());

        // Verify the directory was created
        assert!(temp_dir.path().join("index").exists());
    }

    #[test]
    fn test_add_to_index_valid_json() {
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());
        let mut writer = index.writer(15_000_000).unwrap();

        let event = IngestEvent {
            index: "test_index".to_string(),
            payload: b"{\"message\":\"hello world\"}".to_vec(),
        };

        let result = add_to_index(&mut writer, &schema, event);
        assert!(result.is_ok());

        writer.commit().unwrap();
        let reader = index.reader().unwrap();
        let searcher = reader.searcher();
        assert_eq!(searcher.num_docs(), 1);
    }

    #[test]
    fn test_add_to_index_invalid_json() {
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());
        let mut writer = index.writer(15_000_000).unwrap();

        let event = IngestEvent {
            index: "test_index".to_string(),
            payload: b"not json".to_vec(),
        };

        let result = add_to_index(&mut writer, &schema, event);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON"));
    }

    #[test]
    fn test_add_to_index_not_an_object() {
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());
        let mut writer = index.writer(15_000_000).unwrap();

        let event = IngestEvent {
            index: "test_index".to_string(),
            payload: b"\"just a string\"".to_vec(),
        };

        let result = add_to_index(&mut writer, &schema, event);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Payload must be a JSON object")
        );
    }
}
