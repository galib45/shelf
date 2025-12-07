#![allow(dead_code)]

use std::{fs, path::PathBuf};
use anyhow::{Context, Ok};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub scan_dirs: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self { scan_dirs: Vec::new() }
    }
}

impl Config {
    fn config_path() -> anyhow::Result<PathBuf> {
        let config_path = dirs::home_dir()
            .context("No home directory found! Set HOME environment variable")?
            .join(".shelf").join("config.toml");
        Ok(config_path)
    }

    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path()?;
        
        let app_data_dir = config_path.parent().context("Error getting config path")?;
        if !app_data_dir.exists() { fs::create_dir_all(&app_data_dir)?; }
        
        if !config_path.exists() {
            let default = Self::default();
            default.save()?;
            return Ok(default);
        }

        let contents = fs::read_to_string(&config_path)?;
        let mut config = toml::from_str::<Config>(&contents)?;
        config.scan_dirs = config.scan_dirs.iter()
            .map(|p| {
                let s = p.to_str().unwrap();
                let path = shellexpand::full(s).unwrap();
                PathBuf::from(path.into_owned())
            })
            .collect();

        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()>{
        let config_path = Self::config_path()?;
        
        let app_data_dir = config_path.parent().context("Error getting config path")?;
        if !app_data_dir.exists() { fs::create_dir_all(&app_data_dir)?; }

        let contents = toml::to_string_pretty(self)?;
        fs::write(&config_path, contents)?;
        
        Ok(())
    }
}
