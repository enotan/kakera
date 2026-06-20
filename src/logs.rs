use std::fs;
use std::io;
use std::path::{Path, PathBuf};

///returns the folder where logs are stored
pub fn launch_logs_dir() -> Result<PathBuf, io::Error> {
    let data_dir = match dirs::data_dir() {
        Some(path) => path,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Could not find an app data directory",
            ));
        }
    };

    let logs_dir = data_dir.join("kakera").join("logs");

    fs::create_dir_all(&logs_dir)?;

    Ok(logs_dir)
}

///returns the path used for the most recent log
pub fn latest_launch_log_path() -> Result<PathBuf, io::Error> {
    Ok(launch_logs_dir()?.join("latest.log"))
}

///copies a log to latest.log
pub fn update_latest_launch_log(log_path: &Path) -> Result<(), io::Error> {
    let latest_log = latest_launch_log_path()?;
    fs::copy(log_path, latest_log)?;

    Ok(())
}

///makes a new log path for a vn launch
pub fn new_launch_log_path(vn_id: u64, vn_title: String) -> Result<PathBuf, io::Error> {
    let logs_dir = launch_logs_dir()?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let safe_title = safe_filename(vn_title);

    Ok(logs_dir.join(format!("{timestamp}-vn-{vn_id}-{safe_title}.log")))
}

///converts a vn title into a filename safe ver
fn safe_filename(title: String) -> String {
    let mut safe = String::new();

    for character in title.chars().take(48) {
        if character.is_ascii_alphanumeric() {
            safe.push(character);
        } else if character == '-' || character == '_' {
            safe.push(character);
        } else if character.is_whitespace() {
            safe.push('-');
        } else {
            safe.push('_');
        }
    }

    if safe.is_empty() {
        "untitled".to_string()
    } else {
        safe
    }
}
