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

    // Initialize the WAL channel and appender thread
    let (wal_tx, wal_rx) = mpsc::channel(10000); // Buffer up to 10k requests in memory
    let data_dir_str = env::var("EDGEWIT_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let data_dir = PathBuf::from(data_dir_str);

    let wal_appender = WalAppender::new(data_dir, wal_rx);
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
