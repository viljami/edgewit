use crate::partition::format_partition_name;
use crate::registry::IndexRegistry;
use crate::schema::definition::PartitionStrategy;
use chrono::Utc;
use std::path::PathBuf;
use std::time::Duration;
use tantivy::Index;
use tracing::{info, warn};

pub struct RetentionConfig {
    pub max_index_bytes: u64,
    pub max_wal_bytes: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_index_bytes: 1024 * 1024 * 1024, // 1 GB
            max_wal_bytes: 512 * 1024 * 1024,    // 512 MB
        }
    }
}

impl RetentionConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = std::env::var("EDGEWIT_MAX_INDEX_BYTES") {
            if let Some(parsed) = parse_size(&val) {
                config.max_index_bytes = parsed;
            } else {
                warn!("Invalid EDGEWIT_MAX_INDEX_BYTES: {}", val);
            }
        }

        if let Ok(val) = std::env::var("EDGEWIT_MAX_WAL_BYTES") {
            if let Some(parsed) = parse_size(&val) {
                config.max_wal_bytes = parsed;
            } else {
                warn!("Invalid EDGEWIT_MAX_WAL_BYTES: {}", val);
            }
        }

        config
    }
}

fn parse_size(size_str: &str) -> Option<u64> {
    let s = size_str.trim().to_uppercase();
    let (num_str, multiplier) = if s.ends_with("GB") {
        (&s[..s.len() - 2], 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (&s[..s.len() - 2], 1024 * 1024)
    } else if s.ends_with("KB") {
        (&s[..s.len() - 2], 1024)
    } else if s.ends_with('B') {
        (&s[..s.len() - 1], 1)
    } else {
        (s.as_str(), 1)
    };

    num_str.trim().parse::<u64>().ok().map(|v| v * multiplier)
}

pub async fn run_compaction_and_retention_worker(
    data_dir: PathBuf,
    index: Index,
    config: RetentionConfig,
    registry: IndexRegistry,
) {
    let mut interval = tokio::time::interval(Duration::from_mins(5)); // Run every 5 minutes

    loop {
        interval.tick().await;
        info!("Running background compaction and retention checks...");

        apply_partition_retention(&data_dir, &registry).await;

        let index_dir = data_dir.join("index");

        // 1. Calculate Index Size
        let index_size = get_dir_size(&index_dir);
        if index_size > config.max_index_bytes {
            warn!(
                "Index size {} exceeds limit {}. In a full edge deployment we would drop oldest documents here.",
                index_size, config.max_index_bytes
            );
            // TODO: Delete oldest documents. For now, since Tantivy doesn't easily support age-based dropping
            // without a time field, we rely on the operator to use the API or rotate indices.
        } else {
            info!(
                "Index size: {} bytes (Limit: {})",
                index_size, config.max_index_bytes
            );
        }

        // 2. Clear Old WAL Files
        let wal_size = get_dir_size(&data_dir) - index_size;
        info!(
            "Total WAL size: {} bytes (Limit: {})",
            wal_size, config.max_wal_bytes
        );

        // Let's get the last committed WAL offset from the index
        let mut committed_offset = 0;
        if let Ok(metas) = index.load_metas()
            && let Some(payload) = metas.payload
            && let Ok(offset) = payload.parse::<u64>()
        {
            committed_offset = offset;
        }

        if let Err(e) = cleanup_wals(&data_dir, committed_offset, config.max_wal_bytes) {
            warn!("Cleanup wals failed: {e}");
        }
    }
}

async fn apply_partition_retention(data_dir: &PathBuf, registry: &IndexRegistry) {
    for def in registry.list() {
        if def.partition == PartitionStrategy::None {
            continue;
        }

        if let Some(retention_str) = &def.retention
            && let Some(duration) = parse_retention_duration(retention_str) {
                let cutoff_date = Utc::now() - duration;
                let cutoff_partition = format_partition_name(&cutoff_date, &def.partition);

                let segments_dir = data_dir.join("indexes").join(&def.name).join("segments");
                if let Ok(mut entries) = tokio::fs::read_dir(&segments_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if let Ok(file_type) = entry.file_type().await
                            && file_type.is_dir()
                                && let Some(partition_name) = entry.file_name().to_str()
                                    && partition_name != "default"
                                        && partition_name < cutoff_partition.as_str()
                                    {
                                        info!(
                                            "Deleting expired partition: {} for index {}",
                                            partition_name, def.name
                                        );
                                        if let Err(e) =
                                            tokio::fs::remove_dir_all(entry.path()).await
                                        {
                                            warn!(
                                                "Failed to delete expired partition {:?}: {}",
                                                entry.path(),
                                                e
                                            );
                                        }
                                    }
                    }
                }
            }
    }
}

