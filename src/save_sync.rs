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

///a local blob which passed verification and should be safe to transfer
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedLocalBlob {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub content_hash: String,
}

///a regular save file found on this device before it enters blob storage
#[derive(Debug, Clone, PartialEq)]
struct DiscoveredSaveFile {
    location_id: String,
    relative_path: String,
    source_path: PathBuf,
}

///records the live file fingerprint that a recovery snapshot must preserve
#[derive(Debug, Clone, PartialEq)]
struct RestoreConflictEvidence {
    location_id: String,
    relative_path: String,
    live_fingerprint: FileFingerprint,
}

const HASH_BUFFER_SIZE: usize = 64 * 1024;
const DEVICE_ID_FILE_NAME: &str = "device-id";

///loads this kakera install's id or creates it on first time usage
pub fn load_or_create_device_id(storage_directory: PathBuf) -> Result<String, io::Error> {
    fs::create_dir_all(&storage_directory)?;

    let identity_path = storage_directory.join(DEVICE_ID_FILE_NAME);

    match fs::read_to_string(&identity_path) {
        Ok(identity) => validate_device_id(identity),

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_device_id(identity_path, storage_directory)
        }

        Err(error) => Err(error),
    }
}

fn create_device_id(
    identity_path: PathBuf,
    storage_directory: PathBuf,
) -> Result<String, io::Error> {
    let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();

    let seed = format!(
        "{timestamp}:{}:{}",
        std::process::id(),
        storage_directory.display()
    );

    let identity = format!("device-{}", blake3::hash(seed.as_bytes()).to_hex());

    let mut temp_file = tempfile::NamedTempFile::new_in(&storage_directory)?;
    temp_file.write_all(identity.as_bytes())?;
    temp_file.as_file_mut().sync_all()?;

    match temp_file.persist_noclobber(&identity_path) {
        Ok(_) => Ok(identity),

        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let existing_identity = fs::read_to_string(identity_path)?;
            validate_device_id(existing_identity)
        }

        Err(error) => Err(error.error),
    }
}

fn validate_device_id(id: String) -> Result<String, io::Error> {
    let trimmed_id = id.trim().to_string();
    let hash = trimmed_id.strip_prefix("device-");

    let is_valid = match hash {
        Some(hash) => {
            hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit())
        }
        None => false,
    };

    if !is_valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The save sync device id is invalid",
        ));
    }

    Ok(trimmed_id)
}

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

///inputs required to plan the restoration of one snapshot
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreSnapshotRequest {
    pub manifest: SnapshotManifest,
    pub locations: Vec<SaveLocation>,
    pub storage_directory: PathBuf,
}

///describes one blob and the live save path it would restore
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRestoreFile {
    pub snapshot_file: SnapshotFile,
    pub source_blob_path: PathBuf,
    pub destination_path: PathBuf,
    pub destination_state: RestoreDestinationState,
}

///describes every filesystem change made by a snapshot restore
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePlan {
    pub snapshot_id: String,
    pub files: Vec<PlannedRestoreFile>,
}

///summarises a successfully applied restore plan
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedRestore {
    pub snapshot_id: String,
    pub restored_file_count: usize,
    pub skipped_identical_count: usize,
    pub backup_snapshot_id: Option<String>,
}

///information required to snapshot live saves before replacing conflicts
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreBackupConfig {
    pub vn_sync_id: String,
    pub device_id: String,
    pub locations: Vec<SaveLocation>,
    pub storage_directory: PathBuf,
}

///maps snapshot location ids onto a specific device's configured save paths
pub fn map_received_snapshot_locations(
    manifest: &SnapshotManifest,
    mut locations: Vec<SaveLocation>,
) -> Result<Vec<SaveLocation>, io::Error> {
    let mut incoming_ids: Vec<String> = Vec::new();

    for file in &manifest.files {
        if !incoming_ids.contains(&file.location_id) {
            incoming_ids.push(file.location_id.clone());
        }
    }

    let every_location_matches = incoming_ids
        .iter()
        .all(|incoming_id| locations.iter().any(|location| location.id == *incoming_id));

    if every_location_matches {
        return Ok(locations);
    }

    if incoming_ids.len() == 1 && locations.len() == 1 {
        locations[0].id = incoming_ids[0].clone();
        return Ok(locations);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "The snapshot has {} unmatched save locations but this device has {} configured locations.",
            incoming_ids.len(),
            locations.len()
        ),
    ))
}

///restores a received snapshot into a specific device's configured save locations
pub fn apply_received_snapshot(
    manifest: SnapshotManifest,
    locations: Vec<SaveLocation>,
    kakera_data_dir: PathBuf,
    backup_before_restore: bool,
) -> Result<AppliedReceivedSnapshot, io::Error> {
    let mapped_locations = map_received_snapshot_locations(&manifest, locations)?;

    let vn_sync_id = manifest.vn_sync_id.clone();
    let sync_root = kakera_data_dir.join("save-sync");
    let storage_directory = sync_root.join("vns").join(&vn_sync_id);

    let plan = plan_snapshot_restore(RestoreSnapshotRequest {
        manifest,
        locations: mapped_locations.clone(),
        storage_directory: storage_directory.clone(),
    })?;

    let conflict_policy = if backup_before_restore {
        let device_id = load_or_create_device_id(sync_root)?;

        RestoreConflictPolicy::BackupAndReplace(RestoreBackupConfig {
            vn_sync_id,
            device_id,
            locations: mapped_locations.clone(),
            storage_directory,
        })
    } else {
        RestoreConflictPolicy::Reject
    };

    let restore = apply_restore_plan_with_policy(plan, conflict_policy)?;

    Ok(AppliedReceivedSnapshot {
        restore,
        mapped_locations,
    })
}

