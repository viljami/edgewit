use crc32fast::Hasher;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

/// Represents a single event to be appended to the WAL.
#[derive(Debug)]
pub struct IngestEvent {
    pub index: String,
    pub payload: Vec<u8>,
}

/// A request sent to the WAL worker thread.
/// It includes a `oneshot::Sender` so the HTTP API can wait for fsync confirmation.
pub struct WalRequest {
    pub event: IngestEvent,
    pub responder: oneshot::Sender<Result<(), String>>,
}

/// The WAL Appender is responsible for sequentially writing events to disk
/// and calling fsync. It runs in its own blocking thread to prevent
/// stalling the async HTTP runtime.
pub struct WalAppender {
    receiver: mpsc::Receiver<WalRequest>,
    dir: PathBuf,
}

impl WalAppender {
    /// Creates a new WAL Appender.
    pub fn new(dir: impl AsRef<Path>, receiver: mpsc::Receiver<WalRequest>) -> Self {
        Self {
            receiver,
            dir: dir.as_ref().to_path_buf(),
        }
    }

    /// Starts the blocking event loop. This should be spawned via `tokio::task::spawn_blocking`
    /// or `std::thread::spawn`.
    pub fn run(mut self) {
        // Ensure the directory exists
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            error!("Failed to create WAL directory at {:?}: {}", self.dir, e);
            return;
        }