fn parse_retention_duration(retention: &str) -> Option<chrono::Duration> {
    if retention.is_empty() {
        return None;
    }
    let (num_part, unit_part) = retention.split_at(retention.len() - 1);
    let num = num_part.parse::<i64>().ok()?;
    match unit_part {
        "s" => Some(chrono::Duration::seconds(num)),
        "m" => Some(chrono::Duration::minutes(num)),
        "h" => Some(chrono::Duration::hours(num)),
        "d" => Some(chrono::Duration::days(num)),
        "w" => Some(chrono::Duration::weeks(num)),
        "M" => Some(chrono::Duration::days(num * 30)),
        "Y" => Some(chrono::Duration::days(num * 365)),
        _ => None,
    }
}

fn cleanup_wals(
    data_dir: &PathBuf,
    committed_offset: u64,
    max_wal_bytes: u64,
) -> Result<(), std::io::Error> {
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        let mut wal_files = Vec::new();
        let mut total_wal_size = 0;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("wal")
                && let Some(file_stem) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(start_offset) = u64::from_str_radix(file_stem, 16)
            {
                let size = entry.metadata()?.len();
                wal_files.push((start_offset, path, size));
                total_wal_size += size;
            }
        }

        // Sort by offset (oldest first)
        wal_files.sort_by_key(|(offset, _, _)| *offset);

        // First pass: We can safely delete any WAL file whose end offset is <= committed_offset.
        let mut remaining_wal_files = Vec::new();
        for (offset, path, size) in wal_files {
            let estimated_end = offset + size;
            if estimated_end <= committed_offset {
                info!(
                    "Deleting old WAL file: {:?} (end offset {} <= committed {})",
                    path, estimated_end, committed_offset
                );
                if let Err(e) = std::fs::remove_file(&path) {
                    warn!("Failed to delete old WAL file {:?}: {}", path, e);
                } else {
                    total_wal_size = total_wal_size.saturating_sub(size);
                }
            } else {
                remaining_wal_files.push((offset, path, size));
            }
        }

        // Second pass: Emergency Circuit Breaker
        // If we are still exceeding max_wal_bytes, delete oldest uncommitted WAL files
        // (This sacrifices data to save the edge device from disk exhaustion)
        for (_, path, size) in remaining_wal_files {
            if total_wal_size <= max_wal_bytes {
                break;
            }
            warn!(
                "EMERGENCY PRUNING: Deleting uncommitted WAL file {:?} to stay under {} limit (current size: {})",
                path, max_wal_bytes, total_wal_size
            );
            if let Err(e) = std::fs::remove_file(&path) {
                warn!("Failed to emergency delete WAL file {:?}: {}", path, e);
            } else {
                total_wal_size = total_wal_size.saturating_sub(size);
            }
        }
    }

    Ok(())
}

