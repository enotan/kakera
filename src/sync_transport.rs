use crate::sync_protocol::{
    MAX_CONTROL_MESSAGE_BYTES, SyncMessage, decode_message, encode_message,
};
use iroh::endpoint::{RecvStream, SendStream};
use std::io;

const MESSAGE_LENGTH_BYTES: usize = 4;

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

#[cfg(test)]
mod tests {
    use super::{receive_control_message, send_control_message};
    use crate::{
        sync_peer::{PeerConnectionInfo, bind_peer_endpoint, connect_to_peer},
        sync_protocol::{PeerHello, SYNC_PROTOCOL_VERSION, SyncMessage},
    };

    #[tokio::test]
    async fn exchanges_hello_messages_between_direct_peers() {
        let server_directory = tempfile::tempdir().expect("the server directory should be created");
        let client_directory = tempfile::tempdir().expect("the client directory should be created");

        let server = bind_peer_endpoint(server_directory.path().to_path_buf())
            .await
            .expect("the server endpoint should bind");

        let client = bind_peer_endpoint(client_directory.path().to_path_buf())
            .await
            .expect("the client endpoint should bind");

        let server_info = PeerConnectionInfo::from_endpoint(&server);

        let client_hello = SyncMessage::Hello(PeerHello {
            protocol_version: SYNC_PROTOCOL_VERSION,
            device_id: "client-device".to_string(),
            device_name: "Steam Deck".to_string(),
        });

        let server_hello = SyncMessage::Hello(PeerHello {
            protocol_version: SYNC_PROTOCOL_VERSION,
            device_id: "server-device".to_string(),
            device_name: "Desktop".to_string(),
        });

        let server_conversation = async {
            let incoming = server
                .accept()
                .await
                .expect("the server should receive a connection");

            let connection = incoming
                .await
                .expect("the encrypted handshake should succeed");

            let (mut send_stream, mut receive_stream) = connection
                .accept_bi()
                .await
                .expect("the server should accept the control stream");

            let received_hello = receive_control_message(&mut receive_stream)
                .await
                .expect("the server should receive the client hello");

            assert_eq!(received_hello, client_hello);

            send_control_message(&mut send_stream, &server_hello)
                .await
                .expect("the server should send its hello");

            send_stream
                .finish()
                .expect("the server should finish its response stream");

            let stop_code = send_stream
                .stopped()
                .await
                .expect("waiting for the peer acknowledgement should succeed PLEASE");
            assert_eq!(stop_code, None);
        };

        let client_conversation = async {
            let connection = connect_to_peer(&client, server_info)
                .await
                .expect("the client should connect directly");

            let (mut send_stream, mut receive_stream) = connection
                .open_bi()
                .await
                .expect("the client should open a control stream");

            send_control_message(&mut send_stream, &client_hello)
                .await
                .expect("the client should send its hello");

            let received_hello = receive_control_message(&mut receive_stream)
                .await
                .expect("the client should receive the server hello");

            assert_eq!(received_hello, server_hello);

            send_stream
                .finish()
                .expect("the client should finish its request stream");

            let stop_code = send_stream
                .stopped()
                .await
                .expect("waiting for the peer acknowledgement should succeed PLEASE");
            assert_eq!(stop_code, None);
        };

        tokio::join!(server_conversation, client_conversation);

        client.close().await;
        server.close().await;
    }
}
