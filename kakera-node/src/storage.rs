///persistent storage for save files
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{
        HeaderMap, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use std::{io, path::PathBuf};

///shared info available to every HTTP request handler
#[derive(Debug, Clone)]
pub struct AppState {
    data_directory: PathBuf,
    access_token: String,
}

impl AppState {
    /// creates the shared server state
    pub fn new(data_directory: PathBuf, access_token: String) -> Self {
        Self {
            data_directory,
            access_token,
        }
    }
}

///stores a blob after verifying the hash
pub async fn put_blob(
    State(state): State<AppState>,
    Path(expected_hash): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_authorized(&headers, &state.access_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if !is_valid_hash(&expected_hash) {
        return (
            StatusCode::BAD_REQUEST,
            "The blob hash must be 64 hexadecimal characters",
        )
            .into_response();
    }

    let actual_hash = blake3::hash(&body).to_hex().to_string();

    if actual_hash != expected_hash {
        return (
            StatusCode::BAD_REQUEST,
            "The request body doesn't match the supplied hash",
        )
            .into_response();
    }

    match store_blob(&state.data_directory, &expected_hash, &body).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            eprintln!("Could not store blob: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

///retrieves one blob from persistent storage
pub async fn get_blob(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !is_authorized(&headers, &state.access_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if !is_valid_hash(&hash) {
        return (
            StatusCode::BAD_REQUEST,
            "The blob hash must be 64 hexadecimal characters",
        )
            .into_response();
    }

    let blob_path = state.data_directory.join("blobs").join(hash);

    match tokio::fs::read(blob_path).await {
        Ok(contents) => (
            [(CONTENT_TYPE, "application/octet-stream")],
            Bytes::from(contents),
        )
            .into_response(),

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }

        Err(error) => {
            eprintln!("Could not read blob: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn store_blob(data_directory: &PathBuf, hash: &String, contents: &Bytes) -> io::Result<()> {
    let blob_directory = data_directory.join("blobs");
    tokio::fs::create_dir_all(&blob_directory).await?;

    let blob_path = blob_directory.join(hash);
    tokio::fs::write(blob_path, contents).await
}

fn is_authorized(headers: &HeaderMap, expected_token: &String) -> bool {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        == Some(expected_token.as_str())
}

fn is_valid_hash(hash: &String) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}
