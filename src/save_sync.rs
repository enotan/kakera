use crate::models::SaveLocation;
use serde::{Deserialize, Serialize};

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, PathBuf},
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

///describes a blob safely stored in local content store
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBlob {
    pub path: PathBuf,
    pub fingerprint: FileFingerprint,
    pub reused_existing_blob: bool,
}

///a regular save file found on this device before it enters blob storage
#[derive(Debug, Clone, PartialEq)]
struct DiscoveredSaveFile {
    location_id: String,
    relative_path: String,
    source_path: PathBuf,
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

///copies one source file while hashing the exact bytes written
fn copy_and_hash_file(
    source_path: PathBuf,
    destination: &mut File,
) -> Result<FileFingerprint, io::Error> {
    let mut source = File::open(source_path)?;
    let mut hasher = blake3::Hasher::new();
    let mut size_bytes = 0_u64;
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = source.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];

        destination.write_all(chunk)?;
        hasher.update(chunk);
        size_bytes += bytes_read as u64;
    }

    destination.sync_all()?;

    Ok(FileFingerprint {
        size_bytes,
        content_hash: hasher.finalize().to_hex().to_string(),
    })
}

///stores one save file as a content-addressed blob
pub fn store_blob(source_path: PathBuf, blob_directory: PathBuf) -> Result<StoredBlob, io::Error> {
    fs::create_dir_all(&blob_directory)?;

    let mut temp_blob = tempfile::NamedTempFile::new_in(&blob_directory)?;

    let fingerprint = copy_and_hash_file(source_path, temp_blob.as_file_mut())?;

    let blob_path = blob_directory.join(format!("{}.blob", fingerprint.content_hash));

    match temp_blob.persist_noclobber(&blob_path) {
        Ok(_) => Ok(StoredBlob {
            path: blob_path,
            fingerprint,
            reused_existing_blob: false,
        }),

        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let existing_fingerprint = hash_file(blob_path.clone())?;

            if existing_fingerprint != fingerprint {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Existing content addressed blob failed verification",
                ));
            }

            Ok(StoredBlob {
                path: blob_path,
                fingerprint,
                reused_existing_blob: true,
            })
        }

        Err(error) => Err(error.error),
    }
}

///converts a relative platform path into a / separated manifest path
fn normalize_relative_path(path: PathBuf) -> Result<String, io::Error> {
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = match part.to_str() {
                    Some(text) => text.to_string(),
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Save path contains non UTF-8 text",
                        ));
                    }
                };

                parts.push(text);
            }

            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Snapshot paths must be relative and cannot contain traversal",
                ));
            }
        }
    }

    Ok(parts.join("/"))
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

///inputs required to create one complete local snapshot
#[derive(Debug, Clone, PartialEq)]
pub struct CreateSnapshotRequest {
    pub vn_sync_id: String,
    pub device_id: String,
    pub parent_snapshot_id: Option<String>,
    pub locations: Vec<SaveLocation>,
    pub storage_directory: PathBuf,
}

///result of successfully creating and persisting a snapshot
#[derive(Debug, Clone, PartialEq)]
pub struct CreatedSnapshot {
    pub manifest: SnapshotManifest,
    pub manifest_path: PathBuf,
    pub new_blob_count: usize,
    pub reused_blob_count: usize,
}

///derives a stable ID from the completed manifest contents
fn snapshot_id_for_manifest(manifest: &SnapshotManifest) -> Result<String, io::Error> {
    let identity_data = (
        manifest.format_version,
        &manifest.vn_sync_id,
        &manifest.device_id,
        &manifest.created_at,
        &manifest.parent_snapshot_id,
        &manifest.files,
    );

    let encoded = serde_json::to_vec(&identity_data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    Ok(blake3::hash(&encoded).to_hex().to_string())
}

///writes a snapshot manifest without exposing a partially written file
fn persist_manifest(
    manifest: &SnapshotManifest,
    manifest_directory: PathBuf,
) -> Result<PathBuf, io::Error> {
    fs::create_dir_all(&manifest_directory)?;

    let manifest_path = manifest_directory.join(format!("{}.json", manifest.snapshot_id));

    let mut temp_manifest = tempfile::NamedTempFile::new_in(&manifest_directory)?;

    serde_json::to_writer_pretty(temp_manifest.as_file_mut(), manifest)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    temp_manifest.as_file_mut().write_all(b"\n")?;
    temp_manifest.as_file_mut().sync_all()?;

    match temp_manifest.persist_noclobber(&manifest_path) {
        Ok(_) => Ok(manifest_path),

        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let existing_bytes = fs::read(&manifest_path)?;

            let existing_manifest: SnapshotManifest = serde_json::from_slice(&existing_bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

            if &existing_manifest != manifest {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Existing snapshot manifest failed verification",
                ));
            }

            Ok(manifest_path)
        }

        Err(error) => Err(error.error),
    }
}

