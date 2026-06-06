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
        LaunchMode::Native => Command::new(path).spawn()?,
        LaunchMode::Wine => {
            let wine_command = wine_binary.unwrap_or_else(|| "wine".to_string());
            let mut command = Command::new(wine_command);

            if let Some(prefix) = wine_prefix {
                command.env("WINEPREFIX", prefix);
            }

            if let Some(locale) = wine_locale {
                command.env("LANG", &locale);
                command.env("LC_ALL", locale);
            }

            command.arg(path);

            command.args(launch_arguments.split_whitespace());

            command.spawn()?
        }
        LaunchMode::Proton => {
            //use umu launcher
            let mut command = Command::new("umu-run");

            if let Some(prefix) = wine_prefix {
                command.env("WINEPREFIX", prefix);
            }

            if let Some(proton_path) = proton_path {
                command.env("PROTONPATH", proton_path);
            }

            if let Some(locale) = wine_locale {
                command.env("LANG", &locale);
                command.env("LC_ALL", locale);
            }

            command.env("GAMEID", umu_game_id);
            command.arg(path);
            command.args(launch_arguments.split_whitespace());

            command.spawn()?
        }
    };

    Ok(child)
}
