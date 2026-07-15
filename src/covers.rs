use std::fs;
use std::io;
use std::path::PathBuf;
///returns the folder where cover cache is stored
pub fn cover_cache_dir() -> Result<PathBuf, io::Error> {
    let data_dir = match dirs::data_dir() {
        Some(path) => path,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Could not find an app data directory",
            ));
        }
    };
    let cache_dir = data_dir.join("kakera").join("covers");
    fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}
///downloads a cover into the cache dir, returns the file path
pub async fn cache_cover_image(
    vn_id: u64,
    cover_url: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let cache_dir = cover_cache_dir()?;
    let cover_file = cache_dir.join(format!("{vn_id}.jpg"));
    let image_bytes = reqwest::get(cover_url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    fs::write(&cover_file, image_bytes)?;
    Ok(cover_file.to_string_lossy().to_string())
}
