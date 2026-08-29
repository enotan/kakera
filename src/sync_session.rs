use crate::{
    save_sync::{
        SnapshotManifest, VerifiedLocalBlob, list_local_snapshots_for_vn,
        persist_received_snapshot_manifest, verified_local_blob_for_vn,
    },
    sync_protocol::{
        PeerHello, SYNC_PROTOCOL_VERSION, SnapshotDescriptor, SyncMessage, SyncProtocolErrorCode,
    },
    sync_transport::{
        receive_blob_bytes, receive_control_message, send_blob_bytes, send_control_message,
    },
};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use std::{io, path::PathBuf};

///a successful blobready response, and the file it describes
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedBlobResponse {
    pub ready_message: SyncMessage,
    pub blob: VerifiedLocalBlob,
}

///identifies one completed outgoing snapshot transfer
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedPeerTransfer {
    pub vn_sync_id: String,
    pub snapshot_id: String,
}

///summarises a snapshot successfully received from another device
#[derive(Debug, Clone, PartialEq)]
pub struct ReceivedPeerSnapshot {
    pub manifest: SnapshotManifest,
    pub downloaded_blob_count: usize,
}

///validate a blob request and prepare its control response
pub fn prepare_blob_response(
    request: SyncMessage,
    kakera_data_dir: PathBuf,
) -> Result<PreparedBlobResponse, SyncMessage> {
    let (vn_sync_id, content_hash) = match request {
        SyncMessage::BlobRequest {
            vn_sync_id,
            content_hash,
        } => (vn_sync_id, content_hash),

        _ => {
            return Err(SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "Expected a blob request".to_string(),
            });
        }
    };

    let blob = match verified_local_blob_for_vn(vn_sync_id, content_hash, kakera_data_dir) {
        Ok(blob) => blob,

        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            return Err(SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "The blob request is invalid".to_string(),
            });
        }

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(SyncMessage::Error {
                code: SyncProtocolErrorCode::NotFound,
                message: "The requested blob does not exist".to_string(),
            });
        }

        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return Err(SyncMessage::Error {
                code: SyncProtocolErrorCode::IntegrityFailure,
                message: "The requested blob failed verification".to_string(),
            });
        }

        Err(_) => {
            return Err(SyncMessage::Error {
                code: SyncProtocolErrorCode::Internal,
                message: "The requested blob could not be opened".to_string(),
            });
        }
    };

    let ready_message = SyncMessage::BlobReady {
        content_hash: blob.content_hash.clone(),
        size_bytes: blob.size_bytes,
    };

    Ok(PreparedBlobResponse {
        ready_message,
        blob,
    })
}

