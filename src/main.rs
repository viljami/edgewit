use edgewit::api::{AppState, app_router};
use edgewit::index_manager::IndexManager;
use edgewit::registry::IndexRegistry;
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

    let registry = IndexRegistry::new();
    let indexes_dir = data_dir.join("indexes");
    match registry.load_from_dir(&indexes_dir) {
        Ok(count) => {
            info!("Loaded {} index definitions from {:?}", count, indexes_dir);
        }
        Err(e) => {
            tracing::error!("Startup failed: error loading index definitions: {}", e);
            return Err(e.into());
        }
    }

    // 1. Setup Index Manager
    let index_manager =
        IndexManager::new(data_dir.clone(), registry.clone(), docstore_cache_blocks);

    // 2. Read WAL offset from the last Tantivy commit
    // TODO: Implement proper WAL recovery reading from partition meta.jsons
    let last_offset: u64 = 0;
    let current_offset = last_offset;

    info!(
        "Index Manager initialized. Starting WAL replay at offset: {}",
        last_offset
    );

    // 3. Synchronously Replay the WAL on Startup (Crash Recovery)
    // TODO: Restore synchronous WAL replay using the IndexManager
    info!("Skipping synchronous WAL replay for now (to be implemented with new IndexManager)");

    // 4. Initialize Channels
    let (wal_tx, wal_rx) = mpsc::channel(channel_buffer); // Buffer configurable requests in memory
    let (idx_tx, idx_rx) = mpsc::channel(channel_buffer);

    // 5. Spawn the Indexer Thread
    let indexer = edgewit::indexer::IndexerActor::new(
        index_manager.clone(),
        registry.clone(),
        idx_rx,
        index_memory_mb,
    );
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
    let retention_data_dir = data_dir.clone();
    let retention_registry = registry.clone();
    tokio::spawn(async move {
        edgewit::retention::run_compaction_and_retention_worker(
            retention_data_dir,
            retention_config,
            retention_registry,
        )
        .await;
    });

    // 8. Spawn Compaction Worker
    let compaction_worker = edgewit::compaction::CompactionWorker::new(data_dir.clone());
    tokio::spawn(async move {
        compaction_worker.run().await;
    });

    info!(
        "Search engine configured with {} threads and {} cache blocks",
        search_threads, docstore_cache_blocks
    );

    let prometheus_handle =
        metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder()?;

    let state = AppState {
        prometheus_handle,
        wal_sender: wal_tx,
        index_manager,
        registry,
        data_dir: data_dir.clone(),
    };

    // Bind to 0.0.0.0 to allow external access (essential for Docker)
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting Edgewit...");
    info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app_router(state)).await?;

    Ok(())
}