///controls how an applied restore handles different live save files
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreConflictPolicy {
    Reject,
    BackupAndReplace(RestoreBackupConfig),
}

///describes how a live save file compares with its snapshot version
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreDestinationState {
    Missing,
    Identical,
    Different(FileFingerprint),
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

///loads and verifies one manifest file from the local snapshot store
fn load_snapshot_manifest_file(manifest_path: PathBuf) -> Result<SnapshotManifest, io::Error> {
    let metadata = fs::symlink_metadata(&manifest_path)?;

    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot manifest is not a regular file",
        ));
    }

    let manifest_bytes = fs::read(&manifest_path)?;

    let manifest: SnapshotManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    if manifest.format_version != SNAPSHOT_FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Unsupported snapshot format version {}",
                manifest.format_version
            ),
        ));
    }

    let expected_snapshot_id = snapshot_id_for_manifest(&manifest)?;

    if expected_snapshot_id != manifest.snapshot_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot manifest failed identity verification",
        ));
    }

    let expected_file_name = format!("{}.json", manifest.snapshot_id);

    let actual_file_name = manifest_path.file_name().and_then(|name| name.to_str());

    if actual_file_name != Some(&expected_file_name.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot manifest filename does not match its ID",
        ));
    }

    chrono::DateTime::parse_from_rfc3339(&manifest.created_at)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    Ok(manifest)
}

///lists verified snapshots, starting with the newest first
pub fn list_local_snapshots(
    storage_directory: PathBuf,
) -> Result<Vec<SnapshotManifest>, io::Error> {
    let manifest_directory = storage_directory.join("manifests");

    let entries = match fs::read_dir(&manifest_directory) {
        Ok(entries) => entries,

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }

        Err(error) => return Err(error),
    };

    let mut manifests = Vec::new();

    for entry_result in entries {
        let entry = entry_result?;
        let path = entry.path();

        let extension = path.extension().and_then(|value| value.to_str());

        if extension != Some("json") {
            continue;
        }

        manifests.push(load_snapshot_manifest_file(path)?);
    }

    manifests.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    Ok(manifests)
}

///converts a portable manifest path into a safe local relative path
fn path_from_manifest(relative_path: String) -> Result<Option<PathBuf>, io::Error> {
    if relative_path.is_empty() {
        return Ok(None);
    }

    if relative_path.starts_with('/')
        || relative_path.ends_with('/')
        || relative_path.contains('\\')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Manifest contains an unsafe save path",
        ));
    }

    let mut safe_path = PathBuf::new();
    for part in relative_path.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\0')
            || part.contains(':')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Manifest contains an unsafe path component",
            ));
        }

        let component_path = PathBuf::from(part);
        let mut components = component_path.components();

        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => {
                safe_path.push(part);
            }

            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Manifest contains a non-relative path component",
                ));
            }
        }
    }
    Ok(Some(safe_path))
}

///locates and verifies the blob referenced by one manifest entry
fn verified_blob_path(
    snapshot_file: &SnapshotFile,
    blob_directory: PathBuf,
) -> Result<PathBuf, io::Error> {
    let content_hash = &snapshot_file.content_hash;

    if content_hash.len() != 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot contains an invalid blob hash",
        ));
    }

    for byte in content_hash.bytes() {
        if !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Snapshot contains a non canonical blob hash",
            ));
        }
    }

    let blob_path = blob_directory.join(format!("{content_hash}.blob"));

    let metadata = fs::symlink_metadata(&blob_path)?;

    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot blob is not a regular file",
        ));
    }

    let actual_fingerprint = hash_file(blob_path.clone())?;

    if actual_fingerprint.size_bytes != snapshot_file.size_bytes
        || actual_fingerprint.content_hash != snapshot_file.content_hash
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot blob failed integrity verification",
        ));
    }

    Ok(blob_path)
}

///finds exactly one configured location for a manifest location id
fn location_for_restore(
    location_id: String,
    locations: &[SaveLocation],
) -> Result<SaveLocation, io::Error> {
    let mut matching_location: Option<SaveLocation> = None;

    for location in locations {
        if location.id == location_id {
            if matching_location.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Multiple save locations use the ID {location_id}"),
                ));
            }
            matching_location = Some(location.clone());
        }
    }

    match matching_location {
        Some(location) => Ok(location),

        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No configured save location matches {location_id}"),
        )),
    }
}

///calculates the live destination path for the one snapshot file
fn destination_path_for_snapshot_file(
    snapshot_file: &SnapshotFile,
    locations: &[SaveLocation],
) -> Result<PathBuf, io::Error> {
    let location = location_for_restore(snapshot_file.location_id.clone(), locations)?;

    if location.path.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A restore location cannot have an empty path",
        ));
    }

    let configured_path = PathBuf::from(location.path);
    let relative_path = path_from_manifest(snapshot_file.relative_path.clone())?;

    match relative_path {
        None => match fs::symlink_metadata(&configured_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A direct save location cannot be a symbolic link",
            )),

            Ok(metadata) if metadata.file_type().is_dir() => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A direct save location points to a directory",
            )),

            Ok(metadata) if !metadata.file_type().is_file() => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A direct save location is not a regular file",
            )),

            Ok(_) => Ok(configured_path),

            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(configured_path),

            Err(error) => Err(error),
        },

        Some(relative_path) => {
            match fs::symlink_metadata(&configured_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "A save directory cannot be a symbolic link",
                    ));
                }

                Ok(metadata) if !metadata.file_type().is_dir() => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "A save directory location is not a directory",
                    ));
                }

                Ok(_) => {}

                Err(error) if error.kind() == io::ErrorKind::NotFound => {}

                Err(error) => return Err(error),
            }

            Ok(configured_path.join(relative_path))
        }
    }
}

