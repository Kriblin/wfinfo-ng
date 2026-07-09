use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use log::info;
use serde_json::Value;

use crate::config::Config;

pub const PRICES_URL: &str = "https://api.warframestat.us/wfinfo/prices/";
pub const FILTERED_ITEMS_URL: &str = "https://api.warframestat.us/wfinfo/filtered_items/";
pub const PRICES_FILE_NAME: &str = "prices.json";
pub const FILTERED_ITEMS_FILE_NAME: &str = "filtered_items.json";

pub fn ensure_database_files(config: &Config) -> Result<(PathBuf, PathBuf)> {
    ensure_database_files_with_fetcher(config, fetch_url)
}

pub fn refresh_database_files(config: &Config) -> Result<(PathBuf, PathBuf)> {
    refresh_database_files_with_fetcher(config, fetch_url)
}

pub fn fetch_prices_and_items() -> Result<(PathBuf, PathBuf)> {
    refresh_database_files(&Config::default())
}

pub fn resolve_database_paths(config: &Config) -> Result<(PathBuf, PathBuf)> {
    let default_dir = default_database_dir()?;
    Ok(resolve_database_paths_in_dir(config, &default_dir))
}

fn resolve_database_paths_in_dir(config: &Config, default_dir: &Path) -> (PathBuf, PathBuf) {
    let prices_path = config
        .prices_file_path
        .clone()
        .unwrap_or_else(|| default_dir.join(PRICES_FILE_NAME));
    let items_path = config
        .items_file_path
        .clone()
        .unwrap_or_else(|| default_dir.join(FILTERED_ITEMS_FILE_NAME));

    (prices_path, items_path)
}

fn ensure_database_files_with_fetcher(
    config: &Config,
    fetcher: impl Fn(&str) -> Result<String>,
) -> Result<(PathBuf, PathBuf)> {
    let (prices_path, items_path) = resolve_database_paths(config)?;

    if !is_valid_json_file(&prices_path) {
        info!("Downloading price data to {}", prices_path.display());
        download_json_to_file_with_fetcher(PRICES_URL, &prices_path, &fetcher)?;
    } else {
        info!("Using price data from {}", prices_path.display());
    }

    if !is_valid_json_file(&items_path) {
        info!("Downloading filtered item data to {}", items_path.display());
        download_json_to_file_with_fetcher(FILTERED_ITEMS_URL, &items_path, &fetcher)?;
    } else {
        info!("Using filtered item data from {}", items_path.display());
    }

    Ok((prices_path, items_path))
}

fn refresh_database_files_with_fetcher(
    config: &Config,
    fetcher: impl Fn(&str) -> Result<String>,
) -> Result<(PathBuf, PathBuf)> {
    let (prices_path, items_path) = resolve_database_paths(config)?;
    download_json_to_file_with_fetcher(PRICES_URL, &prices_path, &fetcher)?;
    download_json_to_file_with_fetcher(FILTERED_ITEMS_URL, &items_path, &fetcher)?;
    Ok((prices_path, items_path))
}

fn download_json_to_file_with_fetcher(
    url: &str,
    destination: &Path,
    fetcher: impl Fn(&str) -> Result<String>,
) -> Result<()> {
    let body = fetcher(url).with_context(|| format!("Failed to download {url}"))?;
    write_json_to_file(&body, destination)
}

