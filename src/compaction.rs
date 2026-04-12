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

/// Periodically merges Tantivy segments for every index under `<data_dir>/indexes/`.
///
/// One index = one directory. The worker scans that single directory level —
/// no partition subdirectory traversal required.
pub struct CompactionWorker {
    data_dir: PathBuf,
    interval: Duration,
    min_segments: usize,
}

impl CompactionWorker {
    pub fn new(data_dir: PathBuf) -> Self {
        let interval_secs: u64 = std::env::var("EDGEWIT_COMPACTION_INTERVAL_SECS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .unwrap_or(300);

        let min_segments: usize = std::env::var("EDGEWIT_MERGE_MIN_SEGMENTS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap_or(10);

        Self {
            data_dir,
            interval: Duration::from_secs(interval_secs),
            min_segments,
        }
    }

    /// Runs the compaction loop. Spawn this as a Tokio task.
    pub async fn run(self) {
        info!(
            "Compaction worker started. Interval: {}s, min segments: {}.",
            self.interval.as_secs(),
            self.min_segments
        );
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            ticker.tick().await;
            debug!("Starting compaction cycle.");
            if let Err(e) = self.run_cycle().await {
                error!("Compaction cycle error: {e}");
            }
        }
    }

    async fn run_cycle(&self) -> Result<(), CompactionError> {
        let indexes_dir = self.data_dir.join("indexes");
        if !indexes_dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&indexes_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            // A valid Tantivy index directory always contains meta.json
            if path.is_dir() && path.join("meta.json").exists() {
                if let Err(e) = self.compact_index(path.clone()).await {
                    error!("Compaction failed for {:?}: {e}", path);
                }
            }
        }
        Ok(())
    }

    async fn compact_index(&self, path: PathBuf) -> Result<(), CompactionError> {
        let min_segments = self.min_segments;

        let index = tokio::task::spawn_blocking({
            let p = path.clone();
            move || Index::open_in_dir(p)
        })
        .await??;

        let reader = index.reader()?;
        let segment_count = reader.searcher().segment_readers().len();

        if segment_count < min_segments {
            debug!("{:?} has {segment_count} segments – skipping.", path);
            return Ok(());
        }

        info!(
            "Compacting {:?}: {segment_count} segments >= {min_segments}.",
            path
        );

        let segment_ids: Vec<_> = reader
            .searcher()
            .segment_readers()
            .iter()
            .map(|s| s.segment_id())
            .collect();

        tokio::task::spawn_blocking(move || -> Result<(), CompactionError> {
            let mut writer =
                match index.writer_with_num_threads::<tantivy::TantivyDocument>(1, 15_000_000) {
                    Ok(w) => w,
                    Err(TantivyError::LockFailure(_, _)) => {
                        debug!("{:?} is locked (active ingestion). Skipping.", path);
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                };

            if let Err(e) = writer.merge(&segment_ids).wait() {
                warn!("Merge failed for {:?}: {e}", path);
                return Ok(());
            }
            if let Err(e) = writer.commit() {
                warn!("Commit after merge failed for {:?}: {e}", path);
                return Ok(());
            }
            if let Err(e) = writer.wait_merging_threads() {
                warn!("Wait for merge threads failed for {:?}: {e}", path);
                return Ok(());
            }
            info!("Compaction complete for {:?}.", path);
            Ok(())
        })
        .await??;

        Ok(())
    }
}
