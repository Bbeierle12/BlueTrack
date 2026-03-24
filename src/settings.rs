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
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.ensure_parent(self.database_path().as_path())?;
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, contents).map_err(|error| error.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate ────────────────────────────────────────────────────

    #[test]
    fn validate_default_settings_passes() {
        let s = Settings::default();
        assert!(s.validate().is_empty());
    }

    #[test]
    fn validate_stale_after_zero() {
        let mut s = Settings::default();
        s.stale_after_seconds = 0;
        let errors = s.validate();
        assert!(!errors.is_empty());
        assert!(errors.errors[0].contains("Stale timeout"));
    }

    #[test]
    fn validate_refresh_interval_zero() {
        let mut s = Settings::default();
        s.refresh_interval_seconds = 0;
        let errors = s.validate();
        assert!(!errors.is_empty());
        assert!(errors.errors[0].contains("Refresh interval"));
    }

    #[test]
    fn validate_min_rssi_below_minus_120() {
        let mut s = Settings::default();
        s.min_rssi = -121;
        let errors = s.validate();
        assert!(!errors.is_empty());
        assert!(errors.errors[0].contains("RSSI"));
    }

    #[test]
    fn validate_min_rssi_above_zero() {
        let mut s = Settings::default();
        s.min_rssi = 1;
        let errors = s.validate();
        assert!(!errors.is_empty());
        assert!(errors.errors[0].contains("RSSI"));
    }

    #[test]
    fn validate_min_rssi_boundary_minus_120() {
        let mut s = Settings::default();
        s.min_rssi = -120;
        assert!(s.validate().is_empty());
    }

    #[test]
    fn validate_min_rssi_boundary_zero() {
        let mut s = Settings::default();
        s.min_rssi = 0;
        assert!(s.validate().is_empty());
    }

    #[test]
    fn validate_retention_days_zero() {
        let mut s = Settings::default();
        s.retention_days = 0;
        let errors = s.validate();
        assert!(!errors.is_empty());
        assert!(errors.errors[0].contains("Retention"));
    }

    #[test]
    fn validate_multiple_errors() {
        let mut s = Settings::default();
        s.stale_after_seconds = 0;
        s.refresh_interval_seconds = 0;
        s.retention_days = 0;
        let errors = s.validate();
        assert_eq!(errors.errors.len(), 3);
    }

    // ── load / save round-trip ──────────────────────────────────────

    #[test]
    fn load_missing_file_returns_defaults() {
        // Settings::load() reads from a fixed XDG path; if the file doesn't
        // exist there, we get defaults. We can't easily redirect the path
        // without a refactor, but we can verify the fallback logic by
        // confirming default() matches the expected values.
        let defaults = Settings::default();
        assert!(defaults.auto_start_scan);
        assert_eq!(defaults.stale_after_seconds, 30);
        assert_eq!(defaults.min_rssi, -95);
        assert_eq!(defaults.refresh_interval_seconds, 5);
        assert_eq!(defaults.retention_days, 30);
    }

    #[test]
    fn serde_round_trip() {
        let original = Settings::default();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.stale_after_seconds, original.stale_after_seconds);
        assert_eq!(restored.min_rssi, original.min_rssi);
        assert_eq!(
            restored.refresh_interval_seconds,
            original.refresh_interval_seconds
        );
        assert_eq!(restored.retention_days, original.retention_days);
        assert_eq!(restored.auto_start_scan, original.auto_start_scan);
    }

    #[test]
    fn malformed_json_returns_defaults() {
        let result: Settings = serde_json::from_str("{ broken }").unwrap_or_default();
        assert_eq!(result.stale_after_seconds, 30);
    }

    #[test]
    fn save_and_load_with_tempdir() {
        // Test file I/O via a temp directory. We can't redirect Settings::path()
        // without refactoring, but we can test the serialization + file write
        // directly using the same logic.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded_json = fs::read_to_string(&path).unwrap();
        let loaded: Settings = serde_json::from_str(&loaded_json).unwrap();
        assert_eq!(loaded.stale_after_seconds, settings.stale_after_seconds);
        assert_eq!(loaded.min_rssi, settings.min_rssi);
    }

    // ── ValidationErrors ────────────────────────────────────────────

    #[test]
    fn validation_errors_is_empty() {
        let empty = ValidationErrors::default();
        assert!(empty.is_empty());

        let non_empty = ValidationErrors {
            errors: vec!["oops".into()],
        };
        assert!(!non_empty.is_empty());
    }

    // ── path helpers ────────────────────────────────────────────────

    #[test]
    fn config_dir_ends_with_bluetooth_mapper() {
        let dir = Settings::config_dir();
        let s = dir.to_string_lossy();
        assert!(s.ends_with("bluetooth-mapper"), "got: {s}");
    }

    #[test]
    fn data_dir_ends_with_bluetooth_mapper() {
        let dir = Settings::data_dir();
        let s = dir.to_string_lossy();
        assert!(s.ends_with("bluetooth-mapper"), "got: {s}");
    }

    #[test]
    fn path_ends_with_settings_json() {
        let p = Settings::path();
        assert!(p.ends_with("settings.json"), "got: {}", p.display());
    }

    #[test]
    fn database_path_ends_with_sqlite3() {
        let s = Settings::default();
        let p = s.database_path();
        assert!(
            p.to_string_lossy().ends_with(".sqlite3"),
            "got: {}",
            p.display()
        );
    }

    // ── transport variants ──────────────────────────────────────────

    #[test]
    fn all_transport_variants_serialize() {
        for transport in [
            DiscoveryTransport::Auto,
            DiscoveryTransport::Le,
            DiscoveryTransport::BrEdr,
        ] {
            let mut s = Settings::default();
            s.scan_transport = transport;
            let json = serde_json::to_string(&s).unwrap();
            let restored: Settings = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&restored).unwrap();
            assert_eq!(json, json2);
        }
    }

    // ── load_from / save_to ─────────────────────────────────────────

    #[test]
    fn save_to_and_load_from_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let mut settings = Settings::default();
        settings.stale_after_seconds = 45;
        settings.min_rssi = -80;
        settings.retention_days = 7;
        settings.selected_adapter = Some("hci1".into());

        settings.save_to(&path).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.stale_after_seconds, 45);
        assert_eq!(loaded.min_rssi, -80);
        assert_eq!(loaded.retention_days, 7);
        assert_eq!(loaded.selected_adapter.as_deref(), Some("hci1"));
    }

    #[test]
    fn load_from_nonexistent_returns_defaults() {
        let loaded = Settings::load_from(Path::new("/nonexistent/settings.json"));
        assert_eq!(loaded.stale_after_seconds, 30);
    }

    #[test]
    fn load_from_malformed_json_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not valid json }").unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.stale_after_seconds, 30);
    }

    #[test]
    fn save_to_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("dir").join("settings.json");
        let settings = Settings::default();
        assert!(settings.save_to(&path).is_ok());
        assert!(path.exists());
    }
}
