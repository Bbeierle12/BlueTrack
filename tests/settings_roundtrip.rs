use bluetooth_mapper::settings::Settings;
use std::path::Path;

#[test]
fn save_to_load_from_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let mut settings = Settings::default();
    settings.stale_after_seconds = 60;
    settings.min_rssi = -85;
    settings.refresh_interval_seconds = 10;
    settings.retention_days = 14;
    settings.auto_start_scan = false;
    settings.selected_adapter = Some("hci1".into());

    settings.save_to(&path).unwrap();

    let loaded = Settings::load_from(&path);
    assert_eq!(loaded.stale_after_seconds, 60);
    assert_eq!(loaded.min_rssi, -85);
    assert_eq!(loaded.refresh_interval_seconds, 10);
    assert_eq!(loaded.retention_days, 14);
    assert!(!loaded.auto_start_scan);
    assert_eq!(loaded.selected_adapter.as_deref(), Some("hci1"));
}

#[test]
fn load_from_nonexistent_returns_defaults() {
    let loaded = Settings::load_from(Path::new("/tmp/nonexistent_path/settings.json"));
    assert_eq!(loaded.stale_after_seconds, 30);
    assert!(loaded.auto_start_scan);
}

#[test]
fn save_to_creates_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("settings.json");

    let settings = Settings::default();
    settings.save_to(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn overwrite_preserves_latest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let mut s1 = Settings::default();
    s1.stale_after_seconds = 10;
    s1.save_to(&path).unwrap();

    let mut s2 = Settings::default();
    s2.stale_after_seconds = 99;
    s2.save_to(&path).unwrap();

    let loaded = Settings::load_from(&path);
    assert_eq!(loaded.stale_after_seconds, 99);
}