///compares a restore destination with its expected snapshot contents
fn inspect_restore_destination(
    destination_path: PathBuf,
    snapshot_file: &SnapshotFile,
) -> Result<RestoreDestinationState, io::Error> {
    let metadata = match fs::symlink_metadata(&destination_path) {
        Ok(metadata) => metadata,

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RestoreDestinationState::Missing);
        }

        Err(error) => return Err(error),
    };

    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A restore destination cannot be a symbolic link",
        ));
    }

    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A restore destination is not a regular file",
        ));
    }

    let current_fingerprint = hash_file(destination_path)?;

    if current_fingerprint.size_bytes == snapshot_file.size_bytes
        && current_fingerprint.content_hash == snapshot_file.content_hash
    {
        Ok(RestoreDestinationState::Identical)
    } else {
        Ok(RestoreDestinationState::Different(current_fingerprint))
    }
}

///lists verified local snapshots belonging to one vn
pub fn list_local_snapshots_for_vn(
    vn_sync_id: String,
    kakera_data_dir: PathBuf,
) -> Result<Vec<SnapshotManifest>, io::Error> {
    validate_vn_sync_id(vn_sync_id.clone())?;

    let storage_directory = kakera_data_dir
        .join("save-sync")
        .join("vns")
        .join(vn_sync_id);

    list_local_snapshots(storage_directory)
}

///verifies and stores a snapshot manifest received from another device
pub fn persist_received_snapshot_manifest(
    manifest: SnapshotManifest,
    kakera_data_dir: PathBuf,
) -> Result<PathBuf, io::Error> {
    validate_vn_sync_id(manifest.vn_sync_id.clone())?;

    if manifest.format_version != SNAPSHOT_FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The received snapshot uses an unsupported format",
        ));
    }

    let expected_snapshot_id = snapshot_id_for_manifest(&manifest)?;

    if expected_snapshot_id != manifest.snapshot_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The received snapshot failed identity verification",
        ));
    }

    chrono::DateTime::parse_from_rfc3339(&manifest.created_at)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    for snapshot_file in &manifest.files {
        let blob = verified_local_blob_for_vn(
            manifest.vn_sync_id.clone(),
            snapshot_file.content_hash.clone(),
            kakera_data_dir.clone(),
        )?;

        if blob.size_bytes != snapshot_file.size_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "A received snapshot blob has the wrong size",
            ));
        }
    }

    let manifest_dir = kakera_data_dir
        .join("save-sync")
        .join("vns")
        .join(&manifest.vn_sync_id)
        .join("manifests");

    persist_manifest(&manifest, manifest_dir)
}

///locates and verifies one blob belonging to a vn's snapshot store
pub fn verified_local_blob_for_vn(
    vn_sync_id: String,
    content_hash: String,
    kakera_data_dir: PathBuf,
) -> Result<VerifiedLocalBlob, io::Error> {
    validate_vn_sync_id(vn_sync_id.clone())?;

    let hash_is_valid = content_hash.len() == 64
        && content_hash
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character));

    if !hash_is_valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The requested blob hash is invalid",
        ));
    }

    let blob_path = kakera_data_dir
        .join("save-sync")
        .join("vns")
        .join(vn_sync_id)
        .join("blobs")
        .join(format!("{content_hash}.blob"));

    let metadata = fs::symlink_metadata(&blob_path)?;

    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The requested blob is not a regular file",
        ));
    }

    let fingerprint = hash_file(blob_path.clone())?;

    if fingerprint.content_hash != content_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The requested blob failed integrity verification",
        ));
    }

    Ok(VerifiedLocalBlob {
        path: blob_path,
        size_bytes: fingerprint.size_bytes,
        content_hash,
    })
}

///creates a local snapshot for one vn
pub fn create_local_snapshot_for_vn(
    vn_sync_id: String,
    locations: Vec<SaveLocation>,
    kakera_data_dir: PathBuf,
) -> Result<CreatedSnapshot, io::Error> {
    validate_vn_sync_id(vn_sync_id.clone())?;

    let sync_root = kakera_data_dir.join("save-sync");
    let storage_directory = sync_root.join("vns").join(vn_sync_id.clone());

    let device_id = load_or_create_device_id(sync_root)?;

    let existing_snapshots = list_local_snapshots_for_vn(vn_sync_id.clone(), kakera_data_dir)?;

    let parent_snapshot_id = existing_snapshots
        .first()
        .map(|manifest| manifest.snapshot_id.clone());

    create_snapshot(CreateSnapshotRequest {
        vn_sync_id,
        device_id,
        parent_snapshot_id,
        locations,
        storage_directory,
    })
}

fn validate_vn_sync_id(sync_id: String) -> Result<(), io::Error> {
    let hash = sync_id.strip_prefix("sync-");

    let is_valid = match hash {
        Some(hash) => {
            hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit())
        }
        None => false,
    };

    if !is_valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The VN sync id is invalid",
        ));
    }

    Ok(())
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

