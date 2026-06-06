use std::env;
use std::fs;
use std::path::{Path, PathBuf};

///a wine runner detected on the system
#[derive(Debug, Clone, PartialEq)]
pub struct WineRunner {
    pub name: String,
    pub binary_path: String,
}

///finds wine runners
pub fn detect_wine_runners() -> Vec<WineRunner> {
    let mut runners = Vec::new();

    add_path_wine_runners(&mut runners);

    let home_dir = match env::var_os("HOME") {
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

fn add_path_wine_runners(runners: &mut Vec<WineRunner>) {
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

            add_runner_if_missing(
                runners,
                WineRunner {
                    name: format!("System {binary_name}"),
                    binary_path: binary_path.to_string_lossy().to_string(),
                },
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