///creates and safely persists one complete local save snapshot
pub fn create_snapshot(request: CreateSnapshotRequest) -> Result<CreatedSnapshot, io::Error> {
    let CreateSnapshotRequest {
        vn_sync_id,
        device_id,
        parent_snapshot_id,
        locations,
        storage_directory,
    } = request;

    if vn_sync_id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A snapshot requires a VN sync ID",
        ));
    }

    if device_id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A snapshot requires a device ID",
        ));
    }

    if locations.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A snapshot requires at least one save location",
        ));
    }

    let blob_directory = storage_directory.join("blobs");
    let manifest_directory = storage_directory.join("manifests");

    let mut files = Vec::new();
    let mut new_blob_count = 0;
    let mut reused_blob_count = 0;

    for location in locations {
        let discovered_files = discover_location_files(location)?;

        for discovered in discovered_files {
            let DiscoveredSaveFile {
                location_id,
                relative_path,
                source_path,
            } = discovered;

            let stored_blob = store_blob(source_path, blob_directory.clone())?;

            if stored_blob.reused_existing_blob {
                reused_blob_count += 1;
            } else {
                new_blob_count += 1;
            }

            files.push(SnapshotFile {
                location_id,
                relative_path,
                size_bytes: stored_blob.fingerprint.size_bytes,
                content_hash: stored_blob.fingerprint.content_hash,
            });
        }
    }

    files.sort_by(|left, right| {
        left.location_id
            .cmp(&right.location_id)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });

    let mut manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        snapshot_id: String::new(),
        vn_sync_id,
        device_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        parent_snapshot_id,
        files,
    };

    manifest.snapshot_id = snapshot_id_for_manifest(&manifest)?;

    let manifest_path = persist_manifest(&manifest, manifest_directory)?;

    Ok(CreatedSnapshot {
        manifest,
        manifest_path,
        new_blob_count,
        reused_blob_count,
    })
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

///converts a local path into a file record
fn discovered_save_file(
    location_id: String,
    relative_path: PathBuf,
    source_path: PathBuf,
) -> Result<DiscoveredSaveFile, io::Error> {
    Ok(DiscoveredSaveFile {
        location_id,
        relative_path: normalize_relative_path(relative_path)?,
        source_path,
    })
}

///hashes one discovered file and builds it a portable manifest entry
fn snapshot_file_from_discovered(
    discovered: DiscoveredSaveFile,
) -> Result<SnapshotFile, io::Error> {
    let fingerprint = hash_file(discovered.source_path)?;

    Ok(SnapshotFile {
        location_id: discovered.location_id,
        relative_path: discovered.relative_path,
        size_bytes: fingerprint.size_bytes,
        content_hash: fingerprint.content_hash,
    })
}

///recursively collects regular fils below one configured save dir
fn collect_directory_files(
    root: PathBuf,
    location_id: String,
) -> Result<Vec<DiscoveredSaveFile>, io::Error> {
    let mut pending_directories = vec![root.clone()];
    let mut files = Vec::new();

    while let Some(directory) = pending_directories.pop() {
        for entry_result in fs::read_dir(directory)? {
            let entry = entry_result?;
            let file_type = entry.file_type()?;
            let path = entry.path();

            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                pending_directories.push(path);
                continue;
            }

            if file_type.is_file() {
                let relative_path = path
                    .strip_prefix(&root)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

                files.push(discovered_save_file(
                    location_id.clone(),
                    relative_path.to_path_buf(),
                    path,
                )?);
            }
        }
    }

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(files)
}

