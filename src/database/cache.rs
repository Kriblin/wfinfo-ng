use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use log::info;
use reqwest::{
    StatusCode,
    header::{ETAG, IF_NONE_MATCH},
};
use serde_json::Value;

use crate::config::Config;

pub const PRICES_URL: &str = "https://api.warframestat.us/wfinfo/prices/";
pub const FILTERED_ITEMS_URL: &str = "https://api.warframestat.us/wfinfo/filtered_items/";
pub const PRICES_FILE_NAME: &str = "prices.json";
pub const FILTERED_ITEMS_FILE_NAME: &str = "filtered_items.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheRefreshState {
    Updated,
    NotModified,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedFileRefreshResult {
    pub path: PathBuf,
    pub etag: Option<String>,
    pub http_status: Option<u16>,
    pub state: CacheRefreshState,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseRefreshResult {
    pub prices: CachedFileRefreshResult,
    pub items: CachedFileRefreshResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub body: Option<String>,
}

pub fn ensure_database_files(config: &Config) -> Result<(PathBuf, PathBuf)> {
    ensure_database_files_with_fetcher(config, fetch_url)
}

pub fn refresh_database_files(config: &Config) -> Result<(PathBuf, PathBuf)> {
    let result = refresh_database_files_with_status(config)?;
    ensure_refresh_succeeded(&result)?;
    Ok((result.prices.path, result.items.path))
}

pub fn refresh_database_files_with_status(config: &Config) -> Result<DatabaseRefreshResult> {
    refresh_database_files_with_fetcher(config, fetch_url)
}

pub fn database_cache_status(config: &Config) -> Result<DatabaseRefreshResult> {
    let (prices_path, items_path) = resolve_database_paths(config)?;
    Ok(DatabaseRefreshResult {
        prices: cached_file_status(&prices_path, "Price data cache"),
        items: cached_file_status(&items_path, "Filtered item data cache"),
    })
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
    fetcher: impl Fn(&str, Option<&str>) -> Result<FetchResponse>,
) -> Result<(PathBuf, PathBuf)> {
    let (prices_path, items_path) = resolve_database_paths(config)?;

    if !is_valid_json_file(&prices_path) {
        info!("Downloading price data to {}", prices_path.display());
        let result = refresh_cached_url(PRICES_URL, &prices_path, &fetcher)?;
        ensure_file_refresh_succeeded(&result)?;
    } else {
        info!("Using price data from {}", prices_path.display());
    }

    if !is_valid_json_file(&items_path) {
        info!("Downloading filtered item data to {}", items_path.display());
        let result = refresh_cached_url(FILTERED_ITEMS_URL, &items_path, &fetcher)?;
        ensure_file_refresh_succeeded(&result)?;
    } else {
        info!("Using filtered item data from {}", items_path.display());
    }

    Ok((prices_path, items_path))
}

pub fn refresh_database_files_with_fetcher(
    config: &Config,
    fetcher: impl Fn(&str, Option<&str>) -> Result<FetchResponse>,
) -> Result<DatabaseRefreshResult> {
    let (prices_path, items_path) = resolve_database_paths(config)?;
    Ok(DatabaseRefreshResult {
        prices: refresh_cached_url(PRICES_URL, &prices_path, &fetcher)?,
        items: refresh_cached_url(FILTERED_ITEMS_URL, &items_path, &fetcher)?,
    })
}

fn refresh_cached_url(
    url: &str,
    destination: &Path,
    fetcher: impl Fn(&str, Option<&str>) -> Result<FetchResponse>,
) -> Result<CachedFileRefreshResult> {
    let existing_etag = read_etag(destination)?;
    let response = fetcher(url, existing_etag.as_deref())
        .with_context(|| format!("Failed to download {url}"))?;

    match StatusCode::from_u16(response.status) {
        Ok(StatusCode::OK) => {
            let body = response
                .body
                .ok_or_else(|| anyhow!("Server returned 200 OK for {url} without a body"))?;
            write_json_to_file(&body, destination)?;

            let etag = if let Some(etag) = response.etag {
                write_etag(destination, &etag)?;
                Some(etag)
            } else {
                existing_etag
            };

            Ok(CachedFileRefreshResult {
                path: destination.to_path_buf(),
                etag,
                http_status: Some(response.status),
                state: CacheRefreshState::Updated,
                message: format!("Updated from HTTP {}", response.status),
            })
        }
        Ok(StatusCode::NOT_MODIFIED) => {
            if !is_valid_json_file(destination) {
                bail!(
                    "Server returned 304 Not Modified for {url}, but no valid local cache exists at {}",
                    destination.display()
                );
            }

            Ok(CachedFileRefreshResult {
                path: destination.to_path_buf(),
                etag: existing_etag,
                http_status: Some(response.status),
                state: CacheRefreshState::NotModified,
                message: "Cache is current (HTTP 304 Not Modified)".to_string(),
            })
        }
        Ok(StatusCode::SERVICE_UNAVAILABLE) => Ok(CachedFileRefreshResult {
            path: destination.to_path_buf(),
            etag: existing_etag,
            http_status: Some(response.status),
            state: CacheRefreshState::Failed,
            message: "Refresh failed: service temporarily unavailable (HTTP 503)".to_string(),
        }),
        _ => Ok(CachedFileRefreshResult {
            path: destination.to_path_buf(),
            etag: existing_etag,
            http_status: Some(response.status),
            state: CacheRefreshState::Failed,
            message: format!("Refresh failed with HTTP {}", response.status),
        }),
    }
}

fn fetch_url(url: &str, etag: Option<&str>) -> Result<FetchResponse> {
    let client = reqwest::blocking::Client::new();
    let mut request = client.get(url);
    if let Some(etag) = etag {
        request = request.header(IF_NONE_MATCH, etag);
    }

    let response = request
        .send()
        .with_context(|| format!("Failed to send request to {url}"))?;
    let status = response.status();
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);

    let body = if status == StatusCode::OK {
        Some(
            response
                .text()
                .with_context(|| format!("Failed to read response body from {url}"))?,
        )
    } else {
        None
    };

    Ok(FetchResponse {
        status: status.as_u16(),
        etag,
        body,
    })
}