fn get_dir_size(path: &std::path::Path) -> u64 {
    let mut size = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    size += metadata.len();
                } else if metadata.is_dir() {
                    size += get_dir_size(&entry.path());
                }
            }
        }
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_get_dir_size() {
        let temp_dir = TempDir::new().unwrap();
        let file1_path = temp_dir.path().join("file1.txt");
        let mut file1 = File::create(file1_path).unwrap();
        file1.write_all(&[0u8; 1024]).unwrap();

        let sub_dir = temp_dir.path().join("sub");
        std::fs::create_dir(&sub_dir).unwrap();
        let file2_path = sub_dir.join("file2.txt");
        let mut file2 = File::create(file2_path).unwrap();
        file2.write_all(&[0u8; 2048]).unwrap();

        let size = get_dir_size(temp_dir.path());
        assert_eq!(size, 3072);
    }

    #[test]
    fn test_parse_retention_duration() {
        assert_eq!(
            parse_retention_duration("7d"),
            Some(chrono::Duration::days(7))
        );
        assert_eq!(
            parse_retention_duration("12h"),
            Some(chrono::Duration::hours(12))
        );
        assert_eq!(
            parse_retention_duration("1M"),
            Some(chrono::Duration::days(30))
        );
        assert_eq!(parse_retention_duration("invalid"), None);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100"), Some(100));
        assert_eq!(parse_size("100B"), Some(100));
        assert_eq!(parse_size("1KB"), Some(1024));
        assert_eq!(parse_size(" 5 MB "), Some(5 * 1024 * 1024));
        assert_eq!(parse_size("2GB"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("invalid"), None);
    }

    #[test]
    fn test_cleanup_wals() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_path_buf();

        // Create wal files
        // 0.wal, size 100
        let mut w1 = File::create(dir_path.join("0000000000000000.wal")).unwrap();
        w1.write_all(&[0u8; 100]).unwrap();

        // 100.wal, size 50 (starts at 100 in hex: 0000000000000064)
        let mut w2 = File::create(dir_path.join("0000000000000064.wal")).unwrap();
        w2.write_all(&[0u8; 50]).unwrap();

        // 150.wal, size 200 (starts at 150 in hex: 0000000000000096)
        let mut w3 = File::create(dir_path.join("0000000000000096.wal")).unwrap();
        w3.write_all(&[0u8; 200]).unwrap();

        // Create a non-wal file to ensure it gets ignored
        let mut other = File::create(dir_path.join("other.txt")).unwrap();
        other.write_all(&[0u8; 100]).unwrap();

        // Cleanup with committed_offset = 150, max limit = 1000 (no emergency pruning)
        // - 0.wal (ends at 100) -> 100 <= 150, should be deleted
        // - 100.wal (ends at 150) -> 150 <= 150, should be deleted
        // - 150.wal (ends at 350) -> 350 > 150, should be kept
        cleanup_wals(&dir_path, 150, 1000).expect("wal cleanup failed");

        // Check which files exist
        assert!(!dir_path.join("0000000000000000.wal").exists());
        assert!(!dir_path.join("0000000000000064.wal").exists());
        assert!(dir_path.join("0000000000000096.wal").exists());
        assert!(dir_path.join("other.txt").exists());
    }

    #[test]
    fn test_emergency_wal_pruning() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_path_buf();

        // 0.wal, size 100
        let mut w1 = File::create(dir_path.join("0000000000000000.wal")).unwrap();
        w1.write_all(&[0u8; 100]).unwrap();

        // 100.wal, size 200
        let mut w2 = File::create(dir_path.join("0000000000000064.wal")).unwrap();
        w2.write_all(&[0u8; 200]).unwrap();

        // 300.wal, size 300
        let mut w3 = File::create(dir_path.join("000000000000012c.wal")).unwrap();
        w3.write_all(&[0u8; 300]).unwrap();

        // Total size = 600.
        // Nothing is committed yet (committed_offset = 0)
        // Max limit is 400.
        // This should trigger emergency pruning. It will delete oldest (0.wal) first.
        // New size = 500. Still > 400. Deletes 100.wal.
        // New size = 300. <= 400. Stops.
        cleanup_wals(&dir_path, 0, 400).expect("wal cleanup failed");

        assert!(!dir_path.join("0000000000000000.wal").exists());
        assert!(!dir_path.join("0000000000000064.wal").exists());
        assert!(dir_path.join("000000000000012c.wal").exists());
    }
}