///validates a snapshot and creates a read-only restore plan
pub fn plan_snapshot_restore(request: RestoreSnapshotRequest) -> Result<RestorePlan, io::Error> {
    let RestoreSnapshotRequest {
        manifest,
        locations,
        storage_directory,
    } = request;

    if manifest.format_version != SNAPSHOT_FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Unsupported snapshot format version {}",
                manifest.format_version
            ),
        ));
    }

    if manifest.snapshot_id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "A snapshot manifest requires an ID",
        ));
    }

    let expected_snapshot_id = snapshot_id_for_manifest(&manifest)?;

    if expected_snapshot_id != manifest.snapshot_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot manifest failed identity verification",
        ));
    }

    let snapshot_id = manifest.snapshot_id.clone();
    let blob_directory = storage_directory.join("blobs");
    let mut planned_files: Vec<PlannedRestoreFile> = Vec::new();

    for snapshot_file in manifest.files {
        let destination_path = destination_path_for_snapshot_file(&snapshot_file, &locations)?;

        for existing_file in &planned_files {
            let existing_path = &existing_file.destination_path;

            if destination_path == *existing_path
                || destination_path.starts_with(existing_path)
                || existing_path.starts_with(&destination_path)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Snapshot contains overlapping restore paths: {} and {}",
                        existing_path.display(),
                        destination_path.display()
                    ),
                ));
            }
        }

        let source_blob_path = verified_blob_path(&snapshot_file, blob_directory.clone())?;

        let destination_state =
            inspect_restore_destination(destination_path.clone(), &snapshot_file)?;

        planned_files.push(PlannedRestoreFile {
            snapshot_file,
            source_blob_path,
            destination_path,
            destination_state,
        });
    }

    planned_files.sort_by(|left, right| left.destination_path.cmp(&right.destination_path));

    Ok(RestorePlan {
        snapshot_id,
        files: planned_files,
    })
}

///applies one planned file without overwriting a conflicting live save, returns true when a file
///is written and false if its identical
fn apply_planned_restore_file(planned_file: &PlannedRestoreFile) -> Result<bool, io::Error> {
    let parent_directory = match planned_file.destination_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };

    fs::create_dir_all(&parent_directory)?;

    let parent_metadata = fs::symlink_metadata(&parent_directory)?;

    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Restore destination parent is not a safe directory",
        ));
    }

    match inspect_restore_destination(
        planned_file.destination_path.clone(),
        &planned_file.snapshot_file,
    )? {
        RestoreDestinationState::Missing => {}

        RestoreDestinationState::Identical => {
            return Ok(false);
        }

        RestoreDestinationState::Different(_) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Restore would overwrite a different live safe: {}",
                    planned_file.destination_path.display()
                ),
            ));
        }
    }

    let source_metadata = fs::symlink_metadata(&planned_file.source_blob_path)?;

    if !source_metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Restore source blob is not a regular file",
        ));
    }

    let mut temporary_file = tempfile::NamedTempFile::new_in(&parent_directory)?;

    let copied_fingerprint = copy_and_hash_file(
        planned_file.source_blob_path.clone(),
        temporary_file.as_file_mut(),
    )?;

    if copied_fingerprint.size_bytes != planned_file.snapshot_file.size_bytes
        || copied_fingerprint.content_hash != planned_file.snapshot_file.content_hash
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Restore source changed while it was being copied",
        ));
    }

    match temporary_file.persist_noclobber(&planned_file.destination_path) {
        Ok(_) => Ok(true),

        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let current_state = inspect_restore_destination(
                planned_file.destination_path.clone(),
                &planned_file.snapshot_file,
            )?;

            if current_state == RestoreDestinationState::Identical {
                Ok(false)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Restore destination changed during the operation",
                ))
            }
        }

        Err(error) => Err(error.error),
    }
}

///verifies every source blob before an applied restore begins
fn verify_restore_plan_sources(files: &[PlannedRestoreFile]) -> Result<(), io::Error> {
    for planned_file in files {
        let source_metadata = fs::symlink_metadata(&planned_file.source_blob_path)?;

        if !source_metadata.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Restore source blob is not a regular file",
            ));
        }

        let source_fingerprint = hash_file(planned_file.source_blob_path.clone())?;

        if source_fingerprint.size_bytes != planned_file.snapshot_file.size_bytes
            || source_fingerprint.content_hash != planned_file.snapshot_file.content_hash
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Restore source blob failed preflight verification",
            ));
        }
    }

    Ok(())
}

///applies a conflict-free restore plan
pub fn apply_restore_plan(plan: RestorePlan) -> Result<AppliedRestore, io::Error> {
    let RestorePlan { snapshot_id, files } = plan;

    verify_restore_plan_sources(&files)?;

    for planned_file in &files {
        let current_state = inspect_restore_destination(
            planned_file.destination_path.clone(),
            &planned_file.snapshot_file,
        )?;

        if let RestoreDestinationState::Different(_) = current_state {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Restore requires a bakcup before replacing {}",
                    planned_file.destination_path.display()
                ),
            ));
        }
    }

    let mut restored_file_count = 0;
    let mut skipped_identical_count = 0;

    for planned_file in &files {
        if apply_planned_restore_file(planned_file)? {
            restored_file_count += 1;
        } else {
            skipped_identical_count += 1;
        }
    }

    Ok(AppliedRestore {
        snapshot_id,
        restored_file_count,
        skipped_identical_count,
        backup_snapshot_id: None,
    })
}

///result of mapping and restoring a snapshot received from another device
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedReceivedSnapshot {
    pub restore: AppliedRestore,
    pub mapped_locations: Vec<SaveLocation>,
}

