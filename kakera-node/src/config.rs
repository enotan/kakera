//configuration loaded from the node's env vars

use std::{env, io, path::PathBuf};

const DEFAULT_ADDRESS: &str = "127.0.0.1:47840";

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub address: String,
    pub data_directory: PathBuf,
    pub access_token: String,
}

impl NodeConfig {
    ///loads node config from env vars
    pub fn from_env() -> io::Result<Self> {
        let address = env::var("KAKERA_NODE_BIND").unwrap_or_else(|_| DEFAULT_ADDRESS.to_string());

        let data_directory = match env::var_os("KAKERA_NODE_DATA_DIR") {
            Some(path) => PathBuf::from(path),
            None => dirs::data_dir()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "the OS has no user data directory")
                })?
                .join("kakera-node"),
        };

        let access_token = env::var("KAKERA_NODE_TOKEN").map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "KAKERA_NODE_TOKEN must be set")
        })?;

        if access_token.len() < 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "KAKERA_NODE_TOKEN must contain atleast 32 characters",
            ));
        }

        Ok(Self {
            address,
            data_directory,
            access_token,
        })
    }
}
