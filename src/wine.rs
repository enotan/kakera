use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

///a wine runner detected on the system
#[derive(Debug, Clone, PartialEq)]
pub struct WineRunner {
    pub name: String,
    pub binary_path: String,
}

///a proton runner detected on the system
#[derive(Debug, Clone, PartialEq)]
pub struct ProtonRunner {
    pub name: String,
    pub path: String,
}

///a steam-created wine prefix
#[derive(Debug, Clone, PartialEq)]
pub struct SteamPrefix {
    pub app_id: String,
    pub game_name: Option<String>,
    pub path: String,
}

///finds wine runners
pub fn detect_wine_runners() -> Vec<WineRunner> {
    let mut runners = Vec::new();

    add_path_wine_runners(&mut runners);

    let home_dir = match host_home_dir() {
        Some(home) => PathBuf::from(home),
        None => return runners,
    };

    let runner_directories = [
        home_dir.join(".local/share/lutris/runners/wine"),
        home_dir.join(".var/app/net.lutris.Lutris/data/lutris/runners/wine"),
        home_dir.join(".local/share/bottles/runners"),
        home_dir.join(".var/app/com.usebottles.bottles/data/bottles/runners"),
    ];

    for directory in runner_directories {
        add_runners_from_directory(&mut runners, &directory);
    }

    runners
}

///find proton runners
pub fn detect_proton_runners() -> Vec<ProtonRunner> {
    let mut runners = Vec::new();

    let home_dir = match host_home_dir() {
        Some(home) => PathBuf::from(home),
        None => return runners,
    };

    let proton_directories = [
        home_dir.join(".local/share/Steam/compatibilitytools.d"),
        home_dir.join(".steam/root/compatibilitytools.d"),
        home_dir.join(".var/app/com.valvesoftware.Steam/data/Steam/compatibilitytools.d"),
        home_dir.join(".steam/debian-installation/compatibilitytools.d"),
        home_dir.join(".local/share/Steam/steamapps/common"),
        home_dir.join(".steam/root/steamapps/common"),
        home_dir.join(".steam/debian-installation/steamapps/common"),
        home_dir.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/common"),
    ];

    for directory in proton_directories {
        add_proton_runners_from_directory(&mut runners, &directory);
    }

    runners
}

///find wine prefixes made by steam
pub fn detect_steam_prefixes() -> Vec<SteamPrefix> {
    let mut prefixes = Vec::new();

    let home_dir = match host_home_dir() {
        Some(home) => home,
        None => return prefixes,
    };

    let compatdata_directories = [
        home_dir.join(".local/share/Steam/steamapps/compatdata"),
        home_dir.join(".steam/root/steamapps/compatdata"),
        home_dir.join(".steam/debian-installation/steamapps/compatdata"),
        home_dir.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/compatdata"),
    ];

    for directory in compatdata_directories {
        add_steam_prefixes_from_directory(&mut prefixes, &directory);
    }

    prefixes.sort_by(|left, right| left.app_id.cmp(&right.app_id));
    prefixes
}

fn add_runners_from_directory(runners: &mut Vec<WineRunner>, directory: &Path) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let runner_directory = entry.path();
        let wine_binary = runner_directory.join("bin/wine");

        if !wine_binary.is_file() {
            continue;
        }

        let runner_name = match runner_directory.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        add_runner_if_missing(
            runners,
            WineRunner {
                name: runner_name,
                binary_path: wine_binary.to_string_lossy().to_string(),
            },
        );
    }
}

fn add_proton_runners_from_directory(runners: &mut Vec<ProtonRunner>, directory: &Path) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let proton_directory = entry.path();

        if !proton_directory.join("proton").is_file() {
            continue;
        }

        let runner_name = match proton_directory.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        let proton_path = proton_directory.to_string_lossy().to_string();

        let already_exists = runners.iter().any(|existing| existing.path == proton_path);

        if !already_exists {
            runners.push(ProtonRunner {
                name: runner_name,
                path: proton_path,
            });
        }
    }
}

fn add_path_wine_runners(runners: &mut Vec<WineRunner>) {
    if env::var_os("FLATPAK_ID").is_some() {
        let output = match Command::new("flatpak-spawn")
            .args([
                "--host",
                "sh",
                "-c",
                "for binary in wine wine64; do command -v \"$binary\" 2>/dev/null || true; done",
            ])
            .output()
            {
                Ok(output) => output,
                Err(_) => return,
            };

            let output_text = match String::from_utf8(output.stdout) {
                Ok(text) => text,
                Err(_) => return,
            };

            for binary_path in output_text.lines() {
                let path = PathBuf::from(binary_path);

                let name = match path.file_name() {
                    Some(name) => format!("System {}", name.to_string_lossy()),
                    None => continue,
                };

                add_runner_if_missing(runners,
                    WineRunner {
                        name,
                        binary_path: binary_path.to_string(),
                    },
                );
            }

            return;
    }

    let path_value = match env::var_os("PATH") {
        Some(path) => path,
        None => return,
    };

    for directory in env::split_paths(&path_value) {
        for binary_name in ["wine", "wine64"] {
            let binary_path = directory.join(binary_name);

            if !binary_path.is_file() {
                continue;
            }

            add_runner_if_missing(runners,
                WineRunner {
                    name: format!("System {binary_name}"), 
                    binary_path: binary_path.to_string_lossy().to_string(),
                }
            );
        }
    }
}

fn add_runner_if_missing(runners: &mut Vec<WineRunner>, runner: WineRunner) {
    let already_exists = runners
        .iter()
        .any(|existing| existing.binary_path == runner.binary_path);

    if !already_exists {
        runners.push(runner);
    }
}

fn add_steam_prefixes_from_directory(prefixes: &mut Vec<SteamPrefix>, directory: &Path) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let compatdata_directory = entry.path();
        let prefix_directory = compatdata_directory.join("pfx");
        
        if !prefix_directory.is_dir() {
            continue;
        }

        let app_id = match compatdata_directory.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        let prefix_path = prefix_directory.to_string_lossy().to_string();

        let already_exists = prefixes
            .iter()
            .any(|existing| existing.app_id == app_id);

        if !already_exists {
            let game_name = steam_game_name(&compatdata_directory, &app_id);

            prefixes.push(SteamPrefix {
                app_id,
                game_name,
                path: prefix_path,
            });
        }
    }
}

///returns the real home dir when running flatpak
fn host_home_dir() -> Option<PathBuf> {
    if env::var_os("FLATPAK_ID").is_none() {
        return env::var_os("HOME").map(PathBuf::from);
    }

    let output = Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c", "printf %s \"$HOME\""])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let home = String::from_utf8(output.stdout).ok()?;

    if home.is_empty() {
        None
    } else {
        Some(PathBuf::from(home))
    }
}

///get the steam game name from the .acf file for use in prefix stuff
fn steam_game_name(compatdata_directory: &Path, app_id: &str) -> Option<String> {
    let steamapps_directory = compatdata_directory.parent()?.parent()?;
    let manifest_path = steamapps_directory.join(format!("appmanifest_{app_id}.acf"));
    let manifest = fs::read_to_string(manifest_path).ok()?;

    for line in manifest.lines() {
        let trimmed = line.trim();

        if !trimmed.starts_with("\"name\"") {
            continue;
        }

        return trimmed
            .split('"')
            .nth(3)
            .map(String::from);
    }

    None
}