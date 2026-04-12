use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::indexer::PurgeCommand;
use crate::registry::IndexRegistry;

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
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("EDGEWIT_MAX_INDEX_BYTES") {
            if let Some(n) = parse_size(&v) {
                cfg.max_index_bytes = n;
            } else {
                warn!("Invalid EDGEWIT_MAX_INDEX_BYTES: {v}");
            }
        }
        if let Ok(v) = std::env::var("EDGEWIT_MAX_WAL_BYTES") {
            if let Some(n) = parse_size(&v) {
                cfg.max_wal_bytes = n;
            } else {
                warn!("Invalid EDGEWIT_MAX_WAL_BYTES: {v}");
            }
        }
        cfg
    }
}

/// Recursively computes the total byte size of all files under `path`.
pub(crate) fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            e.metadata().map_or(0, |m| {
                if m.is_file() {
                    m.len()
                } else if m.is_dir() {
                    dir_size(&e.path())
                } else {
                    0
                }
            })
        })
        .sum()
}

/// Parses a retention string like `"7d"`, `"12h"`, `"1M"` into a [`chrono::Duration`].
pub fn parse_retention_duration(s: &str) -> Option<chrono::Duration> {
    if s.is_empty() {
        return None;
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let n: i64 = num_str.parse().ok()?;
    match unit {
        "s" => Some(chrono::Duration::seconds(n)),
        "m" => Some(chrono::Duration::minutes(n)),
        "h" => Some(chrono::Duration::hours(n)),
        "d" => Some(chrono::Duration::days(n)),
        "w" => Some(chrono::Duration::days(n * 7)),
        "M" => Some(chrono::Duration::days(n * 30)),
        "y" => Some(chrono::Duration::days(n * 365)),
        _ => None,
    }
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    let (num_str, mul) = if s.ends_with("GB") {
        (&s[..s.len() - 2], 1_073_741_824u64)
    } else if s.ends_with("MB") {
        (&s[..s.len() - 2], 1_048_576u64)
    } else if s.ends_with("KB") {
        (&s[..s.len() - 2], 1_024u64)
    } else if s.ends_with('B') {
        (&s[..s.len() - 1], 1u64)
    } else {
        (s.as_str(), 1u64)
    };
    num_str.trim().parse::<u64>().ok().map(|v| v * mul)
}

fn cleanup_wals(
    data_dir: &PathBuf,
    committed_offset: u64,
    max_wal_bytes: u64,
) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return Ok(());
    };

    let mut wal_files: Vec<(u64, PathBuf, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.is_file() || path.extension()?.to_str() != Some("wal") {
                return None;
            }
            let start = u64::from_str_radix(path.file_stem()?.to_str()?, 16).ok()?;
            let size = e.metadata().ok()?.len();
            Some((start, path, size))
        })
        .collect();

    wal_files.sort_by_key(|(offset, _, _)| *offset);

    let mut total: u64 = wal_files.iter().map(|(_, _, s)| s).sum();

    // First pass: delete WAL files whose data is fully committed
    let mut remaining = Vec::new();
    for (start, path, size) in wal_files {
        if start + size <= committed_offset {
            info!("Deleting committed WAL: {:?}", path);
            if let Err(e) = std::fs::remove_file(&path) {
                warn!("Failed to delete WAL {:?}: {e}", path);
            } else {
                total = total.saturating_sub(size);
            }
        } else {
            remaining.push((start, path, size));
        }
    }

    // Second pass: emergency pruning when still over the size limit
    for (_, path, size) in remaining {
        if total <= max_wal_bytes {
            break;
        }
        warn!(
            "EMERGENCY WAL PRUNING: {:?} (current: {total}B > limit: {max_wal_bytes}B)",
            path
        );
        if let Err(e) = std::fs::remove_file(&path) {
            warn!("Failed to emergency-delete WAL {:?}: {e}", path);
        } else {
            total = total.saturating_sub(size);
        }
    }

    Ok(())
}

