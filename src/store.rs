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
                params![device.address, json, device.last_seen.to_rfc3339()],
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
                    seen_at.to_rfc3339(),
                    device.rssi,
                    device.manufacturer_summary(),
                    device.proximity_band(),
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn prune_old_sightings(&self, retention_days: u32) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM sightings WHERE seen_at < datetime('now', ?1)",
                params![format!("-{} day", retention_days)],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}