///applies a restore using an explicit conflict policy
pub fn apply_restore_plan_with_policy(
    plan: RestorePlan,
    policy: RestoreConflictPolicy,
) -> Result<AppliedRestore, io::Error> {
    match policy {
        RestoreConflictPolicy::Reject => apply_restore_plan(plan),

        RestoreConflictPolicy::BackupAndReplace(config) => {
            verify_restore_plan_sources(&plan.files)?;

            let backup = create_restore_backup_if_needed(&plan.files, &config)?;

            let backup = match backup {
                Some(backup) => backup,
                None => return apply_restore_plan(plan),
            };

            let backup_snapshot_id = backup.manifest.snapshot_id.clone();

            let RestorePlan { snapshot_id, files } = plan;
            let mut restored_file_count = 0;
            let mut skipped_identical_count = 0;

            for planned_file in &files {
                if replace_planned_restore_file(planned_file, &backup.manifest)? {
                    restored_file_count += 1;
                } else {
                    skipped_identical_count += 1;
                }
            }

            Ok(AppliedRestore {
                snapshot_id,
                restored_file_count,
                skipped_identical_count,
                backup_snapshot_id: (Some(backup_snapshot_id)),
            })
        }
    }
}

///creates and verifies a recovery snapshot when live conflicts exist
fn create_restore_backup_if_needed(
    files: &[PlannedRestoreFile],
    config: &RestoreBackupConfig,
) -> Result<Option<CreatedSnapshot>, io::Error> {
    let mut conflicts = Vec::new();

    for planned_file in files {
        let current_state = inspect_restore_destination(
            planned_file.destination_path.clone(),
            &planned_file.snapshot_file,
        )?;

        if let RestoreDestinationState::Different(live_fingerprint) = current_state {
            conflicts.push(RestoreConflictEvidence {
                location_id: planned_file.snapshot_file.location_id.clone(),
                relative_path: planned_file.snapshot_file.relative_path.clone(),
                live_fingerprint,
            });
        }
    }

    if conflicts.is_empty() {
        return Ok(None);
    }

    let mut existing_locations = Vec::new();

    for status in inspect_save_locations(config.locations.clone()) {
        match status.state {
            SaveLocationState::File | SaveLocationState::Directory => {
                existing_locations.push(status.location);
            }

            SaveLocationState::Missing => {}

            SaveLocationState::Unreadable(message) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("Cannot back up {}: {message}", status.location.label),
                ));
            }

            SaveLocationState::Unsupported => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Cannot back up unspported location {}",
                        status.location.label
                    ),
                ));
            }
        }
    }

    if existing_locations.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Conflicting saves exist but no backup locations are available",
        ));
    }

    let backup = create_snapshot(CreateSnapshotRequest {
        vn_sync_id: config.vn_sync_id.clone(),
        device_id: config.device_id.clone(),
        parent_snapshot_id: None,
        locations: existing_locations,
        storage_directory: config.storage_directory.clone(),
    })?;

    for conflict in conflicts {
        let mut verified = false;

        for backup_file in &backup.manifest.files {
            if backup_file.location_id == conflict.location_id
                && backup_file.relative_path == conflict.relative_path
            {
                if backup_file.size_bytes != conflict.live_fingerprint.size_bytes
                    || backup_file.content_hash != conflict.live_fingerprint.content_hash
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Live save changed while its backup was being created",
                    ));
                }

                verified = true;
                break;
            }
        }

        if !verified {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Recovery snapshot is missing {}/{}",
                    conflict.location_id, conflict.relative_path
                ),
            ));
        }
    }

    Ok(Some(backup))
}

