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

///checks if a command is available 
pub fn host_command_exists(command_name: String) -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }

    let output = if std::env::var_os("FLATPAK_ID").is_some() {
        Command::new("flatpak-spawn")
            .args([
                "--host",
                "sh",
                "-lc",
                &format!("command -v -- {command_name}"),
            ])
            .output()
    } else {
        Command::new("sh")
            .args(["-lc", &format!("command -v -- {command_name}")])
            .output()
    };

    match output {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}
