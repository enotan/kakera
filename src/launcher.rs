use std::io;
use std::path::PathBuf;
use std::process::{Child, Command};

use crate::models::LaunchMode;

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
) -> Result<Child, io::Error> {
    let path = PathBuf::from(executable_path);

    //native runs the app as normal, and wine through wine ofc
    let child: Child = match launch_mode {
        LaunchMode::Native => host_command(path.to_string_lossy().to_string(), Vec::new()).spawn()?,
        LaunchMode::Wine => {
            let wine_command = wine_binary.unwrap_or_else(|| "wine".to_string());
            let mut environment = Vec::new();

            if let Some(prefix) = wine_prefix {
                environment.push(("WINEPREFIX".to_string(), prefix));
            }

            if let Some(locale) = wine_locale {
                environment.push(("LANG".to_string(), locale.clone()));
                environment.push(("LC_ALL".to_string(), locale));
            }

            let mut command = host_command(wine_command, environment);

            command.arg(path);

            command.args(launch_arguments.split_whitespace());

            command.spawn()?
        }
        LaunchMode::Proton => {
            //use umu launcher
            let mut environment = Vec::new();

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

            let mut command = host_command("umu-run".to_string(), environment);

            command.arg(path);
            command.args(launch_arguments.split_whitespace());

            command.spawn()?
        }
    };

    Ok(child)
}

///for running host commands with flatpak
fn host_command(program: String, environment: Vec<(String, String)>) -> Command {
    if std::env::var_os("FLATPAK_ID").is_some() {
        let mut command = Command::new("flatpak-spawn");
        command.arg("--host");
        
        for (name, value) in environment {
            command.arg(format!("--env={name}={value}"));
        }

        command.arg(program);
        command
    } else {
        let mut command = Command::new(program);

        for (name, value) in environment {
            command.env(name, value);
        }

        command
    }
}