///replaces one conflict after another, verifiying its recovery backup
fn replace_planned_restore_file(
    planned_file: &PlannedRestoreFile,
    backup_manifest: &SnapshotManifest,
) -> Result<bool, io::Error> {
    let current_state = inspect_restore_destination(
        planned_file.destination_path.clone(),
        &planned_file.snapshot_file,
    )?;

    match current_state {
        RestoreDestinationState::Missing => {
            return apply_planned_restore_file(planned_file);
        }

        RestoreDestinationState::Identical => {
            return Ok(false);
        }

        RestoreDestinationState::Different(current_fingerprint) => {
            let mut backed_up_fingerprint: Option<FileFingerprint> = None;

            for backup_file in &backup_manifest.files {
                if backup_file.location_id == planned_file.snapshot_file.location_id
                    && backup_file.relative_path == planned_file.snapshot_file.relative_path
                {
                    backed_up_fingerprint = Some(FileFingerprint {
                        size_bytes: backup_file.size_bytes,
                        content_hash: backup_file.content_hash.clone(),
                    });
                    break;
                }
            }

            let backed_up_fingerprint = match backed_up_fingerprint {
                Some(fingerprint) => fingerprint,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Recovery snapshot does not contain the conflicting save",
                    ));
                }
            };

            if current_fingerprint != backed_up_fingerprint {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Live save changed after its recovery backup was created",
                ));
            }
        }
    }

    let parent_directory = match planned_file.destination_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),

        _ => PathBuf::from("."),
    };

    let parent_metadata = fs::symlink_metadata(&parent_directory)?;

    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Restore destination parent is not a safe directory",
        ));
    }

    let source_metadata = fs::symlink_metadata(&planned_file.source_blob_path)?;

    if !source_metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Restore source blob is not a regular file",
        ));
    }

    let mut temporary_file = tempfile::NamedTempFile::new_in(&parent_directory)?;

    let copied_fingerprint = copy_and_hash_file(
        planned_file.source_blob_path.clone(),
        temporary_file.as_file_mut(),
    )?;

    if copied_fingerprint.size_bytes != planned_file.snapshot_file.size_bytes
        || copied_fingerprint.content_hash != planned_file.snapshot_file.content_hash
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Restore source changed while it was being copied",
        ));
    }

    match temporary_file.persist(&planned_file.destination_path) {
        Ok(_) => Ok(true),
        Err(error) => Err(error.error),
    }
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
        CreateSnapshotRequest, RestoreBackupConfig, RestoreConflictPolicy, RestoreDestinationState,
        RestoreSnapshotRequest, SNAPSHOT_FORMAT_VERSION, SaveLocationState, SnapshotFile,
        SnapshotManifest, apply_restore_plan, apply_restore_plan_with_policy,
        collect_location_files, create_snapshot, hash_file, inspect_restore_destination,
        inspect_save_locations, list_local_snapshots, load_or_create_device_id,
        normalize_relative_path, path_from_manifest, plan_snapshot_restore, store_blob,
        verified_blob_path,
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

    #[test]
    fn validates_portable_manifest_paths() {
        let nested_path = path_from_manifest("slot-1/save.dat".to_string())
            .expect("A safe nested path should be accepted");

        assert_eq!(
            nested_path,
            Some(std::path::PathBuf::from("slot-1").join("save.dat"))
        );

        let direct_file = path_from_manifest(String::new())
            .expect("an empty path should represent a direct save file");

        assert_eq!(direct_file, None);

        for unsafe_path in [
            "../private.dat",
            "slot/../../private.dat",
            "/absolute/save.dat",
            "slot\\..\\private.dat",
            "slot//save.dat",
            "slot/",
            "C:/Windows/save.dat",
        ] {
            let result = path_from_manifest(unsafe_path.to_string());

            assert!(
                result.is_err(),
                "unsafe path should be rejected: {unsafe_path}"
            );
        }
    }

    #[test]
    fn verifies_blobs_before_restore() {
        let test_root = std::env::temp_dir().join(format!(
            "kakera-restore-blob-verification-{}",
            std::process::id()
        ));

        let source_file = test_root.join("live-save.dat");
        let blob_directory = test_root.join("blobs");

        let _ = fs::remove_dir_all(&test_root);
        fs::create_dir_all(&test_root)
            .expect("the restore verification directory should be created");

        fs::write(&source_file, b"valid save contents").expect("the source save should be written");

        let stored_blob = store_blob(source_file, blob_directory.clone())
            .expect("the test blob should be stored");

        let snapshot_file = SnapshotFile {
            location_id: "main-saves".to_string(),
            relative_path: "save.dat".to_string(),
            size_bytes: stored_blob.fingerprint.size_bytes,
            content_hash: stored_blob.fingerprint.content_hash,
        };

        let verified_path = verified_blob_path(&snapshot_file, blob_directory.clone())
            .expect("the unchanged blob should pass verification");

        assert_eq!(verified_path, stored_blob.path);

        fs::write(&stored_blob.path, b"corrupted contents")
            .expect("the stored blob should be modified for the test");

        let corrupted_result = verified_blob_path(&snapshot_file, blob_directory.clone());

        assert!(corrupted_result.is_err());

        let unsafe_snapshot_file = SnapshotFile {
            content_hash: "../../outside-the-blob-store".to_string(),
            ..snapshot_file
        };

        let unsafe_result = verified_blob_path(&unsafe_snapshot_file, blob_directory);

        assert!(unsafe_result.is_err());

        fs::remove_dir_all(test_root)
            .expect("the restore verification directory should be removed");
    }

    #[test]
    fn plans_snapshot_restore_without_writing_live_files() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-restore-plan-{}", std::process::id()));

        let source_directory = test_root.join("source-saves");
        let source_slot = source_directory.join("slot-1");
        let destination_directory = test_root.join("restored-saves");
        let storage_directory = test_root.join("snapshot-store");

        let _ = fs::remove_dir_all(&test_root);

        fs::create_dir_all(&source_slot).expect("the source save directory should be created");

        fs::write(source_directory.join("settings.dat"), b"settings")
            .expect("the settings save should be written");

        fs::write(source_slot.join("progress.dat"), b"progress")
            .expect("the progress save should be written");

        let created = create_snapshot(CreateSnapshotRequest {
            vn_sync_id: "vn-restore-test".to_string(),
            device_id: "source-device".to_string(),
            parent_snapshot_id: None,
            locations: vec![SaveLocation {
                id: "main-saves".to_string(),
                path: source_directory.to_string_lossy().into_owned(),
                label: "Main saves".to_string(),
            }],
            storage_directory: storage_directory.clone(),
        })
        .expect("the source snapshot should be created");

        let plan = plan_snapshot_restore(RestoreSnapshotRequest {
            manifest: created.manifest,
            locations: vec![SaveLocation {
                id: "main-saves".to_string(),
                path: destination_directory.to_string_lossy().into_owned(),
                label: "Main saves".to_string(),
            }],
            storage_directory,
        })
        .expect("the restore plan should be created");

        assert_eq!(plan.files.len(), 2);

        assert_eq!(
            plan.files[0].destination_path,
            destination_directory.join("settings.dat")
        );

        assert_eq!(
            plan.files[1].destination_path,
            destination_directory.join("slot-1").join("progress.dat")
        );

        for planned_file in &plan.files {
            assert!(planned_file.source_blob_path.is_file());
            assert_eq!(
                planned_file.destination_state,
                RestoreDestinationState::Missing
            );
        }

        assert!(!destination_directory.exists());

        fs::remove_dir_all(test_root).expect("the restore plan test directory should be removed");
    }
    #[test]
    fn classifies_restore_destination_states() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-restore-destination-{}", std::process::id()));

        let expected_file = test_root.join("expected.dat");
        let destination_file = test_root.join("destination.dat");
        let expected_contents = b"snapshot save contents";

        let _ = fs::remove_dir_all(&test_root);
        fs::create_dir_all(&test_root).expect("the destination test directory should be created");

        fs::write(&expected_file, expected_contents)
            .expect("the expected save file should be written");

        let expected_fingerprint =
            hash_file(expected_file).expect("the expected save should be hashed");

        let snapshot_file = SnapshotFile {
            location_id: "main-saves".to_string(),
            relative_path: "destination.dat".to_string(),
            size_bytes: expected_fingerprint.size_bytes,
            content_hash: expected_fingerprint.content_hash,
        };

        let missing_state = inspect_restore_destination(destination_file.clone(), &snapshot_file)
            .expect("a missing destination should be inspected");

        assert_eq!(missing_state, RestoreDestinationState::Missing);

        fs::write(&destination_file, expected_contents)
            .expect("the identical destination should be written");

        let identical_state = inspect_restore_destination(destination_file.clone(), &snapshot_file)
            .expect("the identical destination should be inspected");

        assert_eq!(identical_state, RestoreDestinationState::Identical);

        fs::write(&destination_file, b"newer live save contents")
            .expect("the different destination should be written");

        let different_state = inspect_restore_destination(destination_file, &snapshot_file)
            .expect("the different destination should be inspected");

        match different_state {
            RestoreDestinationState::Different(current_fingerprint) => {
                assert_ne!(current_fingerprint.content_hash, snapshot_file.content_hash);
            }

            other_state => {
                panic!("expected a different destination, got {other_state:?}");
            }
        }

        fs::remove_dir_all(test_root).expect("the destination test directory should be removed");
    }

    #[test]
    fn applies_retries_and_refuses_conflicting_restores() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-apply-restore-{}", std::process::id()));

        let source_directory = test_root.join("source");
        let destination_directory = test_root.join("destination");
        let storage_directory = test_root.join("storage");
        let source_file = source_directory.join("save.dat");
        let destination_file = destination_directory.join("save.dat");

        let _ = fs::remove_dir_all(&test_root);
        fs::create_dir_all(&source_directory).expect("the source directory should be created");

        fs::write(&source_file, b"snapshot progress").expect("the source save should be written");

        let created = create_snapshot(CreateSnapshotRequest {
            vn_sync_id: "vn-apply-test".to_string(),
            device_id: "source-device".to_string(),
            parent_snapshot_id: None,
            locations: vec![SaveLocation {
                id: "main-saves".to_string(),
                path: source_directory.to_string_lossy().into_owned(),
                label: "Main saves".to_string(),
            }],
            storage_directory: storage_directory.clone(),
        })
        .expect("the snapshot should be created");

        let restore_location = SaveLocation {
            id: "main-saves".to_string(),
            path: destination_directory.to_string_lossy().into_owned(),
            label: "Main saves".to_string(),
        };

        let first_plan = plan_snapshot_restore(RestoreSnapshotRequest {
            manifest: created.manifest.clone(),
            locations: vec![restore_location.clone()],
            storage_directory: storage_directory.clone(),
        })
        .expect("the first restore should be planned");

        let first_result =
            apply_restore_plan(first_plan).expect("the first restore should be applied");

        assert_eq!(first_result.restored_file_count, 1);
        assert_eq!(first_result.skipped_identical_count, 0);
        assert_eq!(
            fs::read(&destination_file).expect("the restored save should be readable"),
            b"snapshot progress"
        );

        let retry_plan = plan_snapshot_restore(RestoreSnapshotRequest {
            manifest: created.manifest.clone(),
            locations: vec![restore_location.clone()],
            storage_directory: storage_directory.clone(),
        })
        .expect("the retry should be planned");

        let retry_result = apply_restore_plan(retry_plan).expect("the retry should safely succeed");

        assert_eq!(retry_result.restored_file_count, 0);
        assert_eq!(retry_result.skipped_identical_count, 1);

        fs::write(&destination_file, b"newer live progress")
            .expect("the live save should be changed");

        let conflict_plan = plan_snapshot_restore(RestoreSnapshotRequest {
            manifest: created.manifest,
            locations: vec![restore_location],
            storage_directory,
        })
        .expect("the conflicting restore should still be planned");

        let conflict_result = apply_restore_plan(conflict_plan);

        assert!(conflict_result.is_err());

        assert_eq!(
            fs::read(&destination_file).expect("the conflicting save should remain readable"),
            b"newer live progress"
        );

        fs::remove_dir_all(test_root).expect("the apply restore test directory should be removed");
    }

    #[test]
    fn backs_up_and_replaces_conflicting_live_saves() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-backup-replace-{}", std::process::id()));

        let source_directory = test_root.join("source");
        let destination_directory = test_root.join("destination");
        let storage_directory = test_root.join("storage");

        let source_file = source_directory.join("save.dat");
        let destination_file = destination_directory.join("save.dat");

        let _ = fs::remove_dir_all(&test_root);

        fs::create_dir_all(&source_directory).expect("the source directory should be created");

        fs::create_dir_all(&destination_directory)
            .expect("the destination directory should be created");

        fs::write(&source_file, b"snapshot progress")
            .expect("the snapshot source should be written");

        let snapshot = create_snapshot(CreateSnapshotRequest {
            vn_sync_id: "vn-backup-test".to_string(),
            device_id: "remote-device".to_string(),
            parent_snapshot_id: None,
            locations: vec![SaveLocation {
                id: "main-saves".to_string(),
                path: source_directory.to_string_lossy().into_owned(),
                label: "Main saves".to_string(),
            }],
            storage_directory: storage_directory.clone(),
        })
        .expect("the remote snapshot should be created");

        fs::write(&destination_file, b"newer live progress")
            .expect("the conflicting live save should be written");

        let local_location = SaveLocation {
            id: "main-saves".to_string(),
            path: destination_directory.to_string_lossy().into_owned(),
            label: "Main saves".to_string(),
        };

        let plan = plan_snapshot_restore(RestoreSnapshotRequest {
            manifest: snapshot.manifest,
            locations: vec![local_location.clone()],
            storage_directory: storage_directory.clone(),
        })
        .expect("the conflicting restore should be planned");

        let applied = apply_restore_plan_with_policy(
            plan,
            RestoreConflictPolicy::BackupAndReplace(RestoreBackupConfig {
                vn_sync_id: "vn-backup-test".to_string(),
                device_id: "local-device".to_string(),
                locations: vec![local_location],
                storage_directory: storage_directory.clone(),
            }),
        )
        .expect("the conflicting restore should be backed up and applied");

        assert_eq!(applied.restored_file_count, 1);
        assert_eq!(applied.skipped_identical_count, 0);

        assert_eq!(
            fs::read(&destination_file).expect("the replaced live save should be readable"),
            b"snapshot progress"
        );

        let backup_snapshot_id = applied
            .backup_snapshot_id
            .expect("a recovery snapshot ID should be returned");

        let backup_manifest_path = storage_directory
            .join("manifests")
            .join(format!("{backup_snapshot_id}.json"));

        let backup_manifest_bytes =
            fs::read(backup_manifest_path).expect("the recovery manifest should be readable");

        let backup_manifest: SnapshotManifest = serde_json::from_slice(&backup_manifest_bytes)
            .expect("the recovery manifest should contain valid JSON");

        assert_eq!(backup_manifest.files.len(), 1);

        let backup_file = &backup_manifest.files[0];
        let backup_blob_path = storage_directory
            .join("blobs")
            .join(format!("{}.blob", backup_file.content_hash));

        assert_eq!(
            fs::read(backup_blob_path).expect("the recovery blob should be readable"),
            b"newer live progress"
        );

        fs::remove_dir_all(test_root).expect(
            "the backup replacement test directory should be
          removed",
        );
    }

    #[test]
    fn lists_local_snapshots_and_rejects_tampering() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-snapshot-catalog-{}", std::process::id()));

        let source_directory = test_root.join("saves");
        let source_file = source_directory.join("save.dat");
        let storage_directory = test_root.join("storage");

        let _ = fs::remove_dir_all(&test_root);

        let empty_catalog = list_local_snapshots(storage_directory.clone()).expect(
            "missing snapshot storage should be an empty
              catalog",
        );

        assert!(empty_catalog.is_empty());

        fs::create_dir_all(&source_directory).expect("the source directory should be created");

        fs::write(&source_file, b"first progress").expect("the first save should be written");

        let location = SaveLocation {
            id: "main-saves".to_string(),
            path: source_directory.to_string_lossy().into_owned(),
            label: "Main saves".to_string(),
        };

        let first = create_snapshot(CreateSnapshotRequest {
            vn_sync_id: "vn-catalog-test".to_string(),
            device_id: "test-device".to_string(),
            parent_snapshot_id: None,
            locations: vec![location.clone()],
            storage_directory: storage_directory.clone(),
        })
        .expect("the first snapshot should be created");

        std::thread::sleep(std::time::Duration::from_millis(2));

        fs::write(&source_file, b"second progress").expect("the second save should be written");

        let second = create_snapshot(CreateSnapshotRequest {
            vn_sync_id: "vn-catalog-test".to_string(),
            device_id: "test-device".to_string(),
            parent_snapshot_id: Some(first.manifest.snapshot_id.clone()),
            locations: vec![location],
            storage_directory: storage_directory.clone(),
        })
        .expect("the second snapshot should be created");

        let catalog = list_local_snapshots(storage_directory.clone())
            .expect("the snapshot catalog should load");

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].snapshot_id, second.manifest.snapshot_id);
        assert_eq!(catalog[1].snapshot_id, first.manifest.snapshot_id);

        let mut tampered_manifest = first.manifest;
        tampered_manifest.device_id = "tampered-device".to_string();

        let tampered_json = serde_json::to_vec_pretty(&tampered_manifest)
            .expect("the tampered manifest should serialize");

        fs::write(first.manifest_path, tampered_json)
            .expect("the manifest should be tampered with");

        let tampered_catalog = list_local_snapshots(storage_directory);

        assert!(tampered_catalog.is_err());

        fs::remove_dir_all(test_root).expect("the catalog test directory should be removed");
    }

    #[test]
    fn creates_and_reuses_one_device_id() {
        let test_root =
            std::env::temp_dir().join(format!("kakera-device-id-{}", std::process::id()));

        let _ = fs::remove_dir_all(&test_root);

        let first_id = load_or_create_device_id(test_root.clone())
            .expect("the first device id should be created");

        let second_id = load_or_create_device_id(test_root.clone())
            .expect("the stored device id should be loaded");

        assert_eq!(first_id, second_id);
        assert!(first_id.starts_with("device-"));

        let _ = fs::remove_dir_all(test_root);
    }
}
