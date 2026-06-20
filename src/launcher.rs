use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::models::LaunchMode;
use crate::system::find_host_command;

///launch the executable
pub fn launch_executable(
    executable_path: String,
    launch_mode: LaunchMode,
    wine_binary: Option<String>,
    wine_prefix: Option<String>,
    wine_locale: Option<String>,
    proton_path: Option<String>,
    umu_game_id: String,
    launch_arguments: String,
    launch_environment: Vec<(String, String)>,
    launch_log_path: Option<PathBuf>,
) -> Result<Child, io::Error> {
    let path = PathBuf::from(executable_path);
    let working_directory = path.parent().map(Path::to_path_buf);

    let mut stdout_log = None;
    let mut stderr_log = None;

    if let Some(log_path) = launch_log_path {
        stdout_log = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?,
        );

        stderr_log = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?,
        );
    }

    //native runs the app as normal, and wine through wine ofc
    let child: Child = match launch_mode {
        LaunchMode::Native => {
            let mut command = host_command(
                path.to_string_lossy().to_string(),
                launch_environment.clone(),
                working_directory.clone(),
            );

            attach_launch_log(&mut command, stdout_log, stderr_log);

            command.spawn()?
        }
        LaunchMode::Wine => {
            let wine_command = wine_binary.unwrap_or_else(|| "wine".to_string());
            let mut environment = launch_environment.clone();

            if let Some(prefix) = wine_prefix {
                environment.push(("WINEPREFIX".to_string(), prefix));
            }

            if let Some(locale) = wine_locale {
                environment.push(("LANG".to_string(), locale.clone()));
                environment.push(("LC_ALL".to_string(), locale));
            }

            let mut command = host_command(wine_command, environment, working_directory.clone());

            command.arg(path);

            command.args(launch_arguments.split_whitespace());

            attach_launch_log(&mut command, stdout_log, stderr_log);

            command.spawn()?
        }
        LaunchMode::Proton => {
            //use umu launcher
            let mut environment = launch_environment.clone();

            if let Some(prefix) = wine_prefix {
                environment.push(("WINEPREFIX".to_string(), prefix));
            }

            if let Some(proton_path) = proton_path {
                environment.push(("PROTONPATH".to_string(), proton_path));
            }

            if let Some(locale) = wine_locale {
                environment.push(("LANG".to_string(), locale.clone()));
                environment.push(("LC_ALL".to_string(), locale));
            }

            environment.push(("GAMEID".to_string(), umu_game_id));

            let mut command = if std::env::var_os("FLATPAK_ID").is_some() {
                let mut command = Command::new("/app/bin/umu-run");

                if let Some(directory) = working_directory {
                    command.current_dir(directory);
                }

                for (name, value) in environment {
                    command.env(name, value);
                }

                command
            } else {
                let umu_path = find_host_command("umu-run".to_string()).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "UMU Launcher not found")
                })?;

                host_command(umu_path, environment, working_directory)
            };

            command.arg(path);
            command.args(launch_arguments.split_whitespace());

            attach_launch_log(&mut command, stdout_log, stderr_log);

            command.spawn()?
        }
    };

    Ok(child)
}

///for running host commands with flatpak
fn host_command(
    program: String,
    environment: Vec<(String, String)>,
    working_directory: Option<PathBuf>,
) -> Command {
    if std::env::var_os("FLATPAK_ID").is_some() {
        let mut command = Command::new("flatpak-spawn");
        command.arg("--host");

        if let Ok(display) = std::env::var("DISPLAY") {
            command.arg(format!("--env=DISPLAY={display}"));
        }

        if let Some(directory) = working_directory {
            command.arg(format!("--directory={}", directory.to_string_lossy()));
        }

        for (name, value) in environment {
            command.arg(format!("--env={name}={value}"));
        }

        command.arg(program);
        command
    } else {
        let mut command = Command::new(program);

        if let Some(directory) = working_directory {
            command.current_dir(directory);
        }

        for (name, value) in environment {
            command.env(name, value);
        }

        command
    }
}

fn attach_launch_log(
    command: &mut Command,
    stdout_log: Option<std::fs::File>,
    stderr_log: Option<std::fs::File>,
) {
    if let Some(file) = stdout_log {
        command.stdout(Stdio::from(file));
    }

    if let Some(file) = stderr_log {
        command.stderr(Stdio::from(file));
    }
}

///parse env vars into real env vars
pub fn parse_launch_environment(environment_text: String) -> Result<Vec<(String, String)>, String> {
    let mut environment = Vec::new();

    for (index, line) in environment_text.lines().enumerate() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            return Err(format!("Line {} must be KEY=value", index + 1));
        };

        let name = name.trim();
        let value = value.trim();

        if name.is_empty() {
            return Err(format!("Line {} has an empty variable name", index + 1));
        }

        if name.contains(char::is_whitespace) {
            return Err(format!(
                "Line {} has whitespace in the variable name",
                index + 1
            ));
        }

        environment.push((name.to_string(), value.to_string()));
    }

    Ok(environment)
}