///this is a self-hosted node which can be used on a homelab / server
///to sync kakera snapshots between devices, sorta more like steam's cloud save.
use axum::{Router, routing::get};
use tokio::net::TcpListener;

const NODE_ADDRESS: &str = "127.0.0.1:47840";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = Router::new().route("/health", get(health));

    let listener = TcpListener::bind(NODE_ADDRESS).await?;

    println!("Kakera node listening at http://{NODE_ADDRESS}");

    axum::serve(listener, app).await
}

async fn health() -> String {
    "ok".to_string()
}