        // For M1, we use a single hardcoded WAL file. In future milestones (M5),
        // we will implement file rotation and segment compaction.
        let wal_path = self.dir.join("00000001.wal");
        let file = match OpenOptions::new().create(true).append(true).open(&wal_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open WAL file {:?}: {}", wal_path, e);
                return;
            }
        };

        // We use a small buffer. Since we explicitly flush and sync_data() at the end
        // of every batch, this just helps reduce syscall overhead during the batch write.
        let mut writer = BufWriter::with_capacity(64 * 1024, file);

        info!("WAL appender thread started at {:?}", wal_path);

        // Blocking loop: waits for at least one request.
        while let Some(req) = self.receiver.blocking_recv() {
            let mut batch = vec![req];

            // ADAPTIVE BATCHING:
            // If the channel has a backlog (e.g., during a spike in HTTP requests),
            // drain everything immediately available into this single batch.
            // This is the secret to high throughput (5k/sec) on slow SD cards,
            // as it groups many small events into a single fsync.
            while let Ok(next_req) = self.receiver.try_recv() {
                batch.push(next_req);
                // Arbitrary limit to prevent infinite blocking on massive sustained load
                if batch.len() >= 5000 {
                    break;
                }
            }

            debug!("WAL processing batch of size {}", batch.len());

            let mut batch_success = true;

            // Write all events in the batch to the buffer
            for req in &batch {
                let index_bytes = req.event.index.as_bytes();
                let payload_bytes = &req.event.payload;

                // Calculate CRC32 for corruption detection on unreliable hardware
                let mut hasher = Hasher::new();
                hasher.update(index_bytes);
                hasher.update(payload_bytes);
                let checksum = hasher.finalize();

                // Simple binary framing:
                // [Index Length: u16] [Index Bytes] [Payload Length: u32] [Payload Bytes] [CRC32: u32]
                let mut frame_success = true;

                if let Err(e) = writer.write_all(&(index_bytes.len() as u16).to_le_bytes()) {
                    error!("WAL failed to write index length: {}", e);
                    frame_success = false;
                }
                if frame_success && writer.write_all(index_bytes).is_err() {
                    error!("WAL failed to write index bytes");
                    frame_success = false;
                }
                if frame_success
                    && writer
                        .write_all(&(payload_bytes.len() as u32).to_le_bytes())
                        .is_err()
                {
                    error!("WAL failed to write payload length");
                    frame_success = false;
                }
                if frame_success && writer.write_all(payload_bytes).is_err() {
                    error!("WAL failed to write payload bytes");
                    frame_success = false;
                }
                if frame_success && writer.write_all(&checksum.to_le_bytes()).is_err() {
                    error!("WAL failed to write checksum");
                    frame_success = false;
                }

                if !frame_success {
                    batch_success = false;
                    break; // Stop writing the batch if the disk is failing
                }
            }

            // Flush the userspace buffer to the OS
            if batch_success {
                if let Err(e) = writer.flush() {
                    error!("WAL flush error: {}", e);
                    batch_success = false;
                }
            }

            // Sync the OS buffer to the physical disk (fsync)
            // This guarantees durability. If the Pi loses power after this returns,
            // the data is safe.
            if batch_success {
                if let Err(e) = writer.get_ref().sync_data() {
                    error!("WAL sync_data error: {}", e);
                    batch_success = false;
                }
            }

            // Respond to all waiting HTTP requests
            for req in batch {
                let res = if batch_success {
                    Ok(())
                } else {
                    Err("Failed to safely write to WAL".to_string())
                };

                // It's possible the HTTP client disconnected before we finished writing.
                // We ignore the Result here because the data is safely persisted regardless.
                let _ = req.responder.send(res);
            }
        }

        info!("WAL appender thread shutting down.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    async fn setup_wal() -> (TempDir, mpsc::Sender<WalRequest>) {
        let temp_dir = TempDir::new().unwrap();
        let (tx, rx) = mpsc::channel(100);
        let appender = WalAppender::new(temp_dir.path(), rx);

        tokio::task::spawn_blocking(move || {
            appender.run();
        });

        (temp_dir, tx)
    }

    #[tokio::test]
    async fn test_single_append_success() {
        let (_dir, tx) = setup_wal().await;
        let (resp_tx, resp_rx) = oneshot::channel();

        tx.send(WalRequest {
            event: IngestEvent {
                index: "test_index".to_string(),
                payload: b"hello world".to_vec(),
            },
            responder: resp_tx,
        })
        .await
        .unwrap();

        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wal_framing_and_durability() {
        let (dir, tx) = setup_wal().await;

        // Send 2 messages to test adaptive batching / sequential writes
        let (rtx1, rrx1) = oneshot::channel();
        let (rtx2, rrx2) = oneshot::channel();

        tx.send(WalRequest {
            event: IngestEvent {
                index: "idx1".to_string(),
                payload: b"data1".to_vec(),
            },
            responder: rtx1,
        })
        .await
        .unwrap();

        tx.send(WalRequest {
            event: IngestEvent {
                index: "idx2".to_string(),
                payload: b"data2".to_vec(),
            },
            responder: rtx2,
        })
        .await
        .unwrap();

        // Wait for both to be synced to disk
        rrx1.await.unwrap().unwrap();
        rrx2.await.unwrap().unwrap();

        // Open the raw file to verify the binary framing
        let file_path = dir.path().join("00000001.wal");
        let mut file = std::fs::File::open(file_path).expect("File must exist");

        // Helper closure to read a single frame
        let mut read_frame = || -> Option<(String, Vec<u8>, u32)> {
            let mut len_buf = [0u8; 2];
            if file.read_exact(&mut len_buf).is_err() {
                return None;
            }
            let idx_len = u16::from_le_bytes(len_buf) as usize;

            let mut idx_buf = vec![0u8; idx_len];
            file.read_exact(&mut idx_buf).unwrap();
            let index = String::from_utf8(idx_buf).unwrap();

            let mut plen_buf = [0u8; 4];
            file.read_exact(&mut plen_buf).unwrap();
            let p_len = u32::from_le_bytes(plen_buf) as usize;

            let mut payload = vec![0u8; p_len];
            file.read_exact(&mut payload).unwrap();

            let mut crc_buf = [0u8; 4];
            file.read_exact(&mut crc_buf).unwrap();
            let crc = u32::from_le_bytes(crc_buf);

            Some((index, payload, crc))
        };

        // Validate Frame 1
        let (idx1, payload1, crc1) = read_frame().expect("First frame missing");
        assert_eq!(idx1, "idx1");
        assert_eq!(payload1, b"data1");

        let mut hasher = Hasher::new();
        hasher.update(idx1.as_bytes());
        hasher.update(&payload1);
        assert_eq!(hasher.finalize(), crc1, "Checksum mismatch for frame 1");

        // Validate Frame 2
        let (idx2, payload2, crc2) = read_frame().expect("Second frame missing");
        assert_eq!(idx2, "idx2");
        assert_eq!(payload2, b"data2");

        let mut hasher2 = Hasher::new();
        hasher2.update(idx2.as_bytes());
        hasher2.update(&payload2);
        assert_eq!(hasher2.finalize(), crc2, "Checksum mismatch for frame 2");

        // EOF check
        assert!(read_frame().is_none(), "Unexpected trailing data");
    }
}
