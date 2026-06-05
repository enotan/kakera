use std::io;
use std::path::Path;
use std::process::Command;

///opens a folder
pub fn open_folder(path: &Path) -> Result<(), io::Error> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command.spawn()?;

    Ok(())
}