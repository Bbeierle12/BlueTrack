use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManufacturerEntry {
    pub id: u16,
    pub payload_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceDataEntry {
    pub uuid: String,
    pub payload_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub address: String,
    pub adapter_name: String,
    pub name: Option<String>,
    pub alias: Option<String>,
    pub address_type: Option<String>,
    pub icon: Option<String>,
    pub class: Option<u32>,
    pub appearance: Option<u16>,
    pub modalias: Option<String>,
    pub rssi: Option<i16>,
    pub rssi_min: Option<i16>,
    pub rssi_max: Option<i16>,
    pub rssi_sum: i64,
    pub rssi_sum_squares: f64,
    pub rssi_samples: u64,
    pub tx_power: Option<i16>,
    pub battery_percentage: Option<u8>,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub blocked: bool,
    pub services_resolved: bool,
    pub legacy_pairing: bool,
    pub wake_allowed: bool,
    pub uuids: Vec<String>,
    pub manufacturer_data: Vec<ManufacturerEntry>,
    pub service_data: Vec<ServiceDataEntry>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub seen_count: u64,
    pub advertisement_count: u64,
    pub active_days: u32,
    pub last_seen_day: String,
    pub stale: bool,
}

impl DeviceRecord {
    pub fn new(
        address: impl Into<String>,
        adapter_name: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        let last_seen_day = now.format("%Y-%m-%d").to_string();
        Self {
            address: address.into(),
            adapter_name: adapter_name.into(),
            name: None,
            alias: None,
            address_type: None,
            icon: None,
            class: None,
            appearance: None,
            modalias: None,
            rssi: None,
            rssi_min: None,
            rssi_max: None,
            rssi_sum: 0,
            rssi_sum_squares: 0.0,
            rssi_samples: 0,
            tx_power: None,
            battery_percentage: None,
            paired: false,
            trusted: false,
            connected: false,
            blocked: false,
            services_resolved: false,
            legacy_pairing: false,
            wake_allowed: false,
            uuids: Vec::new(),
            manufacturer_data: Vec::new(),
            service_data: Vec::new(),
            first_seen: now,
            last_seen: now,
            seen_count: 0,
            advertisement_count: 0,
            active_days: 1,
            last_seen_day,
            stale: false,
        }
    }

    pub fn display_name(&self) -> &str {
        self.alias
            .as_deref()
            .filter(|value| !value.is_empty())
            .or(self.name.as_deref().filter(|value| !value.is_empty()))
            .unwrap_or("Unknown device")
    }

    /// Called on every advertisement/property-change event from BlueZ.
    /// `is_first` should be true only when the device was just inserted into
    /// the HashMap for the very first time (genuinely never seen before).
    pub fn note_advertisement(&mut self, now: DateTime<Utc>, rssi: Option<i16>, is_first: bool) {
        self.last_seen = now;
        self.advertisement_count = self.advertisement_count.saturating_add(1);
        self.stale = false;

        if is_first {
            self.seen_count = 1;
        }

        let day = now.format("%Y-%m-%d").to_string();
        if day != self.last_seen_day {
            self.active_days = self.active_days.saturating_add(1);
            self.last_seen_day = day;
        }

        if let Some(value) = rssi {
            self.rssi = Some(value);
            self.rssi_min = Some(self.rssi_min.map_or(value, |current| current.min(value)));
            self.rssi_max = Some(self.rssi_max.map_or(value, |current| current.max(value)));
            self.rssi_sum += i64::from(value);
            self.rssi_sum_squares += f64::from(value) * f64::from(value);
            self.rssi_samples = self.rssi_samples.saturating_add(1);
        }
    }

    pub fn is_public_address(&self) -> bool {
        self.address_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("public"))
    }

    pub fn mark_stale(&mut self) {
        self.stale = true;
        self.rssi = None;
    }

    pub fn avg_rssi(&self) -> Option<f32> {
        if self.rssi_samples == 0 {
            None
        } else {
            Some(self.rssi_sum as f32 / self.rssi_samples as f32)
        }
    }

    pub fn rssi_std_dev(&self) -> Option<f32> {
        if self.rssi_samples < 2 {
            return None;
        }
        let samples = self.rssi_samples as f64;
        let mean = self.rssi_sum as f64 / samples;
        let variance = (self.rssi_sum_squares / samples) - mean.powi(2);
        Some(variance.max(0.0).sqrt() as f32)
    }

    pub fn proximity_band(&self) -> &'static str {
        match self
            .rssi
            .or_else(|| self.avg_rssi().map(|value| value.round() as i16))
        {
            Some(value) if value >= -58 => "Near",
            Some(value) if value >= -74 => "Mid",
            Some(_) => "Far",
            None if self.stale => "Stale",
            None => "Unknown",
        }
    }

    pub fn stability_score(&self) -> u8 {
        let mut score = 35.0;
        score += (self.seen_count.min(120) as f32) * 0.35;
        score += (self.active_days.min(14) as f32) * 2.0;
        if self.connected {
            score += 8.0;
        }
        if let Some(std_dev) = self.rssi_std_dev() {
            score += 30.0 - std_dev.min(30.0);
        }
        score.clamp(0.0, 100.0) as u8
    }

    pub fn recurrence_label(&self) -> &'static str {
        match self.active_days {
            0 | 1 => "Transient",
            2..=4 => "Recurring",
            _ => "Resident",
        }
    }

    pub fn manufacturer_summary(&self) -> String {
        if self.manufacturer_data.is_empty() {
            "None".to_string()
        } else {
            self.manufacturer_data
                .iter()
                .map(|entry| format!("0x{:04x}", entry.id))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterStatus {
    pub name: String,
    pub alias: String,
    pub address: Option<String>,
    pub address_type: Option<String>,
    pub powered: bool,
    pub discovering: bool,
    pub discoverable: bool,
    pub pairable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub at: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub started_at: DateTime<Utc>,
    pub total_devices: usize,
    pub live_devices: usize,
    pub stale_devices: usize,
    pub public_devices: usize,
    pub random_devices: usize,
    pub scan_restarts: u64,
    pub bluez_errors: u64,
}

impl Default for RuntimeMetrics {
    fn default() -> Self {
        Self {
            started_at: Utc::now(),
            total_devices: 0,
            live_devices: 0,
            stale_devices: 0,
            public_devices: 0,
            random_devices: 0,
            scan_restarts: 0,
            bluez_errors: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub generated_at: DateTime<Utc>,
    pub active_adapter: Option<String>,
    pub scan_active: bool,
    pub status_line: String,
    pub last_error: Option<String>,
    pub adapters: Vec<AdapterStatus>,
    pub devices: Vec<DeviceRecord>,
    pub metrics: RuntimeMetrics,
    pub event_log: Vec<EventLogEntry>,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            generated_at: Utc::now(),
            active_adapter: None,
            scan_active: false,
            status_line: "Idle".to_string(),
            last_error: None,
            adapters: Vec::new(),
            devices: Vec::new(),
            metrics: RuntimeMetrics::default(),
            event_log: Vec::new(),
        }
    }
}

impl AppSnapshot {
    pub fn sorted_devices(&self) -> Vec<&DeviceRecord> {
        let mut devices = self.devices.iter().collect::<Vec<_>>();
        devices.sort_by(|left, right| {
            right
                .stability_score()
                .cmp(&left.stability_score())
                .then_with(|| right.last_seen.cmp(&left.last_seen))
                .then_with(|| compare_option_i16(right.rssi, left.rssi))
                .then_with(|| left.address.cmp(&right.address))
        });
        devices
    }
}

fn compare_option_i16(left: Option<i16>, right: Option<i16>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

pub fn format_relative_time(time: DateTime<Utc>) -> String {
    let delta = Utc::now().signed_duration_since(time);
    if delta.num_seconds() < 5 {
        "just now".to_string()
    } else if delta.num_seconds() < 60 {
        format!("{}s ago", delta.num_seconds())
    } else if delta.num_minutes() < 60 {
        format!("{}m ago", delta.num_minutes())
    } else if delta.num_hours() < 48 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
