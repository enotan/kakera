///this is a self-hosted node which can be used on a homelab / server
///to sync kakera snapshots between devices, sorta more like steam's cloud save.
mod config;
use axum::{Router, routing::get};
use config::NodeConfig;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = NodeConfig::from_env()?;

    tokio::fs::create_dir_all(&config.data_directory).await?;

    let app = Router::new().route("/health", get(health));
    let listener = TcpListener::bind(&config.address).await?;

    println!("Kakera node listening at http://{}", config.address);
    println!("Data directory: {}", config.data_directory.display());

    axum::serve(listener, app).await
}

async fn health() -> String {
    "ok".to_string()
}
