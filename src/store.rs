use std::{fs, path::Path};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::model::DeviceRecord;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let conn = Connection::open(path).map_err(|error| error.to_string())?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|error| error.to_string())?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn migrate(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS device_state (
                    address TEXT PRIMARY KEY,
                    json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS sightings (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    address TEXT NOT NULL,
                    adapter_name TEXT NOT NULL,
                    name TEXT,
                    seen_at TEXT NOT NULL,
                    rssi INTEGER,
                    manufacturer_summary TEXT,
                    proximity_band TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_sightings_address_seen_at
                    ON sightings(address, seen_at DESC);
                CREATE INDEX IF NOT EXISTS idx_sightings_seen_at
                    ON sightings(seen_at);
                ",
            )
            .map_err(|error| error.to_string())
    }

    pub fn load_devices(&self) -> Result<Vec<DeviceRecord>, String> {
        let mut statement = self
            .conn
            .prepare("SELECT json FROM device_state ORDER BY updated_at DESC")
            .map_err(|error| error.to_string())?;

        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;

        let mut devices = Vec::new();
        for row in rows {
            let json = row.map_err(|error| error.to_string())?;
            let device = serde_json::from_str(&json).map_err(|error| error.to_string())?;
            devices.push(device);
        }
        Ok(devices)
    }

    pub fn upsert_device(&self, device: &DeviceRecord) -> Result<(), String> {
        let json = serde_json::to_string(device).map_err(|error| error.to_string())?;
        self.conn
            .execute(
                "
                INSERT INTO device_state (address, json, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(address) DO UPDATE SET
                    json = excluded.json,
                    updated_at = excluded.updated_at
                ",
                params![device.address, json, format_sqlite_datetime(device.last_seen)],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn insert_sighting(
        &self,
        device: &DeviceRecord,
        seen_at: DateTime<Utc>,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "
                INSERT INTO sightings (
                    address,
                    adapter_name,
                    name,
                    seen_at,
                    rssi,
                    manufacturer_summary,
                    proximity_band
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    device.address,
                    device.adapter_name,
                    device.name.clone().or_else(|| device.alias.clone()),
                    format_sqlite_datetime(seen_at),
                    device.rssi,
                    device.manufacturer_summary(),
                    device.proximity_band(),
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn prune_old_sightings(&self, retention_days: u32) -> Result<(), String> {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));
        self.conn
            .execute(
                "DELETE FROM sightings WHERE seen_at < ?1",
                params![format_sqlite_datetime(cutoff)],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// Formats a DateTime<Utc> in SQLite-compatible format (space-separated, no timezone suffix).
/// This ensures consistent lexicographic comparison with SQLite's datetime() output.
fn format_sqlite_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()
    }

    fn make_device(address: &str) -> DeviceRecord {
        DeviceRecord::new(address, "hci0", fixed_time())
    }

    fn open_store() -> Store {
        Store::open_in_memory().expect("in-memory store")
    }

    // ── migrate idempotency ─────────────────────────────────────────

    #[test]
    fn migrate_is_idempotent() {
        let store = open_store();
        // migrate() was already called in open_in_memory; call again
        assert!(store.migrate().is_ok());
        assert!(store.migrate().is_ok());
    }

    // ── schema creation ─────────────────────────────────────────────

    #[test]
    fn schema_has_device_state_table() {
        let store = open_store();
        let count: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='device_state'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn schema_has_sightings_table() {
        let store = open_store();
        let count: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='sightings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── upsert_device ───────────────────────────────────────────────

    #[test]
    fn upsert_device_insert() {
        let store = open_store();
        let device = make_device("AA:BB:CC:DD:EE:FF");
        assert!(store.upsert_device(&device).is_ok());

        let devices = store.load_devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address, "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn upsert_device_update() {
        let store = open_store();
        let mut device = make_device("AA:BB:CC:DD:EE:FF");
        store.upsert_device(&device).unwrap();

        device.name = Some("Updated".into());
        device.last_seen = Utc.with_ymd_and_hms(2025, 6, 16, 0, 0, 0).unwrap();
        store.upsert_device(&device).unwrap();

        let devices = store.load_devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name.as_deref(), Some("Updated"));
    }

    // ── load_devices ordering ───────────────────────────────────────

    #[test]
    fn load_devices_ordered_by_updated_at_desc() {
        let store = open_store();

        let mut d1 = make_device("AA:00:00:00:00:01");
        d1.last_seen = Utc.with_ymd_and_hms(2025, 6, 14, 0, 0, 0).unwrap();
        store.upsert_device(&d1).unwrap();

        let mut d2 = make_device("AA:00:00:00:00:02");
        d2.last_seen = Utc.with_ymd_and_hms(2025, 6, 16, 0, 0, 0).unwrap();
        store.upsert_device(&d2).unwrap();

        let devices = store.load_devices().unwrap();
        assert_eq!(devices.len(), 2);
        // Most recently updated first
        assert_eq!(devices[0].address, "AA:00:00:00:00:02");
        assert_eq!(devices[1].address, "AA:00:00:00:00:01");
    }

    // ── JSON round-trip integrity ───────────────────────────────────

    #[test]
    fn device_json_round_trip() {
        let store = open_store();
        let mut device = make_device("AA:BB:CC:DD:EE:FF");
        device.name = Some("Test Device".into());
        device.alias = Some("My Device".into());
        device.rssi = Some(-65);
        device.paired = true;
        device.manufacturer_data.push(crate::model::ManufacturerEntry {
            id: 0x004c,
            payload_hex: "0215abcd".into(),
            company_name: None,
        });
        device.uuids = vec!["0000180a-0000-1000-8000-00805f9b34fb".into()];

        store.upsert_device(&device).unwrap();
        let loaded = store.load_devices().unwrap();
        assert_eq!(loaded.len(), 1);

        let d = &loaded[0];
        assert_eq!(d.name.as_deref(), Some("Test Device"));
        assert_eq!(d.alias.as_deref(), Some("My Device"));
        assert_eq!(d.rssi, Some(-65));
        assert!(d.paired);
        assert_eq!(d.manufacturer_data.len(), 1);
        assert_eq!(d.manufacturer_data[0].id, 0x004c);
        assert_eq!(d.uuids.len(), 1);
    }

    // ── insert_sighting ─────────────────────────────────────────────

    #[test]
    fn insert_sighting_basic() {
        let store = open_store();
        let device = make_device("AA:BB:CC:DD:EE:FF");
        let now = fixed_time();
        assert!(store.insert_sighting(&device, now).is_ok());

        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM sightings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn insert_sighting_stores_correct_fields() {
        let store = open_store();
        let mut device = make_device("AA:BB:CC:DD:EE:FF");
        device.name = Some("TestDev".into());
        device.rssi = Some(-55);
        let now = fixed_time();
        store.insert_sighting(&device, now).unwrap();

        let (addr, name, rssi, band): (String, Option<String>, Option<i16>, String) = store
            .conn
            .query_row(
                "SELECT address, name, rssi, proximity_band FROM sightings LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(addr, "AA:BB:CC:DD:EE:FF");
        assert_eq!(name.as_deref(), Some("TestDev"));
        assert_eq!(rssi, Some(-55));
        assert_eq!(band, "Near"); // -55 >= -58
    }

    // ── prune_old_sightings ─────────────────────────────────────────

    #[test]
    fn prune_old_sightings_removes_old() {
        let store = open_store();
        let device = make_device("AA:BB:CC:DD:EE:FF");

        // Insert a sighting with a very old timestamp
        store
            .conn
            .execute(
                "INSERT INTO sightings (address, adapter_name, seen_at, proximity_band)
                 VALUES ('AA:BB:CC:DD:EE:FF', 'hci0', datetime('now', '-60 day'), 'Far')",
                [],
            )
            .unwrap();

        // Insert a recent sighting
        store.insert_sighting(&device, Utc::now()).unwrap();

        store.prune_old_sightings(30).unwrap();

        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM sightings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1); // only the recent one survives
    }

    #[test]
    fn prune_old_sightings_keeps_recent() {
        let store = open_store();
        let device = make_device("AA:BB:CC:DD:EE:FF");
        store.insert_sighting(&device, Utc::now()).unwrap();

        store.prune_old_sightings(30).unwrap();

        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM sightings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── empty store ─────────────────────────────────────────────────

    #[test]
    fn load_devices_empty_store() {
        let store = open_store();
        let devices = store.load_devices().unwrap();
        assert!(devices.is_empty());
    }

    // ── multiple devices ────────────────────────────────────────────

    #[test]
    fn multiple_devices_insert_and_load() {
        let store = open_store();
        for i in 0..10 {
            let device = make_device(&format!("AA:BB:CC:DD:EE:{i:02X}"));
            store.upsert_device(&device).unwrap();
        }
        let devices = store.load_devices().unwrap();
        assert_eq!(devices.len(), 10);
    }

    // ── sighting name fallback ──────────────────────────────────────

    #[test]
    fn sighting_with_alias_fallback() {
        let store = open_store();
        let mut device = make_device("AA:BB:CC:DD:EE:FF");
        device.name = None;
        device.alias = Some("My Alias".into());
        store.insert_sighting(&device, fixed_time()).unwrap();

        let name: Option<String> = store
            .conn
            .query_row("SELECT name FROM sightings LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name.as_deref(), Some("My Alias"));
    }

    #[test]
    fn sighting_with_no_name_or_alias() {
        let store = open_store();
        let device = make_device("AA:BB:CC:DD:EE:FF");
        store.insert_sighting(&device, fixed_time()).unwrap();

        let name: Option<String> = store
            .conn
            .query_row("SELECT name FROM sightings LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert!(name.is_none());
    }

    #[test]
    fn multiple_sightings_for_same_device() {
        let store = open_store();
        let device = make_device("AA:BB:CC:DD:EE:FF");
        for i in 0..5 {
            let t = fixed_time() + chrono::Duration::minutes(i);
            store.insert_sighting(&device, t).unwrap();
        }
        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM sightings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn prune_preserves_recent_different_devices() {
        let store = open_store();
        let d1 = make_device("AA:00:00:00:00:01");
        let d2 = make_device("AA:00:00:00:00:02");
        store.insert_sighting(&d1, Utc::now()).unwrap();
        store.insert_sighting(&d2, Utc::now()).unwrap();
        store.prune_old_sightings(30).unwrap();

        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM sightings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
