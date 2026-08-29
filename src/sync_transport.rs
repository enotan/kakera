use crate::{
    save_sync::{VerifiedLocalBlob, hash_file},
    sync_protocol::{MAX_CONTROL_MESSAGE_BYTES, SyncMessage, decode_message, encode_message},
};
use iroh::endpoint::{RecvStream, SendStream};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
};

const MESSAGE_LENGTH_BYTES: usize = 4;
const BLOB_BUFFER_SIZE: usize = 64 * 1024;
pub const MAX_BLOB_BYTES: u64 = 512 * 1024 * 1024;

///writes a message with a prefix of its length
pub async fn send_control_message(
    send_stream: &mut SendStream,
    message: &SyncMessage,
) -> Result<(), io::Error> {
    let encoded_message = encode_message(message).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("The sync message could not be encoded: {error}"),
        )
    })?;

    if encoded_message.len() > MAX_CONTROL_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The sync control message exceeds the size limit",
        ));
    }

    let message_length = u32::try_from(encoded_message.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "The sync control message length cannot fit in its prefix",
        )
    })?;

    send_stream
        .write_all(&message_length.to_be_bytes())
        .await
        .map_err(io::Error::other)?;

    send_stream
        .write_all(&encoded_message)
        .await
        .map_err(io::Error::other)?;

    Ok(())
}

///reads and validates a control message
pub async fn receive_control_message(
    receive_stream: &mut RecvStream,
) -> Result<SyncMessage, io::Error> {
    let mut length_bytes = [0_u8; MESSAGE_LENGTH_BYTES];

    receive_stream
        .read_exact(&mut length_bytes)
        .await
        .map_err(io::Error::other)?;

    let message_length = u32::from_be_bytes(length_bytes) as usize;

    if message_length > MAX_CONTROL_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The peer sent an oversized sync control message...",
        ));
    }

    let mut encoded_message = vec![0_u8; message_length];

    receive_stream
        .read_exact(&mut encoded_message)
        .await
        .map_err(io::Error::other)?;

    decode_message(encoded_message).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("The peer sent an invalid sync message: {error}"),
        )
    })
}

///streams one verified local blob to a peer
pub async fn send_blob_bytes(
    send_stream: &mut SendStream,
    blob: &VerifiedLocalBlob,
) -> Result<(), io::Error> {
    let mut source_file = File::open(&blob.path)?;
    let mut buffer = [0_u8; BLOB_BUFFER_SIZE];
    let mut sent_bytes = 0_u64;
    let mut hasher = blake3::Hasher::new();

    loop {
        let bytes_read = source_file.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];

        send_stream
            .write_all(chunk)
            .await
            .map_err(io::Error::other)?;

        hasher.update(chunk);
        sent_bytes += bytes_read as u64;
    }

    let sent_hash = hasher.finalize().to_hex().to_string();

    if sent_bytes != blob.size_bytes || sent_hash != blob.content_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The local blob changed during transmission!!",
        ));
    }

    Ok(())
}

