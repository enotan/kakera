use crate::models::LaunchMode;
use crate::storage::kakera_data_dir;
use crate::system::find_host_command;
use std::fs::{self, OpenOptions};

use std::io;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(target_os = "linux")]
const STEAM_WRAPPER_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
helper="$script_dir/kakera-steam-helper"
arguments=("$@")
last_separator=-1

for index in "${!arguments[@]}"; do
    if [[ "${arguments[$index]}" == "--" ]]; then
        last_separator=$index
    fi
done

if (( last_separator < 0 )); then
    echo "Kakera wrapper: Steam command did not contain a --
    separator." >&2
    exit 1
fi

if [[ ! -x "$helper" ]]; then
    echo "Kakera wrapper: helper is missing or not executable:
    $helper" >&2
    exit 1
fi

runtime_arguments=("${arguments[@]:0:last_separator + 1}")
game_arguments=("${arguments[@]:last_separator + 1}")

exec "${runtime_arguments[@]}" "$helper" "${game_arguments[@]}"
"#;

#[cfg(target_os = "linux")]
const STEAM_HELPER_SCRIPT: &str = r#"#!/usr/bin/env bash
set -u

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

app_id="${STEAM_COMPAT_APP_ID:-}"

if [[ -z "$app_id" ]]; then
    echo "Kakera helper: Steam App ID is unavailable." >&2
    exit 1
fi

request_file="$script_dir/tool-request-$app_id"
active_file="$script_dir/active-$app_id"
log_file="$script_dir/helper-$app_id.log"


if [[ "$#" -lt 3 ]]; then
    echo "Kakera helper: incomplete Proton command." >> "$log_file"
    exit 1
fi

proton_runner="$1"
shift

rm -f "$request_file"
touch "$active_file"
trap 'rm -f "$active_file" "$request_file"' EXIT

"$proton_runner" "$@" >> "$log_file" 2>&1 &
game_process=$!

while kill -0 "$game_process" 2>/dev/null; do
    if [[ -s "$request_file" ]]; then
        tool_path="$(head -n 1 "$request_file")"
        rm -f "$request_file"

        echo "Launching tool: $tool_path" >> "$log_file"
        "$proton_runner" run "$tool_path" >> "$log_file" 2>&1 &
    fi

    sleep 0.25
done

wait "$game_process"
"#;

///launch the executable
pub fn launch_executable(
    executable_path: String,
    launch_mode: LaunchMode,
    steam_app_id: Option<u32>,
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
        LaunchMode::Steam => {
            let app_id = steam_app_id.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Steam App ID is not configured",
                )
            })?;

            let mut command = if cfg!(target_os = "windows") {
                let mut command = Command::new("cmd");
                command
                    .arg("/C")
                    .arg("start")
                    .arg("")
                    .arg(format!("steam://rungameid/{app_id}"));
                command
            } else {
                let mut command =
                    host_command("steam".to_string(), launch_environment, working_directory);
                command.arg("-applaunch").arg(app_id.to_string());
                command
            };

            attach_launch_log(&mut command, stdout_log, stderr_log);
            command.spawn()?
        }
    };
    Ok(child)
}

///describes how a tool was started
pub enum ToolLaunch {
    Process(Child),
    Queued,
}

///launches tools inside a steam game's proton environment
pub fn launch_steam_tool(
    tool_path: String,
    app_id: u32,
    launch_environment: Vec<(String, String)>,
    launch_log_path: Option<PathBuf>,
) -> Result<ToolLaunch, io::Error> {
    let path = PathBuf::from(tool_path);
    let working_directory = path.parent().map(Path::to_path_buf);

    if cfg!(target_os = "windows") {
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

        let mut command = host_command(
            path.to_string_lossy().to_string(),
            launch_environment,
            working_directory,
        );

        attach_launch_log(&mut command, stdout_log, stderr_log);
        return Ok(ToolLaunch::Process(command.spawn()?));
    }

    let tools_directory = kakera_data_dir()?.join("steam-tools");
    let active_path = tools_directory.join(format!("active-{app_id}"));

    if !active_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "Steam wrapper is not active; launch the game from Steam first",
        ));
    }

    let request_path = tools_directory.join(format!("tool-request-{app_id}"));
    fs::write(request_path, format!("{}\n", path.display()))?;

    Ok(ToolLaunch::Queued)
}

/// installs the steam tool scripts and returns the steam launch option
#[cfg(target_os = "linux")]
pub fn install_steam_tool_wrapper() -> Result<String, io::Error> {
    let tools_directory = kakera_data_dir()?.join("steam-tools");
    fs::create_dir_all(&tools_directory)?;

    let wrapper_path = tools_directory.join("kakera-steam-wrapper");
    let helper_path = tools_directory.join("kakera-steam-helper");

    write_executable_script(wrapper_path.clone(), STEAM_WRAPPER_SCRIPT.to_string())?;
    write_executable_script(helper_path, STEAM_HELPER_SCRIPT.to_string())?;

    Ok(format!("{} %command%", wrapper_path.display()))
}

#[cfg(target_os = "linux")]
fn write_executable_script(path: PathBuf, contents: String) -> Result<(), io::Error> {
    fs::write(&path, contents)?;

    let permissions = fs::Permissions::from_mode(0o755);
    fs::set_permissions(path, permissions)?;

    Ok(())
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
