///this is a self-hosted node which can be used on a homelab / server
///to sync kakera snapshots between devices, sorta more like steam's cloud save.
mod config;
mod storage;

use axum::{Router, extract::DefaultBodyLimit, routing::get};
use config::NodeConfig;
use storage::{AppState, get_blob, put_blob};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = NodeConfig::from_env()?;

    tokio::fs::create_dir_all(&config.data_directory).await?;

    let state = AppState::new(config.data_directory.clone(), config.access_token.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/blobs/{hash}", get(get_blob).put(put_blob))
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
        .with_state(state);
    let listener = TcpListener::bind(&config.address).await?;

    println!("Kakera node listening at http://{}", config.address);
    println!("Data directory: {}", config.data_directory.display());

    axum::serve(listener, app).await
}

async fn health() -> String {
    "ok".to_string()
}
