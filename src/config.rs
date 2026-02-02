use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::Parser;
use log::{info, warn};
use serde::{Deserialize, Serialize};

/// Configuration for WFinfo-ng
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the `EE.log` file located in the game installation directory
    ///
    /// Most likely located at `~/.local/share/Steam/steamapps/compatdata/230410/pfx/drive_c/users/steamuser/AppData/Local/Warframe/EE.log`
    pub game_log_file_path: Option<PathBuf>,

    /// Warframe Window Name
    ///
    /// Some systems may require the window name to be specified (e.g. when using gamescope)
    pub window_name: String,

    /// Hotkey for manual detection
    pub hotkey: String,

    /// Delay in milliseconds after detection before capturing the screen
    pub capture_delay_ms: u64,

    /// Path to the prices.json file
    pub prices_file_path: Option<PathBuf>,

    /// Path to the filtered_items.json file
    pub items_file_path: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace, off)
    pub log_level: String,

    /// Whether to show log timestamps
    pub log_timestamps: bool,
}

impl Default for Config {
    fn default() -> Self {
        let default_log_path = dirs::home_dir()
            .map(|home| {
                home.join(".local/share/Steam/steamapps/compatdata/230410/pfx/drive_c/users/steamuser/AppData/Local/Warframe/EE.log")
            });

        Self {
            game_log_file_path: default_log_path,
            window_name: "Warframe".to_string(),
            hotkey: "F12".to_string(),
            capture_delay_ms: 1500,
            prices_file_path: None,
            items_file_path: None,
            log_level: "info".to_string(),
            log_timestamps: false,
        }
    }
}

impl Config {
    /// Load configuration from the default config file path
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_file_path()?;
        Self::load_from_file(&config_path)
    }

    /// Load configuration from the specified file path
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            info!("Config file not found at {}, creating default config", path.display());
            let config = Self::default();
            config.save_to_file(path)?;
            return Ok(config);
        }

        let mut file = File::open(path)
            .with_context(|| format!("Failed to open config file: {}", path.display()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Save configuration to the default config file path
    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_file_path()?;
        self.save_to_file(&config_path)
    }

    /// Save configuration to the specified file path
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let contents = serde_yaml::to_string(self)
            .context("Failed to serialize config to YAML")?;

        let mut file = File::create(path)
            .with_context(|| format!("Failed to create config file: {}", path.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    /// Get the default config file path
    pub fn get_config_file_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
            .join("wfinfo-ng");
        
        Ok(config_dir.join("config.yaml"))
    }

    /// Update configuration with command-line arguments
    pub fn update_from_args(&mut self, args: &Args) {
        if let Some(ref path) = args.game_log_file_path {
            self.game_log_file_path = Some(path.clone());
        }

        if let Some(ref name) = args.window_name {
            self.window_name = name.clone();
        }

        if let Some(ref hotkey) = args.hotkey {
            self.hotkey = hotkey.clone();
        }

        if let Some(delay) = args.capture_delay_ms {
            self.capture_delay_ms = delay;
        }

        if let Some(ref level) = args.log_level {
            self.log_level = level.clone();
        }

        if let Some(timestamps) = args.log_timestamps {
            self.log_timestamps = timestamps;
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate hotkey
        if let Err(err) = global_hotkey::hotkey::HotKey::from_str(&self.hotkey) {
            return Err(anyhow::anyhow!("Invalid hotkey '{}': {}", self.hotkey, err));
        }

        // Validate log level
        match self.log_level.as_str() {
            "error" | "warn" | "info" | "debug" | "trace" | "off" => {}
            _ => return Err(anyhow::anyhow!("Invalid log level: {}", self.log_level)),
        }

        // Validate game log file path if specified
        if let Some(ref path) = self.game_log_file_path
            && !path.exists() {
            warn!("Game log file not found at {}", path.display());
        }

        Ok(())
    }
}

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