fn ensure_refresh_succeeded(result: &DatabaseRefreshResult) -> Result<()> {
    ensure_file_refresh_succeeded(&result.prices)?;
    ensure_file_refresh_succeeded(&result.items)?;
    Ok(())
}

fn ensure_file_refresh_succeeded(result: &CachedFileRefreshResult) -> Result<()> {
    if result.state == CacheRefreshState::Failed {
        bail!("{}", result.message);
    }

    Ok(())
}

fn cached_file_status(path: &Path, label: &str) -> CachedFileRefreshResult {
    let etag = read_etag(path).unwrap_or_default();
    let exists = path.exists();
    CachedFileRefreshResult {
        path: path.to_path_buf(),
        etag,
        http_status: None,
        state: CacheRefreshState::Skipped,
        message: if exists {
            format!("{label} is present")
        } else {
            format!("{label} is not downloaded")
        },
    }
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

fn etag_path(destination: &Path) -> PathBuf {
    let Some(file_name) = destination.file_name() else {
        return destination.with_extension("etag");
    };
    let mut etag_file_name = file_name.to_os_string();
    etag_file_name.push(".etag");
    destination.with_file_name(etag_file_name)
}

fn read_etag(destination: &Path) -> Result<Option<String>> {
    let path = etag_path(destination);
    match fs::read_to_string(&path) {
        Ok(etag) => {
            let etag = etag.trim().to_string();
            if etag.is_empty() {
                Ok(None)
            } else {
                Ok(Some(etag))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Failed to read ETag {}", path.display())),
    }
}

fn write_etag(destination: &Path, etag: &str) -> Result<()> {
    let path = etag_path(destination);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create ETag directory {}", parent.display()))?;
    }
    fs::write(&path, etag).with_context(|| format!("Failed to write ETag {}", path.display()))
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
    use std::sync::{Arc, Mutex};
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

        let paths = ensure_database_files_with_fetcher(&config, |_, _| {
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

        ensure_database_files_with_fetcher(&config, |url, etag| {
            assert!(etag.is_none());
            if url == PRICES_URL {
                Ok(FetchResponse {
                    status: 200,
                    etag: Some("\"prices-v1\"".to_string()),
                    body: Some("[]".to_string()),
                })
            } else {
                panic!("valid existing items file should not be downloaded")
            }
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(config.prices_file_path.as_ref().unwrap()).unwrap(),
            "[]"
        );
        assert_eq!(
            fs::read_to_string(etag_path(config.prices_file_path.as_ref().unwrap())).unwrap(),
            "\"prices-v1\""
        );
    }

    #[test]
    fn first_successful_refresh_writes_body_and_etag() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };

        let result = refresh_database_files_with_fetcher(&config, |url, etag| {
            assert!(etag.is_none());
            let (body, etag) = if url == PRICES_URL {
                (r#"[{"item_name":"Forma"}]"#, "\"prices-v1\"")
            } else {
                (r#"{"items":[]}"#, "\"items-v1\"")
            };

            Ok(FetchResponse {
                status: 200,
                etag: Some(etag.to_string()),
                body: Some(body.to_string()),
            })
        })
        .unwrap();

        assert_eq!(result.prices.state, CacheRefreshState::Updated);
        assert_eq!(result.items.state, CacheRefreshState::Updated);
        assert_eq!(
            fs::read_to_string(config.prices_file_path.as_ref().unwrap()).unwrap(),
            "[\n  {\n    \"item_name\": \"Forma\"\n  }\n]"
        );
        assert_eq!(
            fs::read_to_string(etag_path(config.prices_file_path.as_ref().unwrap())).unwrap(),
            "\"prices-v1\""
        );
        assert_eq!(
            fs::read_to_string(etag_path(config.items_file_path.as_ref().unwrap())).unwrap(),
            "\"items-v1\""
        );
    }

    #[test]
    fn second_refresh_sends_if_none_match_and_keeps_not_modified_files() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };

        refresh_database_files_with_fetcher(&config, |url, etag| {
            assert!(etag.is_none());
            let (body, etag) = if url == PRICES_URL {
                (r#"["first-prices"]"#, "\"prices-v1\"")
            } else {
                (r#"{"first":"items"}"#, "\"items-v1\"")
            };
            Ok(FetchResponse {
                status: 200,
                etag: Some(etag.to_string()),
                body: Some(body.to_string()),
            })
        })
        .unwrap();

        let observed_etags = Arc::new(Mutex::new(Vec::new()));
        let observed_etags_clone = observed_etags.clone();
        let result = refresh_database_files_with_fetcher(&config, move |url, etag| {
            observed_etags_clone
                .lock()
                .unwrap()
                .push((url.to_string(), etag.map(ToOwned::to_owned)));
            Ok(FetchResponse {
                status: 304,
                etag: None,
                body: None,
            })
        })
        .unwrap();

        assert_eq!(result.prices.state, CacheRefreshState::NotModified);
        assert_eq!(result.items.state, CacheRefreshState::NotModified);
        assert_eq!(
            fs::read_to_string(config.prices_file_path.as_ref().unwrap()).unwrap(),
            "[\n  \"first-prices\"\n]"
        );
        assert_eq!(
            fs::read_to_string(config.items_file_path.as_ref().unwrap()).unwrap(),
            "{\n  \"first\": \"items\"\n}"
        );
        assert_eq!(
            *observed_etags.lock().unwrap(),
            vec![
                (PRICES_URL.to_string(), Some("\"prices-v1\"".to_string())),
                (
                    FILTERED_ITEMS_URL.to_string(),
                    Some("\"items-v1\"".to_string())
                ),
            ]
        );
    }

    #[test]
    fn error_status_does_not_overwrite_cached_file_or_etag() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };
        fs::write(config.prices_file_path.as_ref().unwrap(), r#"["cached"]"#).unwrap();
        fs::write(
            config.items_file_path.as_ref().unwrap(),
            r#"{"cached":true}"#,
        )
        .unwrap();
        write_etag(config.prices_file_path.as_ref().unwrap(), "\"prices-v1\"").unwrap();
        write_etag(config.items_file_path.as_ref().unwrap(), "\"items-v1\"").unwrap();

        let result = refresh_database_files_with_fetcher(&config, |_, _| {
            Ok(FetchResponse {
                status: 500,
                etag: Some("\"should-not-write\"".to_string()),
                body: Some(r#"["server"]"#.to_string()),
            })
        })
        .unwrap();

        assert_eq!(result.prices.state, CacheRefreshState::Failed);
        assert_eq!(result.items.state, CacheRefreshState::Failed);
        assert_eq!(
            fs::read_to_string(config.prices_file_path.as_ref().unwrap()).unwrap(),
            r#"["cached"]"#
        );
        assert_eq!(
            fs::read_to_string(etag_path(config.prices_file_path.as_ref().unwrap())).unwrap(),
            "\"prices-v1\""
        );
    }

    #[test]
    fn service_unavailable_does_not_overwrite_cached_file_or_etag() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };
        fs::write(config.prices_file_path.as_ref().unwrap(), r#"["cached"]"#).unwrap();
        fs::write(
            config.items_file_path.as_ref().unwrap(),
            r#"{"cached":true}"#,
        )
        .unwrap();
        write_etag(config.prices_file_path.as_ref().unwrap(), "\"prices-v1\"").unwrap();
        write_etag(config.items_file_path.as_ref().unwrap(), "\"items-v1\"").unwrap();

        let result = refresh_database_files_with_fetcher(&config, |_, _| {
            Ok(FetchResponse {
                status: 503,
                etag: Some("\"should-not-write\"".to_string()),
                body: Some(r#"["server"]"#.to_string()),
            })
        })
        .unwrap();

        assert_eq!(result.prices.state, CacheRefreshState::Failed);
        assert_eq!(result.prices.http_status, Some(503));
        assert!(result.prices.message.contains("temporarily unavailable"));
        assert_eq!(
            fs::read_to_string(config.prices_file_path.as_ref().unwrap()).unwrap(),
            r#"["cached"]"#
        );
        assert_eq!(
            fs::read_to_string(etag_path(config.prices_file_path.as_ref().unwrap())).unwrap(),
            "\"prices-v1\""
        );
    }

    #[test]
    fn missing_local_file_with_not_modified_response_is_an_error() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };

        let error = refresh_database_files_with_fetcher(&config, |_, _| {
            Ok(FetchResponse {
                status: 304,
                etag: None,
                body: None,
            })
        })
        .unwrap_err();

        assert!(error.to_string().contains("304 Not Modified"));
        assert!(!config.prices_file_path.as_ref().unwrap().exists());
    }

    #[test]
    fn missing_etag_performs_request_without_if_none_match() {
        let dir = unique_temp_dir();
        let config = Config {
            prices_file_path: Some(dir.join(PRICES_FILE_NAME)),
            items_file_path: Some(dir.join(FILTERED_ITEMS_FILE_NAME)),
            ..Config::default()
        };
        fs::write(config.prices_file_path.as_ref().unwrap(), "[]").unwrap();
        fs::write(config.items_file_path.as_ref().unwrap(), "{}").unwrap();

        let observed_etags = Arc::new(Mutex::new(Vec::new()));
        let observed_etags_clone = observed_etags.clone();
        refresh_database_files_with_fetcher(&config, move |url, etag| {
            observed_etags_clone
                .lock()
                .unwrap()
                .push((url.to_string(), etag.map(ToOwned::to_owned)));
            Ok(FetchResponse {
                status: 200,
                etag: None,
                body: Some(if url == PRICES_URL {
                    "[]".to_string()
                } else {
                    "{}".to_string()
                }),
            })
        })
        .unwrap();

        assert_eq!(
            *observed_etags.lock().unwrap(),
            vec![
                (PRICES_URL.to_string(), None),
                (FILTERED_ITEMS_URL.to_string(), None),
            ]
        );
    }
}
