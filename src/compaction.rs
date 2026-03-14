use std::path::PathBuf;
use std::time::Duration;
use tantivy::{Index, TantivyError};
use tracing::{debug, error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tantivy error: {0}")]
    Tantivy(#[from] TantivyError),
    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// A background worker that periodically scans all index partitions
/// and forces a segment merge if there are too many small segments.
/// This prevents file handle exhaustion and degrades gracefully if
/// a partition is currently being written to.
pub struct CompactionWorker {
    data_dir: PathBuf,
    interval: Duration,
    min_segments: usize,
}

impl CompactionWorker {
    pub fn new(data_dir: PathBuf) -> Self {
        let interval_secs = std::env::var("EDGEWIT_COMPACTION_INTERVAL_SECS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .unwrap_or(300);

        let min_segments = std::env::var("EDGEWIT_MERGE_MIN_SEGMENTS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap_or(10);

        Self {
            data_dir,
            interval: Duration::from_secs(interval_secs),
            min_segments,
        }
    }

    /// Starts the compaction loop. Should be spawned as a Tokio task.
    pub async fn run(self) {
        info!(
            "Compaction worker started. Interval: {}s, Min segments: {}",
            self.interval.as_secs(),
            self.min_segments
        );
        let mut ticker = tokio::time::interval(self.interval);

        loop {
            ticker.tick().await;
            debug!("Starting background compaction cycle.");
            if let Err(e) = self.run_compaction_cycle().await {
                error!("Compaction cycle failed: {}", e);
            }
        }
    }

    async fn run_compaction_cycle(&self) -> Result<(), CompactionError> {
        let indexes_dir = self.data_dir.join("indexes");
        if !indexes_dir.exists() {
            return Ok(());
        }

        let mut index_entries = tokio::fs::read_dir(&indexes_dir).await?;

        while let Some(index_entry) = index_entries.next_entry().await? {
            let index_path = index_entry.path();
            if !index_path.is_dir() {
                continue;
            }

            let segments_dir = index_path.join("segments");
            if !segments_dir.exists() {
                continue;
            }

            let mut partition_entries = tokio::fs::read_dir(&segments_dir).await?;
            while let Some(partition_entry) = partition_entries.next_entry().await? {
                let partition_path = partition_entry.path();

                if partition_path.is_dir() {
                    // Check for meta.json to ensure it's a valid Tantivy index before attempting to open
                    if partition_path.join("meta.json").exists()
                        && let Err(e) = self.compact_partition(partition_path.clone()).await
                    {
                        error!("Error compacting partition {:?}: {}", partition_path, e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn compact_partition(&self, path: PathBuf) -> Result<(), CompactionError> {
        let min_segments = self.min_segments;

        // Open the index inside a blocking task since file I/O is involved
        let index = tokio::task::spawn_blocking({
            let path_clone = path.clone();
            move || Index::open_in_dir(path_clone)
        })
        .await??;

        // Count segments
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let segment_readers = searcher.segment_readers();

        if segment_readers.len() >= min_segments {
            info!(
                "Partition {:?} has {} segments (>= {}). Compacting...",
                path,
                segment_readers.len(),
                min_segments
            );

            let segment_ids: Vec<_> = segment_readers.iter().map(|s| s.segment_id()).collect();

            // Perform the actual compaction in a blocking thread to avoid starving Tokio
            tokio::task::spawn_blocking(move || -> Result<(), CompactionError> {
                // We request a writer with minimal memory because this is just a merge, not heavy ingestion
                let mut writer = match index
                    .writer_with_num_threads::<tantivy::TantivyDocument>(1, 15_000_000)
                {
                    Ok(w) => w,
                    Err(TantivyError::LockFailure(_, _)) => {
                        debug!(
                            "Partition {:?} is locked (active ingestion). Skipping compaction.",
                            path
                        );
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                };

                // Dispatch the merge request
                let _merge_future = writer.merge(&segment_ids).wait();
                if let Err(e) = _merge_future {
                    warn!("Merge request failed for {:?}: {}", path, e);
                    return Ok(());
                }

                // Commit the changes so the new segment replaces the old ones in meta.json
                if let Err(e) = writer.commit() {
                    warn!("Error committing merged segment in {:?}: {}", path, e);
                    return Ok(());
                } else {
                    info!("Successfully committed compacted segment {:?}", path);
                }

                // Block until the background merge thread finishes combining the segments
                if let Err(e) = writer.wait_merging_threads() {
                    warn!("Error waiting for merge in {:?}: {}", path, e);
                    return Ok(());
                }

                Ok(())
            })
            .await??;
        } else {
            debug!(
                "Partition {:?} has {} segments. No compaction needed.",
                path,
                segment_readers.len()
            );
        }

        Ok(())
    }
}
