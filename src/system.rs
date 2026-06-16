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

///finds the absolute path of a command
pub fn find_host_command(command_name: String) -> Option<String> {
    let output = if std::env::var_os("FLATPAK_ID").is_some() {
        Command::new("flatpak-spawn")
            .args([
                "--host",
                "sh",
                "-c",
                "command -v -- \"$1\"",
                "sh",
                &command_name,
            ])
            .output()
    } else {
        Command::new("sh")
            .args(["-lc", "command -v -- \"$1\"", "sh", &command_name])
            .output()
    }
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();

    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

///return true when a path is coming from /run/flatpak/doc/
pub fn is_flatpak_document_portal_path(path: &str) -> bool {
    path.starts_with("/run/flatpak/doc/")
}
