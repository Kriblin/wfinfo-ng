use std::path::PathBuf;

use clap::Parser;

/// Command-line arguments for WFinfo-ng
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the `EE.log` file located in the game installation directory
    ///
    /// Most likely located at `~/.local/share/Steam/steamapps/compatdata/230410/pfx/drive_c/users/steamuser/AppData/Local/Warframe/EE.log`
    #[arg(short, long)]
    pub game_log_file_path: Option<PathBuf>,

    /// Warframe Window Name
    ///
    /// Some systems may require the window name to be specified (e.g. when using gamescope)
    #[arg(short, long)]
    pub window_name: Option<String>,

    /// Hotkey for manual detection
    #[arg(long)]
    pub hotkey: Option<String>,

    /// Delay in milliseconds after detection before capturing the screen
    #[arg(long)]
    pub capture_delay_ms: Option<u64>,

    /// Log level (error, warn, info, debug, trace, off)
    #[arg(long)]
    pub log_level: Option<String>,

    /// Whether to show log timestamps
    #[arg(short, long)]
    pub log_timestamps: Option<bool>,

    /// Path to the config file
    #[arg(short, long)]
    pub config_file: Option<PathBuf>,
}