///receives exactly one announced blob and safely stores it by content hash
pub async fn receive_blob_bytes(
    receive_stream: &mut RecvStream,
    content_hash: String,
    size_bytes: u64,
    destination_dir: PathBuf,
) -> Result<PathBuf, io::Error> {
    if size_bytes > MAX_BLOB_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The peer announced a blob larger than Kakera's limit",
        ));
    }

    let hash_is_valid = content_hash.len() == 64
        && content_hash
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character));

    if !hash_is_valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The peer announced an invalid blob hash",
        ));
    }

    std::fs::create_dir_all(&destination_dir)?;

    let destination_path = destination_dir.join(format!("{content_hash}.blob"));

    let mut temp_file = tempfile::NamedTempFile::new_in(&destination_dir)?;

    let mut buffer = [0_u8; BLOB_BUFFER_SIZE];
    let mut remaining_bytes = size_bytes;
    let mut hasher = blake3::Hasher::new();

    while remaining_bytes > 0 {
        let next_chunk_size = remaining_bytes.min(BLOB_BUFFER_SIZE as u64) as usize;

        let chunk = &mut buffer[..next_chunk_size];

        receive_stream
            .read_exact(chunk)
            .await
            .map_err(io::Error::other)?;

        temp_file.write_all(chunk)?;
        hasher.update(chunk);
        remaining_bytes -= next_chunk_size as u64;
    }

    temp_file.as_file_mut().sync_all()?;

    let received_hash = hasher.finalize().to_hex().to_string();

    if received_hash != content_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The received blob failed integrity verification",
        ));
    }

    match temp_file.persist_noclobber(&destination_path) {
        Ok(_) => Ok(destination_path),

        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let existing_fingerprint = hash_file(destination_path.clone())?;

            if existing_fingerprint.content_hash != content_hash
                || existing_fingerprint.size_bytes != size_bytes
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The existing received blob failed verification",
                ));
            }

            Ok(destination_path)
        }

        Err(error) => Err(error.error),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        models::{SaveLocation, new_save_sync_id},
        save_sync::{
            create_local_snapshot_for_vn, list_local_snapshots_for_vn, verified_local_blob_for_vn,
        },
        sync_peer::{PeerConnectionInfo, bind_peer_endpoint, connect_to_peer},
        sync_protocol::{PeerHello, SYNC_PROTOCOL_VERSION, SnapshotDescriptor, SyncMessage},
        sync_session::{receive_latest_snapshot_from_peer, serve_sync_connection},
    };

    #[tokio::test]
    async fn exchange_snapshot_manifest_between_direct_peers() {
        let server_directory = tempfile::tempdir().expect("the server directory should be created");
        let client_directory = tempfile::tempdir().expect("the client directory should be created");

        let server = bind_peer_endpoint(server_directory.path().to_path_buf())
            .await
            .expect("the server endpoint should bind");

        let client = bind_peer_endpoint(client_directory.path().to_path_buf())
            .await
            .expect("the client endpoint should bind");

        let server_info = PeerConnectionInfo::from_endpoint(&server);

        let vn_sync_id = new_save_sync_id(42);

        let live_save_dir = server_directory.path().join("live-saves");
        std::fs::create_dir_all(&live_save_dir).expect("The live save directory should be created");

        let live_save_path = live_save_dir.join("slot1.sav");
        std::fs::write(&live_save_path, b"chapter 3 save data")
            .expect("The live save should be written");

        let created_snapshot = create_local_snapshot_for_vn(
            vn_sync_id.clone(),
            vec![SaveLocation {
                id: "main-save".to_string(),
                path: live_save_path.to_string_lossy().into_owned(),
                label: "Main save".to_string(),
            }],
            server_directory.path().to_path_buf(),
        )
        .expect("The server snapshot should be created");

        let expected_descriptor = SnapshotDescriptor::from_manifest(&created_snapshot.manifest);

        let expected_manifest = created_snapshot.manifest.clone();

        let client_hello_data = PeerHello {
            protocol_version: SYNC_PROTOCOL_VERSION,
            device_id: "client-device".to_string(),
            device_name: "Steam Deck".to_string(),
        };

        let server_hello_data = PeerHello {
            protocol_version: SYNC_PROTOCOL_VERSION,
            device_id: "server-device".to_string(),
            device_name: "Desktop".to_string(),
        };

        let server_hello = SyncMessage::Hello(server_hello_data.clone());

        let server_conversation = async {
            let incoming = server
                .accept()
                .await
                .expect("The server should receive a connection");

            let connection = incoming
                .await
                .expect("The encrypted handshake should succeed");

            let completed = serve_sync_connection(
                connection,
                server_hello_data,
                server_directory.path().to_path_buf(),
            )
            .await
            .expect("The production server should complete the transfer");

            assert_eq!(completed.vn_sync_id, vn_sync_id);
            assert_eq!(completed.snapshot_id, expected_manifest.snapshot_id);
        };

        let client_conversation = async {
            let connection = connect_to_peer(&client, server_info)
                .await
                .expect("The client should connect directly");

            let received = receive_latest_snapshot_from_peer(
                connection,
                client_hello_data,
                vn_sync_id.clone(),
                client_directory.path().to_path_buf(),
            )
            .await
            .expect("The production client should receive the latest snapshot");

            assert_eq!(received.manifest, expected_manifest);
            assert_eq!(received.downloaded_blob_count, 1);

            let client_snapshots = list_local_snapshots_for_vn(
                vn_sync_id.clone(),
                client_directory.path().to_path_buf(),
            )
            .expect("The client snapshot inventory should load");

            assert_eq!(client_snapshots, vec![expected_manifest.clone()]);

            let received_blob = verified_local_blob_for_vn(
                vn_sync_id.clone(),
                expected_manifest.files[0].content_hash.clone(),
                client_directory.path().to_path_buf(),
            )
            .expect("The transferred blob should be stored and valid");

            let received_bytes =
                std::fs::read(received_blob.path).expect("The transferred blob should be readable");

            assert_eq!(received_bytes, b"chapter 3 save data");
        };

        tokio::join!(server_conversation, client_conversation);

        client.close().await;
        server.close().await;
    }
}
