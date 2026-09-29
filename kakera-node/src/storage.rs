///persistent storage for save files
use axum::{
    body::Bytes,
    extract::{Path, State},
    handler::HandlerWithoutStateExt,
    http::{
        HeaderMap, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use serde::Deserialize;
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

#[derive(Debug, Deserialize)]
struct ManifestId {
    snapshot_id: String,
    vn_sync_id: String,
}

///stores a snapshot manifest after checking its id
pub async fn put_manifest(
    State(state): State<AppState>,
    Path((vn_sync_id, snapshot_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_authorized(&headers, &state.access_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if !is_valid_sync_id(&vn_sync_id) || !is_valid_hash(&snapshot_id) {
        return (
            StatusCode::BAD_REQUEST,
            "The VN sync ID or snapshot ID is invalid",
        )
            .into_response();
    }

    let id = match serde_json::from_slice::<ManifestId>(&body) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "The request body is not a valid snapshot manifest",
            )
                .into_response();
        }
    };

    if id.vn_sync_id != vn_sync_id || id.snapshot_id != snapshot_id {
        return (
            StatusCode::BAD_REQUEST,
            "The manifest id does not match the request path",
        )
            .into_response();
    }

    match store_manifest(&state.data_directory, &id, &body).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            eprintln!("Could not store manifest: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
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

///retrieves one snapshot manifest from storage
pub async fn get_manifest(
    State(state): State<AppState>,
    Path((vn_sync_id, snapshot_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if !is_authorized(&headers, &state.access_token) {
        return (
            StatusCode::BAD_REQUEST,
            "The VN sync ID or snapshot ID is invalid",
        )
            .into_response();
    }

    let manifest_path = state
        .data_directory
        .join("vns")
        .join(vn_sync_id)
        .join("snapshots")
        .join(format!("{snapshot_id}.json"));

    match tokio::fs::read(manifest_path).await {
        Ok(contents) => {
            ([(CONTENT_TYPE, "application/json")], Bytes::from(contents)).into_response()
        }

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }

        Err(error) => {
            eprintln!("Could not read manifest: {error}");
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

async fn store_manifest(
    data_directory: &PathBuf,
    id: &ManifestId,
    contents: &Bytes,
) -> io::Result<()> {
    let manifest_directory = data_directory
        .join("vns")
        .join(&id.vn_sync_id)
        .join("snapshots");

    tokio::fs::create_dir_all(&manifest_directory).await?;

    let manifest_path = manifest_directory.join(format!("{}.json", id.snapshot_id));

    tokio::fs::write(manifest_path, contents).await
}

async fn store_blob(data_directory: &PathBuf, hash: &String, contents: &Bytes) -> io::Result<()> {
    let blob_directory = data_directory.join("blobs");
    tokio::fs::create_dir_all(&blob_directory).await?;

    let blob_path = blob_directory.join(hash);
    tokio::fs::write(blob_path, contents).await
}

fn is_valid_sync_id(sync_id: &String) -> bool {
    match sync_id.strip_prefix("sync-") {
        Some(hash) => hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
        None => false,
    }
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
