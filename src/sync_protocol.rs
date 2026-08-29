use crate::save_sync::SnapshotManifest;
use serde::{Deserialize, Serialize};

///current kakera protocol ver
pub const SYNC_PROTOCOL_VERSION: u32 = 1;

///maximum size of one control message
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 1024 * 1024;

///identifies a peer when an authenticated connection starts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerHello {
    pub protocol_version: u32,
    pub device_id: String,
    pub device_name: String,
}

///compact snapshot info used when comparing histories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotDescriptor {
    pub snapshot_id: String,
    pub parent_snapshot_id: Option<String>,
    pub device_id: String,
    pub created_at: String,
    pub file_count: usize,
}

impl SnapshotDescriptor {
    ///builds a network safe summary from a verified local manifest
    pub fn from_manifest(manifest: &SnapshotManifest) -> Self {
        Self {
            snapshot_id: manifest.snapshot_id.clone(),
            parent_snapshot_id: manifest.parent_snapshot_id.clone(),
            device_id: manifest.device_id.clone(),
            created_at: manifest.created_at.clone(),
            file_count: manifest.files.len(),
        }
    }
}

///a control message sent between two paired kakeras
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncMessage {
    ///begins the app level protocol after the secure connection opens
    Hello(PeerHello),

    ///requests the snapshot history for one portable vn id
    SnapshotInventoryRequest { vn_sync_id: String },

    ///returns the snapshot summaries for a vn, newest first
    SnapshotInventory {
        vn_sync_id: String,
        snapshots: Vec<SnapshotDescriptor>,
    },

    ///requests one complete snapshot manifest
    ManifestRequest {
        vn_sync_id: String,
        snapshot_id: String,
    },

    ///returns a manifest after the sender has verified it locally
    Manifest { manifest: SnapshotManifest },

    ///requests one save blob
    BlobRequest {
        vn_sync_id: String,
        content_hash: String,
    },

    ///announces the raw blob bytes that come right after the message
    BlobReady {
        content_hash: String,
        size_bytes: u64,
    },

    ///confirms that a complete snapshot and all required blobs were received
    TransferComplete {
        vn_sync_id: String,
        snapshot_id: String,
    },

    ///reports a request that the peer would not complete
    Error {
        code: SyncProtocolErrorCode,
        message: String,
    },
}

///machine-readable categories for peer protocol failures
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncProtocolErrorCode {
    UnsupportedVersion,
    Unauthorized,
    InvalidRequest,
    NotFound,
    IntegrityFailure,
    Internal,
}

///encodes one control message for transport
pub fn encode_message(message: &SyncMessage) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(message)
}

///decodes one control message after the transport checks its length
pub fn decode_message(bytes: Vec<u8>) -> Result<SyncMessage, serde_json::Error> {
    serde_json::from_slice(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        PeerHello, SYNC_PROTOCOL_VERSION, SnapshotDescriptor, SyncMessage, decode_message,
        encode_message,
    };

    use crate::save_sync::SnapshotManifest;

    #[test]
    fn round_trips_peer_hello_message() {
        let message = SyncMessage::Hello(PeerHello {
            protocol_version: SYNC_PROTOCOL_VERSION,
            device_id: "device-test".to_string(),
            device_name: "Steam Deck".to_string(),
        });

        let encoded = encode_message(&message).expect("the hello message should encode");

        let decoded = decode_message(encoded).expect("the hello message should decode");

        assert_eq!(decoded, message);
    }

    #[test]
    fn creates_snapshot_descriptor_without_file_details() {
        let manifest = SnapshotManifest {
            format_version: 1,
            snapshot_id: "snapshot-one".to_string(),
            vn_sync_id: "sync-test".to_string(),
            device_id: "device-test".to_string(),
            created_at: "2026-08-08T12:00:00Z".to_string(),
            parent_snapshot_id: Some("snapshot-zero".to_string()),
            files: Vec::new(),
        };

        let descriptor = SnapshotDescriptor::from_manifest(&manifest);

        assert_eq!(descriptor.snapshot_id, "snapshot-one");
        assert_eq!(
            descriptor.parent_snapshot_id,
            Some("snapshot-zero".to_string())
        );
        assert_eq!(descriptor.file_count, 0);
    }

    #[test]
    fn round_trips_snapshot_inventory_message() {
        let message = SyncMessage::SnapshotInventory {
            vn_sync_id: "sync-test".to_string(),
            snapshots: vec![SnapshotDescriptor {
                snapshot_id: "snapshot-one".to_string(),
                parent_snapshot_id: None,
                device_id: "device-test".to_string(),
                created_at: "2026-08-08T12:00:00Z".to_string(),
                file_count: 3,
            }],
        };

        let encoded = encode_message(&message).expect("the inventory should encode");

        let decoded = decode_message(encoded).expect("the inventory should decode");

        assert_eq!(decoded, message);
    }
}
