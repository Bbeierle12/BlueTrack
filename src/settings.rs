use std::{
    fs,
    path::{Path, PathBuf},
};

use dirs::{config_dir, data_local_dir};
use serde::{Deserialize, Serialize};

use bluer::DiscoveryTransport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub selected_adapter: Option<String>,
    pub auto_start_scan: bool,
    pub scan_transport: DiscoveryTransport,
    pub allow_duplicate_data: bool,
    pub stale_after_seconds: u64,
    pub min_rssi: i16,
    pub refresh_interval_seconds: u64,
    pub retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected_adapter: None,
            auto_start_scan: true,
            scan_transport: DiscoveryTransport::Auto,
            allow_duplicate_data: false,
            stale_after_seconds: 30,
            min_rssi: -95,
            refresh_interval_seconds: 5,
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ValidationErrors {
    pub errors: Vec<String>,
}

impl ValidationErrors {
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Settings {
    pub fn config_dir() -> PathBuf {
        config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bluetooth-mapper")
    }

    pub fn data_dir() -> PathBuf {
        data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bluetooth-mapper")
    }

    pub fn path() -> PathBuf {
        Self::config_dir().join("settings.json")
    }

    pub fn database_path(&self) -> PathBuf {
        Self::data_dir().join("bluetooth_mapper.sqlite3")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.ensure_parent(Self::path().as_path())?;
        self.ensure_parent(self.database_path().as_path())?;

        let contents = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        fs::write(Self::path(), contents).map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> ValidationErrors {
        let mut errors = Vec::new();
        if self.stale_after_seconds == 0 {
            errors.push("Stale timeout must be at least 1 second.".to_string());
        }
        if self.refresh_interval_seconds == 0 {
            errors.push("Refresh interval must be at least 1 second.".to_string());
        }
        if self.min_rssi < -120 || self.min_rssi > 0 {
            errors.push("RSSI threshold must be between -120 and 0 dBm.".to_string());
        }
        if self.retention_days == 0 {
            errors.push("Retention must be at least 1 day.".to_string());
        }
        ValidationErrors { errors }
    }

    fn ensure_parent(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