/// Iterates registered indexes that declare a `retention` policy and sends
/// a [`PurgeCommand`] to the indexer actor for each one.
async fn apply_retention(registry: &IndexRegistry, purge_tx: &mpsc::Sender<PurgeCommand>) {
    for def in registry.list() {
        let Some(retention_str) = &def.retention else {
            continue;
        };
        let Some(duration) = parse_retention_duration(retention_str) else {
            warn!(
                "Index '{}' has invalid retention value '{}' — skipping.",
                def.name, retention_str
            );
            continue;
        };

        let cutoff = chrono::Utc::now() - duration;

        info!(
            "Retention: scheduling purge for '{}' (policy: {retention_str}, cutoff: {cutoff}).",
            def.name
        );

        if let Err(e) = purge_tx
            .send(PurgeCommand {
                index_name: def.name.clone(),
                cutoff,
            })
            .await
        {
            warn!("Failed to send purge command for '{}': {e}", def.name);
        }
    }
}

/// Background worker that enforces retention policies and monitors disk usage.
/// Runs every 5 minutes.
pub async fn run_retention_worker(
    data_dir: PathBuf,
    config: RetentionConfig,
    registry: IndexRegistry,
    purge_tx: mpsc::Sender<PurgeCommand>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
    loop {
        interval.tick().await;
        info!("Running retention checks...");

        // 1. Enforce per-index retention policies via the indexer actor
        apply_retention(&registry, &purge_tx).await;

        // 2. Monitor total index size
        let index_size = dir_size(&data_dir.join("indexes"));
        if index_size > config.max_index_bytes {
            warn!(
                "Index size {index_size}B exceeds limit {}B.",
                config.max_index_bytes
            );
        } else {
            info!("Index size: {index_size}B / {}B.", config.max_index_bytes);
        }

        // 3. Clean up old WAL files
        // TODO: derive committed_offset from Tantivy segment metadata on startup
        let committed_offset: u64 = 0;
        if let Err(e) = cleanup_wals(&data_dir, committed_offset, config.max_wal_bytes) {
            warn!("WAL cleanup failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_dir_size() {
        let dir = TempDir::new().unwrap();
        File::create(dir.path().join("a.txt"))
            .unwrap()
            .write_all(&[0u8; 1024])
            .unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        File::create(sub.join("b.txt"))
            .unwrap()
            .write_all(&[0u8; 2048])
            .unwrap();
        assert_eq!(dir_size(dir.path()), 3072);
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
        assert_eq!(
            parse_retention_duration("5y"),
            Some(chrono::Duration::days(1825))
        );
        assert_eq!(parse_retention_duration("invalid"), None);
        assert_eq!(parse_retention_duration(""), None);
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
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();

        File::create(p.join("0000000000000000.wal"))
            .unwrap()
            .write_all(&[0u8; 100])
            .unwrap();
        File::create(p.join("0000000000000064.wal"))
            .unwrap()
            .write_all(&[0u8; 50])
            .unwrap();
        File::create(p.join("0000000000000096.wal"))
            .unwrap()
            .write_all(&[0u8; 200])
            .unwrap();
        File::create(p.join("other.txt"))
            .unwrap()
            .write_all(&[0u8; 100])
            .unwrap();

        // 0.wal ends at 100, 100.wal ends at 150 — both <= committed=150 → deleted
        // 150.wal ends at 350 — > 150 → kept
        cleanup_wals(&p, 150, 1000).unwrap();

        assert!(!p.join("0000000000000000.wal").exists());
        assert!(!p.join("0000000000000064.wal").exists());
        assert!(p.join("0000000000000096.wal").exists());
        assert!(p.join("other.txt").exists());
    }

    #[test]
    fn test_emergency_wal_pruning() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();

        File::create(p.join("0000000000000000.wal"))
            .unwrap()
            .write_all(&[0u8; 100])
            .unwrap();
        File::create(p.join("0000000000000064.wal"))
            .unwrap()
            .write_all(&[0u8; 200])
            .unwrap();
        File::create(p.join("000000000000012c.wal"))
            .unwrap()
            .write_all(&[0u8; 300])
            .unwrap();

        // Total 600B, nothing committed, limit 400B
        // Deletes oldest first until within limit
        cleanup_wals(&p, 0, 400).unwrap();

        assert!(!p.join("0000000000000000.wal").exists());
        assert!(!p.join("0000000000000064.wal").exists());
        assert!(p.join("000000000000012c.wal").exists());
    }
}