///serves snapshot requests over one authenticated peer connection
pub async fn serve_sync_connection(
    connection: Connection,
    local_hello: PeerHello,
    kakera_data_dir: PathBuf,
) -> Result<CompletedPeerTransfer, io::Error> {
    let (mut send_stream, mut receive_stream) =
        connection.accept_bi().await.map_err(io::Error::other)?;

    let peer_hello = receive_control_message(&mut receive_stream).await?;

    match peer_hello {
        SyncMessage::Hello(hello) if hello.protocol_version == SYNC_PROTOCOL_VERSION => {}

        SyncMessage::Hello(_) => {
            let error = SyncMessage::Error {
                code: SyncProtocolErrorCode::UnsupportedVersion,
                message: "The peer uses an unsupported sync protocol".to_string(),
            };

            send_control_message(&mut send_stream, &error).await?;

            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer uses an unsupported sync protocol",
            ));
        }

        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer did not begin with a hello message",
            ));
        }
    }

    send_control_message(&mut send_stream, &SyncMessage::Hello(local_hello)).await?;

    let mut offered_snapshot: Option<(String, String)> = None;

    loop {
        let request = receive_control_message(&mut receive_stream).await?;

        match request {
            request @ SyncMessage::SnapshotInventoryRequest { .. } => {
                let response = build_inventory_response(request, kakera_data_dir.clone());

                send_control_message(&mut send_stream, &response).await?;
            }

            request @ SyncMessage::ManifestRequest { .. } => {
                let response = build_manifest_response(request, kakera_data_dir.clone());

                if let SyncMessage::Manifest { manifest } = &response {
                    offered_snapshot =
                        Some((manifest.vn_sync_id.clone(), manifest.snapshot_id.clone()));
                }

                send_control_message(&mut send_stream, &response).await?;
            }

            request @ SyncMessage::BlobRequest { .. } => {
                match prepare_blob_response(request, kakera_data_dir.clone()) {
                    Ok(prepared) => {
                        send_control_message(&mut send_stream, &prepared.ready_message).await?;

                        send_blob_bytes(&mut send_stream, &prepared.blob).await?;
                    }

                    Err(error_message) => {
                        send_control_message(&mut send_stream, &error_message).await?;
                    }
                }
            }

            SyncMessage::TransferComplete {
                vn_sync_id,
                snapshot_id,
            } => {
                let completion_matches = match &offered_snapshot {
                    Some((offered_vn_id, offered_snapshot_id)) => {
                        offered_vn_id == &vn_sync_id && offered_snapshot_id == &snapshot_id
                    }
                    None => false,
                };

                if !completion_matches {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "The peer confirmed a snapshot that was not offered",
                    ));
                }

                send_stream.finish().map_err(io::Error::other)?;

                receive_stream
                    .read_to_end(0)
                    .await
                    .map_err(io::Error::other)?;

                let stop_code = send_stream.stopped().await.map_err(io::Error::other)?;

                if stop_code.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "The peer stopped the sync response stream",
                    ));
                }

                return Ok(CompletedPeerTransfer {
                    vn_sync_id,
                    snapshot_id,
                });
            }

            _ => {
                let error = SyncMessage::Error {
                    code: SyncProtocolErrorCode::InvalidRequest,
                    message: "The peer sent an unexpected message".to_string(),
                };

                send_control_message(&mut send_stream, &error).await?;
            }
        }
    }
}

///requests the newest snapshot manifest advertised by a peer
async fn request_latest_peer_manifest(
    send_stream: &mut SendStream,
    receive_stream: &mut RecvStream,
    vn_sync_id: String,
) -> Result<SnapshotManifest, io::Error> {
    let inventory_request = SyncMessage::SnapshotInventoryRequest {
        vn_sync_id: vn_sync_id.clone(),
    };

    send_control_message(send_stream, &inventory_request).await?;

    let inventory_response = receive_control_message(receive_stream).await?;

    let snapshots = match inventory_response {
        SyncMessage::SnapshotInventory {
            vn_sync_id: returned_vn_id,
            snapshots,
        } if returned_vn_id == vn_sync_id => snapshots,

        SyncMessage::Error { message, .. } => {
            return Err(io::Error::other(format!(
                "The peer rejected the inventory request: {message}"
            )));
        }

        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer returned an unexpected snapshot inventory",
            ));
        }
    };

    let newest_snapshot = snapshots.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "The peer has no snapshots for this vn",
        )
    })?;

    let manifest_request = SyncMessage::ManifestRequest {
        vn_sync_id: vn_sync_id.clone(),
        snapshot_id: newest_snapshot.snapshot_id.clone(),
    };

    send_control_message(send_stream, &manifest_request).await?;

    let manifest_response = receive_control_message(receive_stream).await?;

    let manifest = match manifest_response {
        SyncMessage::Manifest { manifest } => manifest,

        SyncMessage::Error { message, .. } => {
            return Err(io::Error::other(format!(
                "The peer rejected the manifest request: {message}"
            )));
        }

        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer returned an unexpected manifest response",
            ));
        }
    };

    if manifest.vn_sync_id != vn_sync_id || manifest.snapshot_id != newest_snapshot.snapshot_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The peer returned a different snapshot than the one requested",
        ));
    }

    Ok(manifest)
}

