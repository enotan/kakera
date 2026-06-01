use std::io;
use std::path::PathBuf;
use std::process::{Child, Command};

use crate::models::LaunchMode;

///launch the executable
pub fn launch_executable(
    executable_path: String,
    launch_mode: LaunchMode,
) -> Result<Child, io::Error> {
    let path = PathBuf::from(executable_path);

    //native runs the app as normal, and wine through wine ofc
    let child: Child = match launch_mode {
        LaunchMode::Native => Command::new(path).spawn()?,
        LaunchMode::Wine => Command::new("wine").arg(path).spawn()?,
    };
    
    Ok(child)
}