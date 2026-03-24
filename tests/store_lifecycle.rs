use bluetooth_mapper::model::DeviceRecord;
use bluetooth_mapper::store::Store;
use chrono::{TimeZone, Utc};

#[test]
fn file_backed_store_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let store = Store::open(&db_path).unwrap();
    assert!(db_path.exists());

    let t = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
    let mut d1 = DeviceRecord::new("AA:BB:CC:DD:EE:01", "hci0", t);
    d1.name = Some("Speaker".into());
    d1.rssi = Some(-55);
    store.upsert_device(&d1).unwrap();

    let mut d2 = DeviceRecord::new("AA:BB:CC:DD:EE:02", "hci0", t);
    d2.name = Some("Headphones".into());
    d2.rssi = Some(-70);
    store.upsert_device(&d2).unwrap();

    let loaded = store.load_devices().unwrap();
    assert_eq!(loaded.len(), 2);

    store.insert_sighting(&d1, t).unwrap();
    store.insert_sighting(&d2, t).unwrap();
    store.prune_old_sightings(30).unwrap();

    // Re-open the same database to verify persistence
    drop(store);
    let store2 = Store::open(&db_path).unwrap();
    let reloaded = store2.load_devices().unwrap();
    assert_eq!(reloaded.len(), 2);
    assert!(reloaded.iter().any(|d| d.name.as_deref() == Some("Speaker")));
    assert!(reloaded
        .iter()
        .any(|d| d.name.as_deref() == Some("Headphones")));
}

#[test]
fn store_reopen_preserves_data() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let t = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
    let store = Store::open(&db_path).unwrap();
    let device = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", t);
    store.upsert_device(&device).unwrap();
    drop(store);

    let store2 = Store::open(&db_path).unwrap();
    let devices = store2.load_devices().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].address, "AA:BB:CC:DD:EE:FF");
}
