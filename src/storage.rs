use crate::models::VisualNovel;

use std::path::PathBuf;
use std::{fs, io};

/// Returns the path where Kakera stores the JSON file
pub fn library_file_path() -> Result<PathBuf, io::Error> {
    let data_dir = match dirs::data_dir() {
        Some(path) => path,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "could not find an app data directory.",
            ));
        }
    };

    let kakera_dir = data_dir.join("kakera");
    let library_file = kakera_dir.join("library.json");

    Ok(library_file)
}

///Loads the library from disk, returns empty if the files don't exist yet
pub fn load_library() -> Result<Vec<VisualNovel>, io::Error> {
    let library_file = library_file_path()?;

    let json_text = match fs::read_to_string(&library_file) {
        Ok(text) => text,
        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                return Ok(Vec::new());
            }

            return Err(error);
        }
    };

    let library = match serde_json::from_str::<Vec<VisualNovel>>(&json_text) {
        Ok(visual_novels) => visual_novels,
        Err(error) => {
            return Err(io::Error::new(io::ErrorKind::InvalidData, error));
        }
    };

    Ok(library)
}

///saves the library to disk as a json
pub fn save_library(library: Vec<VisualNovel>) -> Result<(), io::Error> {
    let library_file = library_file_path()?;

    let library_dir = match library_file.parent() {
        Some(path) => path,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "library file path has no parent directory",
            ));
        }
    };

    fs::create_dir_all(library_dir)?;

    let json_text = serde_json::to_string_pretty(&library)?;

    fs::write(library_file, json_text)?;

    Ok(())
}

///adds one play session to the save file
pub fn add_play_session_to_library(
    visual_novel_id: u64,
    play_session: crate::models::PlaySession,
) -> Result<(), io::Error> {
    let mut library = load_library()?;

    for visual_novel in library.iter_mut() {
        if visual_novel.id == visual_novel_id {
            visual_novel.play_sessions.push(play_session.clone());
        }
    }

    save_library(library)?;

    Ok(())
}