///downloads every missing blob referenced by a peer manifest
async fn receive_manifest_blobs(
    send_stream: &mut SendStream,
    receive_stream: &mut RecvStream,
    manifest: &SnapshotManifest,
    kakera_data_dir: PathBuf,
) -> Result<usize, io::Error> {
    let mut downloaded_blob_count = 0;

    for snapshot_file in &manifest.files {
        match verified_local_blob_for_vn(
            manifest.vn_sync_id.clone(),
            snapshot_file.content_hash.clone(),
            kakera_data_dir.clone(),
        ) {
            Ok(existing_blob) if existing_blob.size_bytes == snapshot_file.size_bytes => {
                continue;
            }

            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "An existing local blob has the wrong size",
                ));
            }

            Err(error) if error.kind() == io::ErrorKind::NotFound => {}

            Err(error) => return Err(error),
        }

        let request = SyncMessage::BlobRequest {
            vn_sync_id: manifest.vn_sync_id.clone(),
            content_hash: snapshot_file.content_hash.clone(),
        };

        send_control_message(send_stream, &request).await?;

        let response = receive_control_message(receive_stream).await?;

        let (announced_hash, announced_size) = match response {
            SyncMessage::BlobReady {
                content_hash,
                size_bytes,
            } => (content_hash, size_bytes),

            SyncMessage::Error { message, .. } => {
                return Err(io::Error::other(format!(
                    "The peer rejected a blob request: {message}"
                )));
            }

            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The peer returned an unexpected blob response",
                ));
            }
        };

        if announced_hash != snapshot_file.content_hash
            || announced_size != snapshot_file.size_bytes
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer announced different blob details than requested",
            ));
        }

        let blob_directory = kakera_data_dir
            .join("save-sync")
            .join("vns")
            .join(&manifest.vn_sync_id)
            .join("blobs");

        receive_blob_bytes(
            receive_stream,
            announced_hash,
            announced_size,
            blob_directory,
        )
        .await?;

        downloaded_blob_count += 1;
    }

    Ok(downloaded_blob_count)
}

///downloads and registers the newest snapshot available from a connected peer
pub async fn receive_latest_snapshot_from_peer(
    connection: Connection,
    local_hello: PeerHello,
    vn_sync_id: String,
    kakera_data_dir: PathBuf,
) -> Result<ReceivedPeerSnapshot, io::Error> {
    let (mut send_stream, mut receive_stream) =
        connection.open_bi().await.map_err(io::Error::other)?;

    send_control_message(&mut send_stream, &SyncMessage::Hello(local_hello)).await?;

    let peer_hello = receive_control_message(&mut receive_stream).await?;

    match peer_hello {
        SyncMessage::Hello(hello) if hello.protocol_version == SYNC_PROTOCOL_VERSION => {}

        SyncMessage::Hello(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer uses an unsupported sync protocol",
            ));
        }

        SyncMessage::Error { message, .. } => {
            return Err(io::Error::other(format!(
                "The peer rejected the sync session: {message}"
            )));
        }

        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer did not respond with a hello message",
            ));
        }
    }

    let manifest =
        request_latest_peer_manifest(&mut send_stream, &mut receive_stream, vn_sync_id.clone())
            .await?;

    let downloaded_blob_count = receive_manifest_blobs(
        &mut send_stream,
        &mut receive_stream,
        &manifest,
        kakera_data_dir.clone(),
    )
    .await?;

    persist_received_snapshot_manifest(manifest.clone(), kakera_data_dir)?;

    let completion = SyncMessage::TransferComplete {
        vn_sync_id,
        snapshot_id: manifest.snapshot_id.clone(),
    };

    send_control_message(&mut send_stream, &completion).await?;
    send_stream.finish().map_err(io::Error::other)?;

    receive_stream
        .read_to_end(0)
        .await
        .map_err(io::Error::other)?;

    let stop_code = send_stream.stopped().await.map_err(io::Error::other)?;

    if stop_code.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "The peer stopped the sync request stream",
        ));
    }

    Ok(ReceivedPeerSnapshot {
        manifest,
        downloaded_blob_count,
    })
}

///builds the response to a request
pub fn build_inventory_response(request: SyncMessage, kakera_data_dir: PathBuf) -> SyncMessage {
    let vn_sync_id = match request {
        SyncMessage::SnapshotInventoryRequest { vn_sync_id } => vn_sync_id,
        _ => {
            return SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "Expected a snapshot inventory request".to_string(),
            };
        }
    };

    let manifests = match list_local_snapshots_for_vn(vn_sync_id.clone(), kakera_data_dir) {
        Ok(manifests) => manifests,

        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            return SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "The VN sync ID is invalid".to_string(),
            };
        }

        Err(_) => {
            return SyncMessage::Error {
                code: SyncProtocolErrorCode::Internal,
                message: "The local snapshot inventory could not be loaded".to_string(),
            };
        }
    };

    let mut snapshots = Vec::new();

    for manifest in &manifests {
        snapshots.push(SnapshotDescriptor::from_manifest(manifest))
    }

    SyncMessage::SnapshotInventory {
        vn_sync_id,
        snapshots,
    }
}

