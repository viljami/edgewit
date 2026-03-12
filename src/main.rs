use edgewit::api::{AppState, app_router};
use edgewit::wal::WalAppender;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    // Initialize tracing for standard output logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default tracing subscriber failed");

    // Determine port, defaulting to 9200 (OpenSearch default)
    let port_str = env::var("EDGEWIT_PORT").unwrap_or_else(|_| "9200".to_string());
    let port: u16 = port_str
        .parse()
        .expect("EDGEWIT_PORT must be a valid u16 port number");

    let data_dir_str = env::var("EDGEWIT_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let data_dir = PathBuf::from(data_dir_str);

    // 1. Setup Tantivy Index
    let index = edgewit::indexer::setup_index(&data_dir).expect("Failed to setup Tantivy index");
    let mut writer = index
        .writer(30_000_000)
        .expect("Failed to create IndexWriter");

    // 2. Read WAL offset from the last Tantivy commit
    let metas = index.load_metas().unwrap();
    let last_offset: u64 = metas
        .payload
        .as_ref()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);

    info!("Index loaded. Last committed WAL offset: {}", last_offset);

    // 3. Synchronously Replay the WAL on Startup (Crash Recovery)
    let wal_path = data_dir.join("00000001.wal");
    let mut current_offset = last_offset;

    if wal_path.exists() {
        info!("Replaying WAL from offset {}...", last_offset);
        let mut reader = edgewit::wal::WalReader::new(&wal_path, last_offset)
            .expect("Failed to open WAL for replay");
        let mut replayed = 0;

        while let Some((event, next_offset)) = reader.next_frame().unwrap() {
            if let Err(e) = edgewit::indexer::add_to_index(&mut writer, &index.schema(), event) {
                tracing::error!("Failed to replay document: {}", e);
            }
            current_offset = next_offset;
            replayed += 1;
        }

        if replayed > 0 {
            info!(
                "Replayed {} events. Committing segment at offset {}.",
                replayed, current_offset
            );
            let mut commit = writer.prepare_commit().unwrap();
            commit.set_payload(&current_offset.to_string());
            commit.commit().unwrap();
        } else {
            info!("No new events to replay from WAL.");
        }
    }

    // 4. Initialize Channels
    let (wal_tx, wal_rx) = mpsc::channel(10000); // Buffer up to 10k requests in memory
    let (idx_tx, idx_rx) = mpsc::channel(10000);

    // 5. Spawn the Indexer Thread
    let indexer = edgewit::indexer::IndexerActor::new(writer, index.schema(), idx_rx);
    tokio::spawn(async move {
        indexer.run().await;
    });

    // 6. Spawn the WAL Thread
    let wal_appender = WalAppender::new(data_dir, wal_rx, idx_tx, current_offset);
    tokio::task::spawn_blocking(move || {
        wal_appender.run();
    });

    let state = AppState { wal_sender: wal_tx };

    // Bind to 0.0.0.0 to allow external access (essential for Docker)
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting Edgewit...");
    info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app_router(state))
        .await
        .expect("Server failed");
}
