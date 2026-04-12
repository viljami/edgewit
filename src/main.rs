use edgewit::api::{AppState, app_router};
use edgewit::index_manager::IndexManager;
use edgewit::indexer::PurgeCommand;
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
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let port: u16 = env::var("EDGEWIT_PORT")
        .unwrap_or_else(|_| "9200".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_PORT must be a valid u16")?;

    let data_dir =
        PathBuf::from(env::var("EDGEWIT_DATA_DIR").unwrap_or_else(|_| "./data".to_string()));

    let index_memory_mb: usize = env::var("EDGEWIT_INDEX_MEMORY_MB")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .map_err(|_| "EDGEWIT_INDEX_MEMORY_MB must be a valid usize")?;

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

    rayon::ThreadPoolBuilder::new()
        .num_threads(search_threads)
        .build_global()?;

    // Load index definitions persisted as .index.yaml files
    let registry = IndexRegistry::new();
    let indexes_dir = data_dir.join("indexes");
    let count = registry.load_from_dir(&indexes_dir)?;
    info!("Loaded {count} index definitions from {indexes_dir:?}.");
    info!("Using {index_memory_mb}MB memory budget for indexing.");

    let index_manager =
        IndexManager::new(data_dir.clone(), registry.clone(), docstore_cache_blocks);

    let (wal_tx, wal_rx) = mpsc::channel(channel_buffer);
    let (idx_tx, idx_rx) = mpsc::channel(channel_buffer);
    let (purge_tx, purge_rx) = mpsc::channel::<PurgeCommand>(32);

    // Indexer actor — owns all IndexWriters and processes both ingest events and purge commands
    let indexer = edgewit::indexer::IndexerActor::new(
        index_manager.clone(),
        registry.clone(),
        idx_rx,
        index_memory_mb,
        purge_rx,
    );
    tokio::spawn(async move { indexer.run().await });

    // WAL appender (blocking thread)
    let wal_appender = WalAppender::new(data_dir.clone(), wal_rx, idx_tx, 0);
    tokio::task::spawn_blocking(move || wal_appender.run());

    // Retention worker — monitors disk usage and forwards purge commands to the indexer
    let retention_config = edgewit::retention::RetentionConfig::from_env();
    tokio::spawn(edgewit::retention::run_retention_worker(
        data_dir.clone(),
        retention_config,
        registry.clone(),
        purge_tx,
    ));

    // Compaction worker — periodically merges Tantivy segments
    tokio::spawn(edgewit::compaction::CompactionWorker::new(data_dir.clone()).run());

    let prometheus_handle =
        metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder()?;

    let state = AppState {
        prometheus_handle,
        wal_sender: wal_tx,
        index_manager,
        registry,
        data_dir,
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {addr}.");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app_router(state)).await?;
    Ok(())
}
