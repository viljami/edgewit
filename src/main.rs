use edgewit::api::{AppState, app_router};
use edgewit::wal::WalAppender;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for standard output logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Determine port, defaulting to 9200 (OpenSearch default)
    let port_str = env::var("EDGEWIT_PORT").unwrap_or_else(|_| "9200".to_string());
    let port: u16 = port_str
        .parse()
        .map_err(|_| "EDGEWIT_PORT must be a valid u16 port number")?;

    let data_dir_str = env::var("EDGEWIT_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let data_dir = PathBuf::from(data_dir_str);

    let index_memory_mb: usize = env::var("EDGEWIT_INDEX_MEMORY_MB")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_INDEX_MEMORY_MB must be a valid usize")?;

    info!("Using {}MB memory budget for indexing", index_memory_mb);

    let channel_buffer: usize = env::var("EDGEWIT_CHANNEL_BUFFER")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_CHANNEL_BUFFER must be a valid usize")?;

    let search_threads: usize = env::var("EDGEWIT_SEARCH_THREADS")
        .unwrap_or_else(|_| "1".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_SEARCH_THREADS must be a valid usize")?;

    let docstore_cache_blocks: usize = env::var("EDGEWIT_DOCSTORE_CACHE_BLOCKS")
        .unwrap_or_else(|_| "20".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_DOCSTORE_CACHE_BLOCKS must be a valid usize")?;

    let merge_min_segments: usize = env::var("EDGEWIT_MERGE_MIN_SEGMENTS")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_MERGE_MIN_SEGMENTS must be a valid usize")?;

    rayon::ThreadPoolBuilder::new()
        .num_threads(search_threads)
        .build_global()?;

    // 1. Setup Tantivy Index
    let index = edgewit::indexer::setup_index(&data_dir)?;
    let mut writer = index.writer(index_memory_mb * 1_000_000)?;

    let mut merge_policy = tantivy::merge_policy::LogMergePolicy::default();
    merge_policy.set_min_num_segments(merge_min_segments);
    writer.set_merge_policy(Box::new(merge_policy));

    // 2. Read WAL offset from the last Tantivy commit
    let metas = index.load_metas()?;
    let last_offset: u64 = metas
        .payload
        .as_ref()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);

    info!("Index loaded. Last committed WAL offset: {}", last_offset);

    // 3. Synchronously Replay the WAL on Startup (Crash Recovery)
    let mut current_offset = last_offset;

    let mut wal_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("wal")
                && let Some(file_stem) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(start_offset) = u64::from_str_radix(file_stem, 16)
            {
                wal_files.push((start_offset, path, entry.metadata()?.len()));
            }
        }
    }

    wal_files.sort_by_key(|(offset, _, _)| *offset);

    let mut replayed = 0;

    for (start_offset, wal_path, size) in wal_files {
        let end_offset = start_offset + size;
        if end_offset > current_offset {
            let read_start_file_offset = current_offset.saturating_sub(start_offset);

            info!(
                "Replaying WAL file {:?} from file offset {}...",
                wal_path, read_start_file_offset
            );
            if let Ok(mut reader) = edgewit::wal::WalReader::new(&wal_path, read_start_file_offset)
            {
                reader.current_offset = current_offset;
                while let Ok(Some((event, next_offset))) = reader.next_frame() {
                    if let Err(e) =
                        edgewit::indexer::add_to_index(&mut writer, &index.schema(), event)
                    {
                        tracing::error!("Failed to replay document: {}", e);
                    }
                    current_offset = next_offset;
                    replayed += 1;
                }
            }
        }
    }

    if replayed > 0 {
        info!(
            "Replayed {} events. Committing segment at offset {}.",
            replayed, current_offset
        );
        let mut commit = writer.prepare_commit()?;
        commit.set_payload(&current_offset.to_string());
        commit.commit()?;
    } else {
        info!("No new events to replay from WAL.");
    }

    // 4. Initialize Channels
    let (wal_tx, wal_rx) = mpsc::channel(channel_buffer); // Buffer configurable requests in memory
    let (idx_tx, idx_rx) = mpsc::channel(channel_buffer);

    // 5. Spawn the Indexer Thread
    let indexer = edgewit::indexer::IndexerActor::new(writer, index.schema(), idx_rx);
    tokio::spawn(async move {
        indexer.run().await;
    });

    // 6. Spawn the WAL Thread
    let wal_appender = WalAppender::new(data_dir.clone(), wal_rx, idx_tx, current_offset);
    tokio::task::spawn_blocking(move || {
        wal_appender.run();
    });

    // 7. Spawn Retention Worker
    let retention_config = edgewit::retention::RetentionConfig::from_env();
    let retention_index = index.clone();
    let retention_data_dir = data_dir.clone();
    tokio::spawn(async move {
        edgewit::retention::run_compaction_and_retention_worker(
            retention_data_dir,
            retention_index,
            retention_config,
        )
        .await;
    });

    let index_reader = index
        .reader_builder()
        .doc_store_cache_num_blocks(docstore_cache_blocks)
        .try_into()?;

    info!(
        "Search engine configured with {} threads and {} cache blocks",
        search_threads, docstore_cache_blocks
    );

    let prometheus_handle =
        metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder()?;

    let state = AppState {
        prometheus_handle,
        wal_sender: wal_tx,
        index_reader,
    };

    // Bind to 0.0.0.0 to allow external access (essential for Docker)
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting Edgewit...");
    info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app_router(state)).await?;

    Ok(())
}