fn fetch_url(url: &str) -> Result<String> {
    let response =
        reqwest::blocking::get(url).with_context(|| format!("Failed to send request to {url}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("Server returned {status} for {url}"));
    }

    response
        .text()
        .with_context(|| format!("Failed to read response body from {url}"))
}

fn write_json_to_file(json: &str, destination: &Path) -> Result<()> {
    let value: Value = serde_json::from_str(json)
        .with_context(|| format!("Invalid JSON downloaded for {}", destination.display()))?;

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory {}", parent.display()))?;
    }

    let temp_path = temp_file_path(destination);
    let result = (|| -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .with_context(|| format!("Failed to create temporary file {}", temp_path.display()))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &value)
            .with_context(|| format!("Failed to write JSON to {}", temp_path.display()))?;
        writer
            .flush()
            .with_context(|| format!("Failed to flush JSON to {}", temp_path.display()))?;
        drop(writer);

        fs::rename(&temp_path, destination).with_context(|| {
            format!(
                "Failed to replace database file {} with {}",
                destination.display(),
                temp_path.display()
            )
        })?;

        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

fn is_valid_json_file(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };

    serde_json::from_reader::<_, Value>(file).is_ok()
}

fn default_database_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .ok_or_else(|| anyhow!("Could not determine cache directory"))
        .map(|path| path.join("wfinfo-ng"))
}

fn temp_file_path(destination: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("database.json");

    destination.with_file_name(format!(".{file_name}.tmp.{}.{}", std::process::id(), nanos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "wfinfo-utils-test-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[test]
    fn configured_database_paths_are_respected() {
        let config = Config {
            prices_file_path: Some(PathBuf::from("/custom/prices.json")),
            items_file_path: Some(PathBuf::from("/custom/items.json")),
            ..Config::default()
        };

        let paths = resolve_database_paths_in_dir(&config, Path::new("/default"));

        assert_eq!(paths.0, PathBuf::from("/custom/prices.json"));
        assert_eq!(paths.1, PathBuf::from("/custom/items.json"));
    }

    #[test]
    fn default_database_paths_use_app_cache_directory() {
        let config = Config::default();

        let paths = resolve_database_paths_in_dir(&config, Path::new("/default/cache/wfinfo-ng"));

        assert_eq!(
            paths.0,
            PathBuf::from("/default/cache/wfinfo-ng").join(PRICES_FILE_NAME)
        );
        assert_eq!(
            paths.1,
            PathBuf::from("/default/cache/wfinfo-ng").join(FILTERED_ITEMS_FILE_NAME)
        );
    }

    #[test]
    fn valid_json_is_written_pretty() {
        let dir = unique_temp_dir();
        let destination = dir.join("prices.json");

        write_json_to_file(r#"{"name":"Forma","value":3}"#, &destination).unwrap();

        let written = fs::read_to_string(destination).unwrap();
        assert!(written.contains("\"name\": \"Forma\""));
    }

    #[test]
    fn invalid_json_does_not_overwrite_existing_file() {
        let dir = unique_temp_dir();
        let destination = dir.join("prices.json");
        fs::write(&destination, r#"{"existing":true}"#).unwrap();

        let error = write_json_to_file("not json", &destination).unwrap_err();

        assert!(error.to_string().contains("Invalid JSON"));
        assert_eq!(
            fs::read_to_string(destination).unwrap(),
            r#"{"existing":true}"#
        );
    }

    #[test]
    fn ensure_skips_valid_existing_files() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };
        fs::write(config.prices_file_path.as_ref().unwrap(), "[]").unwrap();
        fs::write(config.items_file_path.as_ref().unwrap(), "{}").unwrap();

        let paths = ensure_database_files_with_fetcher(&config, |_| {
            panic!("valid existing files should not be downloaded")
        })
        .unwrap();

        assert_eq!(paths.0, config.prices_file_path.unwrap());
        assert_eq!(paths.1, config.items_file_path.unwrap());
    }

    #[test]
    fn invalid_existing_file_is_replaced_by_download() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };
        fs::write(config.prices_file_path.as_ref().unwrap(), "invalid").unwrap();
        fs::write(config.items_file_path.as_ref().unwrap(), "{}").unwrap();

        ensure_database_files_with_fetcher(&config, |url| {
            if url == PRICES_URL {
                Ok("[]".to_string())
            } else {
                panic!("valid existing items file should not be downloaded")
            }
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(config.prices_file_path.as_ref().unwrap()).unwrap(),
            "[]"
        );
    }
}
