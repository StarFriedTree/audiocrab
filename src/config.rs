use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::Browser;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub download_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub db_path: PathBuf,
    pub cookies_from_browser: Option<Browser>,
    pub concurrency: usize,
    pub retries: u32,
}

impl Default for Config {
    fn default() -> Self {
        let root = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("audiocrab");
        
        let download_dir = dirs::audio_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("audiocrab");

        Self {
            temp_dir: download_dir.join("temp"),
            download_dir: download_dir,
            db_path: root.join("audiocrab.db"),
            cookies_from_browser: None,
            concurrency: 4,
            retries: 10,
        }
    }
}

impl Config {
    pub fn data_dir(&self) -> &Path {
        self.download_dir.as_path()
    }

    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("audiocrab").join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        
        if !path.exists() {
            let cfg = Self::default();
            cfg.save().context("failed to save initial default config")?;
            return Ok(cfg);
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let parent = path.parent().context("config path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
        let text = toml::to_string_pretty(self)?;

        let mut temp_file = tempfile::NamedTempFile::new_in(parent)
            .context("failed to create temporary file for saving config")?;

        use std::io::Write;
        temp_file.write_all(text.as_bytes()).context("failed to write to temp config file")?;
        
        temp_file.flush().context("failed to flush data to disk")?;

        temp_file.persist(&path).context("failed to overwrite old config file safely")?;

        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "download_dir" => Some(self.download_dir.to_string_lossy().to_string()),
            "temp_dir" => Some(self.temp_dir.to_string_lossy().to_string()),
            "db_path" => Some(self.db_path.to_string_lossy().to_string()),
            "cookies_from_browser" => self
                .cookies_from_browser
                .as_ref()
                .map(|b| format!("{b:?}").to_ascii_lowercase()),
            "concurrency" => Some(self.concurrency.to_string()),
            "retries" => Some(self.retries.to_string()),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "download_dir" => self.download_dir = PathBuf::from(value),
            "temp_dir" => self.temp_dir = PathBuf::from(value),
            "db_path" => self.db_path = PathBuf::from(value),
            "cookies_from_browser" => {
                self.cookies_from_browser = match value.trim().to_ascii_lowercase().as_str() {
                    "" | "none" => None,
                    other => Some(parse_browser(other)?),
                };
            }
            "concurrency" => self.concurrency = value.parse().context("invalid concurrency")?,
            "retries" => self.retries = value.parse().context("invalid retries")?,
            _ => anyhow::bail!("unknown config key: {key}"),
        }
        Ok(())
    }
}

fn parse_browser(value: &str) -> Result<Browser> {
    match value.trim().to_ascii_lowercase().as_str() {
        "brave" => Ok(Browser::Brave),
        "chrome" => Ok(Browser::Chrome),
        "chromium" => Ok(Browser::Chromium),
        "edge" => Ok(Browser::Edge),
        "firefox" => Ok(Browser::Firefox),
        "opera" => Ok(Browser::Opera),
        "safari" => Ok(Browser::Safari),
        "vivaldi" => Ok(Browser::Vivaldi),
        "whale" => Ok(Browser::Whale),
        other => anyhow::bail!("unsupported browser: {other}"),
    }
}
