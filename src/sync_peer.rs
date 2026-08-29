use iroh::{
    Endpoint, EndpointAddr, EndpointId, SecretKey,
    endpoint::{Connection, presets},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
};

const PEER_SECRET_FILE_NAME: &str = "peer-secret-key";
///something for QUIC or whatever idk the docs told me i need it
pub const KAKERA_SYNC_ALPN: &[u8] = b"xyz.majou.kakera/save-sync/1";

///the info needed to be shared to connect to another peer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerConnectionInfo {
    pub endpoint_id: String,
    pub direct_addresses: Vec<String>,
}

///loads the install's peer secret key or creates if needs one.
///should stay same between launches but uhh if they delete it and reinstall idk
pub fn load_or_create_peer_secret(sync_storage_directory: PathBuf) -> Result<SecretKey, io::Error> {
    fs::create_dir_all(&sync_storage_directory)?;

    let secret_path = sync_storage_directory.join(PEER_SECRET_FILE_NAME);

    match fs::read(&secret_path) {
        Ok(bytes) => decode_peer_secret(bytes),

        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_peer_secret(secret_path, sync_storage_directory)
        }

        Err(error) => Err(error),
    }
}

///starts the peer endpoint
///using presets::Minimal just starts it without public address lookup stuff,
///so it only works on LAN or with tailscale
pub async fn bind_peer_endpoint(sync_storage_directory: PathBuf) -> Result<Endpoint, io::Error> {
    let secret = load_or_create_peer_secret(sync_storage_directory)?;

    Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .alpns(vec![KAKERA_SYNC_ALPN.to_vec()])
        .bind()
        .await
        .map_err(io::Error::other)
}

fn decode_peer_secret(bytes: Vec<u8>) -> Result<SecretKey, io::Error> {
    let key_bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "The save sync peer secret must be exactly 32 bytes",
        )
    })?;

    Ok(SecretKey::from_bytes(&key_bytes))
}

impl PeerConnectionInfo {
    ///creates the info to be shared
    pub fn from_endpoint(endpoint: &Endpoint) -> Self {
        let endpoint_address = endpoint.addr();
        let mut direct_addresses = Vec::new();

        for address in endpoint_address.ip_addrs() {
            direct_addresses.push(address.to_string());
        }

        Self {
            endpoint_id: endpoint.id().to_string(),
            direct_addresses,
        }
    }

    ///validates the pairing info and converts it for iroh
    pub fn to_endpoint_addr(&self) -> Result<EndpointAddr, io::Error> {
        let endpoint_id: EndpointId = self.endpoint_id.parse().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("The peer endpoint id is invalid: {error}"),
            )
        })?;

        if self.direct_addresses.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The peer did not provide a direct address",
            ));
        }

        let mut endpoint_address = EndpointAddr::new(endpoint_id);

        for address_text in &self.direct_addresses {
            let address: SocketAddr = address_text.parse().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("The peer address '{address_text}' is invalid: {error}"),
                )
            })?;

            endpoint_address = endpoint_address.with_ip_addr(address);
        }

        Ok(endpoint_address)
    }

    ///encodes the endpoint as like, a "pairing code"? i guess that another kakera install can use to pair
    pub fn to_pairing_code(&self) -> Result<String, io::Error> {
        serde_json::to_string(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    ///decodes and validates the pairing info which was copied from another kakera install
    pub fn from_pairing_code(pairing_code: String) -> Result<Self, io::Error> {
        let peer: Self = serde_json::from_str(pairing_code.trim()).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("The pairing code is invalid: {error}"),
            )
        })?;

        peer.to_endpoint_addr()?;

        Ok(peer)
    }
}

///opens a connetion to a paired peer
pub async fn connect_to_peer(
    endpoint: &Endpoint,
    peer: PeerConnectionInfo,
) -> Result<Connection, io::Error> {
    let peer_address = peer.to_endpoint_addr()?;

    endpoint
        .connect(peer_address, KAKERA_SYNC_ALPN)
        .await
        .map_err(io::Error::other)
}

fn create_peer_secret(
    secret_path: PathBuf,
    sync_storage_directory: PathBuf,
) -> Result<SecretKey, io::Error> {
    let secret = SecretKey::generate();

    let mut temp_file = tempfile::NamedTempFile::new_in(sync_storage_directory)?;
    temp_file.write_all(&secret.to_bytes())?;
    temp_file.as_file_mut().sync_all()?;

    match temp_file.persist_noclobber(&secret_path) {
        Ok(_) => Ok(secret),

        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let existing_bytes = fs::read(secret_path)?;
            decode_peer_secret(existing_bytes)
        }

        Err(error) => Err(error.error),
    }
}

