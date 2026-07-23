use crate::models::SaveLocation;
use serde::{Deserialize, Serialize};

use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

///describes a configured save location
#[derive(Debug, Clone, PartialEq)]
pub enum SaveLocationState {
    File,
    Directory,
    Missing,
    Unreadable(String),
    Unsupported,
}

///combines the configured save location with the current filesystem state

#[derive(Debug, Clone, PartialEq)]
pub struct SaveLocationStatus {
    pub location: SaveLocation,
    pub state: SaveLocationState,
}

///describes one file inside a save snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotFile {
    pub location_id: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub content_hash: String,
}

///the size and content hash calculated for one file
#[derive(Debug, Clone, PartialEq)]
pub struct FileFingerprint {
    pub size_bytes: u64,
    pub content_hash: String,
}

const HASH_BUFFER_SIZE: usize = 64 * 1024;

///calculates the blake3 hash and byte size of one file
pub fn hash_file(path: PathBuf) -> Result<FileFingerprint, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut size_bytes = 0_u64;
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
        size_bytes += bytes_read as u64;
    }

    Ok(FileFingerprint {
        size_bytes,
        content_hash: hasher.finalize().to_hex().to_string(),
    })
}

///current version of the snapshot format, incase it changes in the future
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

///metadata describing one complete save snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotManifest {
    pub format_version: u32,
    pub snapshot_id: String,
    pub vn_sync_id: String,
    pub device_id: String,
    pub created_at: String,

    #[serde(default)]
    pub parent_snapshot_id: Option<String>,

    pub files: Vec<SnapshotFile>,
}

///inspects one configured save location without modifying it
pub fn inspect_save_location(location: SaveLocation) -> SaveLocationStatus {
    let state = match std::fs::metadata(&location.path) {
        Ok(metadata) if metadata.is_file() => SaveLocationState::File,
        Ok(metadata) if metadata.is_dir() => SaveLocationState::Directory,
        Ok(_) => SaveLocationState::Unsupported,

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SaveLocationState::Missing,

        Err(error) => SaveLocationState::Unreadable(error.to_string()),
    };

    SaveLocationStatus { location, state }
}

///inspects every configured save location
pub fn inspect_save_locations(locations: Vec<SaveLocation>) -> Vec<SaveLocationStatus> {
    locations.into_iter().map(inspect_save_location).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        SNAPSHOT_FORMAT_VERSION, SaveLocationState, SnapshotFile, SnapshotManifest, hash_file,
        inspect_save_locations,
    };

    use crate::models::SaveLocation;
    use std::fs;

    #[test]
    fn inspects_common_save_location_types() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-save-inspection-{}", std::process::id()));

        let save_file = test_root.join("save.dat");
        let save_directory = test_root.join("slots");
        let missing_path = test_root.join("missing");
        let _ = fs::remove_dir_all(&test_root);

        fs::create_dir_all(&save_directory)
            .expect("The temporary save directory should be created");
        fs::write(&save_file, b"test save").expect("The temporary save file should be created");

        let statuses = inspect_save_locations(vec![
            SaveLocation {
                id: "save-file".to_string(),
                path: save_file.to_string_lossy().into_owned(),
                label: "Save file".to_string(),
            },
            SaveLocation {
                id: "save-directory".to_string(),
                path: save_directory.to_string_lossy().into_owned(),
                label: "Save directory".to_string(),
            },
            SaveLocation {
                id: "missing-save".to_string(),
                path: missing_path.to_string_lossy().into_owned(),
                label: "Missing save".to_string(),
            },
        ]);

        assert_eq!(statuses[0].state, SaveLocationState::File);
        assert_eq!(statuses[1].state, SaveLocationState::Directory);
        assert_eq!(statuses[2].state, SaveLocationState::Missing);

        fs::remove_dir_all(test_root).expect("The temporary test directory should be removed");
    }

    #[test]
    fn snapshot_manifest_round_trips_through_json() {
        let manifest = SnapshotManifest {
            format_version: SNAPSHOT_FORMAT_VERSION,
            snapshot_id: "snapshot-001".to_string(),
            vn_sync_id: "vn-muramasa".to_string(),
            device_id: "desktop".to_string(),
            created_at: "2026-07-23T12:00:00Z".to_string(),
            parent_snapshot_id: Some("snapshot-000".to_string()),
            files: vec![SnapshotFile {
                location_id: "main-saves".to_string(),
                relative_path: "slot1/save.dat".to_string(),
                size_bytes: 128,
                content_hash: "example-blake3-hash".to_string(),
            }],
        };

        let json = serde_json::to_string_pretty(&manifest)
            .expect("The snapshot manifest should serialize");

        let loaded = serde_json::from_str::<SnapshotManifest>(&json)
            .expect("The snapshot manifest should deserialize");

        assert_eq!(loaded, manifest);
    }

    #[test]
    fn hashes_file_contents_deterministically() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-save-hashing-{}", std::process::id()));
        let save_file = test_root.join("save.dat");
        let first_contents = b"first save contents";

        let _ = fs::remove_dir_all(&test_root);
        fs::create_dir_all(&test_root).expect("The temporary hash directory should be created");
        fs::write(&save_file, first_contents).expect("The first temporary save should be written");

        let first_hash = hash_file(save_file.clone()).expect("The first save should be hashed");
        let repeated_hash =
            hash_file(save_file.clone()).expect("The same save should be hashed again");

        assert_eq!(first_hash, repeated_hash);
        assert_eq!(first_hash.size_bytes, first_contents.len() as u64);

        fs::write(&save_file, b"changed save contents")
            .expect("The changed temporary save should be written");

        let changed_hash = hash_file(save_file).expect("The changed save should be hashed");

        assert_ne!(first_hash.content_hash, changed_hash.content_hash);

        fs::remove_dir_all(test_root).expect("The temporary hash directory should be removed");
    }
}