///discovers files from one configured save file or dir
fn discover_location_files(location: SaveLocation) -> Result<Vec<DiscoveredSaveFile>, io::Error> {
    if location.id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Save locations require a stable ID",
        ));
    }

    let root = PathBuf::from(&location.path);
    let metadata = fs::symlink_metadata(&root)?;
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Save locations cannot be symbolic links",
        ));
    }

    if file_type.is_file() {
        let discovered = discovered_save_file(location.id, PathBuf::new(), root)?;

        return Ok(vec![discovered]);
    }

    if file_type.is_dir() {
        return collect_directory_files(root, location.id);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Save location is not a regular file or directory",
    ))
}

///collects manifest entries from one configured save file or dir
pub fn collect_location_files(location: SaveLocation) -> Result<Vec<SnapshotFile>, io::Error> {
    let discovered_files = discover_location_files(location)?;
    let mut snapshot_files = Vec::new();

    for discovered in discovered_files {
        snapshot_files.push(snapshot_file_from_discovered(discovered)?);
    }

    Ok(snapshot_files)
}

#[cfg(test)]
mod tests {
    use super::{
        CreateSnapshotRequest, SNAPSHOT_FORMAT_VERSION, SaveLocationState, SnapshotFile,
        SnapshotManifest, collect_location_files, create_snapshot, hash_file,
        inspect_save_locations, normalize_relative_path, store_blob,
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

    #[test]
    fn normalizes_safe_paths_and_rejects_traversal() {
        let nested_path = std::path::PathBuf::from("slot1").join("save.dat");

        let normalized = normalize_relative_path(nested_path)
            .expect("A normal relative save path should be accepted");

        assert_eq!(normalized, "slot1/save.dat");

        let traversal_path = std::path::PathBuf::from("..").join("private.dat");
        let traversal_result = normalize_relative_path(traversal_path);

        assert!(traversal_result.is_err());

        let absolute_result =
            normalize_relative_path(std::env::temp_dir().join("outside-save.dat"));

        assert!(absolute_result.is_err());
    }

    #[test]
    fn collects_nested_directory_files_in_stable_order() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-save-collection-{}", std::process::id()));

        let first_directory = test_root.join("slot-a");
        let second_directory = test_root.join("slot-b");

        let _ = fs::remove_dir_all(&test_root);

        fs::create_dir_all(&first_directory).expect("The first save directory should be created");
        fs::create_dir_all(&second_directory).expect("The second save directory should be created");

        fs::write(second_directory.join("save2.dat"), b"second save")
            .expect("The second save should be written");
        fs::write(first_directory.join("save1.dat"), b"first save")
            .expect("The first save should be written");

        let files = collect_location_files(SaveLocation {
            id: "main-saves".to_string(),
            path: test_root.to_string_lossy().into_owned(),
            label: "Main saves".to_string(),
        })
        .expect("The save directory should be collected");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].relative_path, "slot-a/save1.dat");
        assert_eq!(files[1].relative_path, "slot-b/save2.dat");
        assert_eq!(files[0].location_id, "main-saves");
        assert_eq!(files[1].location_id, "main-saves");
        assert!(!files[0].content_hash.is_empty());
        assert!(!files[1].content_hash.is_empty());

        fs::remove_dir_all(test_root)
            .expect("The temporary collection directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_follow_symbolic_links() {
        use std::os::unix::fs::symlink;

        let test_root =
            std::env::temp_dir().join(format!("kakera-save-symlink-{}", std::process::id()));
        let saves_directory = test_root.join("saves");
        let outside_file = test_root.join("private.dat");
        let root_link = test_root.join("linked-saves");

        let _ = fs::remove_dir_all(&test_root);
        fs::create_dir_all(&saves_directory).expect("the save directory should be created");

        fs::write(saves_directory.join("real.dat"), b"real save")
            .expect("the real save should be written");
        fs::write(&outside_file, b"must not be collected")
            .expect("the outside file should be written");

        symlink(&outside_file, saves_directory.join("linked.dat"))
            .expect("the nested file symlink should be created");
        symlink(&saves_directory, &root_link)
            .expect("the root directory symlink should be created");

        let files = collect_location_files(SaveLocation {
            id: "main-saves".to_string(),
            path: saves_directory.to_string_lossy().into_owned(),
            label: "Main saves".to_string(),
        })
        .expect("the real save directory should be collected");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "real.dat");

        let linked_root_result = collect_location_files(SaveLocation {
            id: "linked-saves".to_string(),
            path: root_link.to_string_lossy().into_owned(),
            label: "Linked saves".to_string(),
        });

        assert!(linked_root_result.is_err());

        fs::remove_dir_all(test_root).expect(
            "the temporary symlink directory should be
          removed",
        );
    }
    #[test]
    fn stores_and_reuses_content_addressed_blobs() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-blob-storage-{}", std::process::id()));
        let source_file = test_root.join("live-save.dat");
        let blob_directory = test_root.join("blobs");
        let original_contents = b"visual novel save contents";

        let _ = fs::remove_dir_all(&test_root);
        fs::create_dir_all(&test_root).expect("the blob test directory should be created");
        fs::write(&source_file, original_contents).expect("the live save should be written");

        let first_blob = store_blob(source_file.clone(), blob_directory.clone())
            .expect("the first blob should be stored");

        assert!(!first_blob.reused_existing_blob);
        assert_eq!(
            fs::read(&first_blob.path).expect(
                "the stored blob should
          be readable"
            ),
            original_contents
        );

        let repeated_blob = store_blob(source_file.clone(), blob_directory.clone())
            .expect("the identical blob should be reused");

        assert!(repeated_blob.reused_existing_blob);
        assert_eq!(repeated_blob.path, first_blob.path);
        assert_eq!(repeated_blob.fingerprint, first_blob.fingerprint);

        fs::write(&source_file, b"new save contents")
            .expect("the changed live save should be written");

        let changed_blob = store_blob(source_file, blob_directory.clone())
            .expect("the changed blob should be stored");

        assert!(!changed_blob.reused_existing_blob);
        assert_ne!(changed_blob.path, first_blob.path);

        let blob_count = fs::read_dir(&blob_directory)
            .expect("the blob directory should be readable")
            .count();

        assert_eq!(blob_count, 2);

        fs::remove_dir_all(test_root).expect("the temporary blob directory should be removed");
    }

    #[test]
    fn creates_a_complete_local_snapshot() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-complete-snapshot-{}", std::process::id()));

        let saves_directory = test_root.join("live-saves");
        let nested_directory = saves_directory.join("slot-1");
        let storage_directory = test_root.join("snapshot-store");

        let _ = fs::remove_dir_all(&test_root);

        fs::create_dir_all(&nested_directory).expect("the nested save directory should be created");

        fs::write(saves_directory.join("settings.dat"), b"settings")
            .expect("the settings save should be written");

        fs::write(nested_directory.join("progress.dat"), b"progress")
            .expect("the progress save should be written");

        let created = create_snapshot(CreateSnapshotRequest {
            vn_sync_id: "vn-test".to_string(),
            device_id: "test-device".to_string(),
            parent_snapshot_id: None,
            locations: vec![SaveLocation {
                id: "main-saves".to_string(),
                path: saves_directory.to_string_lossy().into_owned(),
                label: "Main saves".to_string(),
            }],
            storage_directory: storage_directory.clone(),
        })
        .expect("the complete snapshot should be created");

        assert_eq!(created.manifest.format_version, SNAPSHOT_FORMAT_VERSION);
        assert_eq!(created.manifest.vn_sync_id, "vn-test");
        assert_eq!(created.manifest.device_id, "test-device");
        assert_eq!(created.manifest.files.len(), 2);
        assert_eq!(created.new_blob_count, 2);
        assert_eq!(created.reused_blob_count, 0);
        assert!(!created.manifest.snapshot_id.is_empty());
        assert!(created.manifest_path.is_file());

        let saved_manifest_bytes =
            fs::read(&created.manifest_path).expect("the persisted manifest should be readable");

        let saved_manifest: SnapshotManifest = serde_json::from_slice(&saved_manifest_bytes)
            .expect(
                "the persisted manifest should contain valid
              JSON",
            );

        assert_eq!(saved_manifest, created.manifest);

        for file in &created.manifest.files {
            let blob_path = storage_directory
                .join("blobs")
                .join(format!("{}.blob", file.content_hash));

            assert!(blob_path.is_file());
        }

        fs::remove_dir_all(test_root).expect(
            "the complete snapshot test directory should be
          removed",
        );
    }
}