///everything another kakera install needs to receive vn snapshots
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncPairingInfo {
    pub peer: PeerConnectionInfo,
    pub vn_sync_id: String,
}

impl SyncPairingInfo {
    ///encodes peer and vn id into one pairing code
    pub fn to_pairing_code(&self) -> Result<String, io::Error> {
        serde_json::to_string(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    ///decodes and validates a complete vn pairing code
    pub fn from_pairing_code(pairing_code: String) -> Result<Self, io::Error> {
        let pairing: Self = serde_json::from_str(pairing_code.trim()).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("The pairing code is invalid: {error}"),
            )
        })?;

        pairing.peer.to_endpoint_addr()?;

        let hash = pairing.vn_sync_id.strip_prefix("sync-");

        let sync_id_is_valid = match hash {
            Some(hash) => {
                hash.len() == 64
                    && hash.chars().all(|character| {
                        character.is_ascii_digit() || ('a'..='f').contains(&character)
                    })
            }
            None => false,
        };

        if !sync_id_is_valid {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The pairing code contains an invalid VN sync ID",
            ));
        }

        Ok(pairing)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PeerConnectionInfo, SyncPairingInfo, bind_peer_endpoint, connect_to_peer,
        load_or_create_peer_secret,
    };

    #[test]
    fn creates_and_reuses_one_peer_secret() {
        let temp_dir = tempfile::tempdir().expect("the temporary directory should be created");

        let first_secret = load_or_create_peer_secret(temp_dir.path().to_path_buf())
            .expect("the first peer secret should be created");

        let second_secret = load_or_create_peer_secret(temp_dir.path().to_path_buf())
            .expect("the existing peer secret should be loaded");

        assert_eq!(first_secret.to_bytes(), second_secret.to_bytes());
        assert_eq!(first_secret.public(), second_secret.public());
    }

    #[tokio::test]
    async fn binds_endpoint_without_relays() {
        let temp_dir = tempfile::tempdir().expect("the temporary directory should be created");

        let endpoint = bind_peer_endpoint(temp_dir.path().to_path_buf())
            .await
            .expect("the direct peer endpoint should bind");

        assert!(!endpoint.bound_sockets().is_empty());
        assert!(endpoint.addr().relay_urls().next().is_none());
        assert_eq!(endpoint.id(), endpoint.secret_key().public());

        endpoint.close().await;
    }
    #[tokio::test]
    async fn connects_two_peers_directly() {
        let server_directory = tempfile::tempdir().expect("the server directory should be created");
        let client_directory = tempfile::tempdir().expect("the client directory should be created");

        let server = bind_peer_endpoint(server_directory.path().to_path_buf())
            .await
            .expect("the server endpoint should bind");

        let client = bind_peer_endpoint(client_directory.path().to_path_buf())
            .await
            .expect("the client endpoint should bind");

        let server_info = PeerConnectionInfo::from_endpoint(&server);
        assert!(!server_info.direct_addresses.is_empty());

        let accepting = async {
            let incoming = server
                .accept()
                .await
                .expect("the server should receive a connection");

            incoming
                .await
                .expect("the incoming encrypted handshake should succeed")
        };

        let connecting = connect_to_peer(&client, server_info);

        let (accepted_connection, connected_result) = tokio::join!(accepting, connecting);

        let connected_connection = connected_result.expect("the client should connect directly");

        assert_eq!(accepted_connection.remote_id(), client.id());
        assert_eq!(connected_connection.remote_id(), server.id());

        connected_connection.close(0_u32.into(), b"test complete");
        client.close().await;
        server.close().await;
    }

    #[test]
    fn round_trips_pairing_code() {
        let secret = iroh::SecretKey::generate();

        let original = PeerConnectionInfo {
            endpoint_id: secret.public().to_string(),
            direct_addresses: vec!["192.168.1.25:49152".to_string()],
        };

        let pairing_code = original
            .to_pairing_code()
            .expect("The pairing code should encode");

        let decoded = PeerConnectionInfo::from_pairing_code(pairing_code)
            .expect("The pairing code should decode");

        assert_eq!(decoded, original);
    }
    #[test]
    fn round_trips_complete_sync_pairing_code() {
        let secret = iroh::SecretKey::generate();

        let original = SyncPairingInfo {
            peer: PeerConnectionInfo {
                endpoint_id: secret.public().to_string(),
                direct_addresses: vec!["192.168.1.25:49152".to_string()],
            },
            vn_sync_id: crate::models::new_save_sync_id(42),
        };

        let code = original
            .to_pairing_code()
            .expect("The complete pairing code should encode");

        let decoded = SyncPairingInfo::from_pairing_code(code)
            .expect("The complete pairing code should decode");

        assert_eq!(decoded, original);
    }
}