///builds a response containing one verified snapshot manifest
pub fn build_manifest_response(request: SyncMessage, kakera_data_dir: PathBuf) -> SyncMessage {
    let (vn_sync_id, snapshot_id) = match request {
        SyncMessage::ManifestRequest {
            vn_sync_id,
            snapshot_id,
        } => (vn_sync_id, snapshot_id),
        _ => {
            return SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "Expected a snapshot manifest request".to_string(),
            };
        }
    };

    let manifests = match list_local_snapshots_for_vn(vn_sync_id.clone(), kakera_data_dir) {
        Ok(manifests) => manifests,
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            return SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "The VN sync iD is invalid".to_string(),
            };
        }

        Err(_) => {
            return SyncMessage::Error {
                code: SyncProtocolErrorCode::Internal,
                message: "The requested manifest could not be loaded".to_string(),
            };
        }
    };

    for manifest in manifests {
        if manifest.snapshot_id == snapshot_id {
            return SyncMessage::Manifest { manifest };
        }
    }

    SyncMessage::Error {
        code: SyncProtocolErrorCode::NotFound,
        message: "The requested snapshot does not exist".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_inventory_response, build_manifest_response, prepare_blob_response};
    use crate::{
        models::new_save_sync_id,
        sync_protocol::{SyncMessage, SyncProtocolErrorCode},
    };

    #[test]
    fn returns_empty_inventory_for_vn_without_snapshots() {
        let temp_dir = tempfile::tempdir().expect("the temporary directory should exist");

        let vn_sync_id = new_save_sync_id(42);

        let response = build_inventory_response(
            SyncMessage::SnapshotInventoryRequest {
                vn_sync_id: vn_sync_id.clone(),
            },
            temp_dir.path().to_path_buf(),
        );

        assert_eq!(
            response,
            SyncMessage::SnapshotInventory {
                vn_sync_id,
                snapshots: Vec::new(),
            }
        );
    }

    #[test]
    fn rejects_non_inventory_messages() {
        let temp_dir = tempfile::tempdir().expect("the temporary directory should exist");

        let response = build_inventory_response(
            SyncMessage::BlobRequest {
                vn_sync_id: new_save_sync_id(42),
                content_hash: "not-an-inventory-request".to_string(),
            },
            temp_dir.path().to_path_buf(),
        );

        assert_eq!(
            response,
            SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "Expected a snapshot inventory request".to_string(),
            }
        );
    }

    #[test]
    fn reports_missing_snapshot_manifest() {
        let temp_dir = tempfile::tempdir().expect("the temporary directory should exist");

        let response = build_manifest_response(
            SyncMessage::ManifestRequest {
                vn_sync_id: new_save_sync_id(42),
                snapshot_id: "missing-snapshot".to_string(),
            },
            temp_dir.path().to_path_buf(),
        );

        assert_eq!(
            response,
            SyncMessage::Error {
                code: SyncProtocolErrorCode::NotFound,
                message: "The requested snapshot does not exist".to_string(),
            }
        );
    }

    #[test]
    fn rejects_non_manifest_requests() {
        let temp_dir = tempfile::tempdir().expect("the temporary directory should exist");

        let response = build_manifest_response(
            SyncMessage::SnapshotInventoryRequest {
                vn_sync_id: new_save_sync_id(42),
            },
            temp_dir.path().to_path_buf(),
        );

        assert_eq!(
            response,
            SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "Expected a snapshot manifest request".to_string(),
            }
        );
    }

    #[test]
    fn rejects_invalid_blob_hash() {
        let temporary_directory = tempfile::tempdir().expect(
            "the temporary directory should
          exist",
        );

        let response = prepare_blob_response(
            SyncMessage::BlobRequest {
                vn_sync_id: new_save_sync_id(42),
                content_hash: "../private-file".to_string(),
            },
            temporary_directory.path().to_path_buf(),
        );

        assert_eq!(
            response,
            Err(SyncMessage::Error {
                code: SyncProtocolErrorCode::InvalidRequest,
                message: "The blob request is invalid".to_string(),
            })
        );
    }
}
