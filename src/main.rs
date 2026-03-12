use edgewit::api::app_router;
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;
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

    // Bind to 0.0.0.0 to allow external access (essential for Docker)
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting Edgewit...");
    info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app_router())
        .await
        .expect("Server failed");
}
