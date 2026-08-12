use crate::{
    save_sync::list_local_snapshots_for_vn,
    sync_protocol::{SnapshotDescriptor, SyncMessage, SyncProtocolErrorCode},
};
use std::{io, path::PathBuf};

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
#[cfg(test)]
mod tests {
    use super::build_inventory_response;
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
}
