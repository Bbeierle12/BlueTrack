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
#[serde(default)]
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

impl Default for DeviceRecord {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            address: String::new(),
            adapter_name: String::new(),
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
            last_seen_day: now.format("%Y-%m-%d").to_string(),
            stale: false,
        }
    }
}

impl DeviceRecord {
    /// Creates a new device record with the given address, adapter, and discovery time.
    ///
    /// # Examples
    ///
    /// ```
    /// use bluetooth_mapper::model::DeviceRecord;
    /// use chrono::Utc;
    ///
    /// let device = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", Utc::now());
    /// assert_eq!(device.address, "AA:BB:CC:DD:EE:FF");
    /// assert_eq!(device.seen_count, 0);
    /// ```
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
    pub fn note_advertisement(&mut self, now: DateTime<Utc>, rssi: Option<i16>) {
        self.last_seen = now;
        self.advertisement_count = self.advertisement_count.saturating_add(1);
        self.stale = false;

        self.seen_count = self.seen_count.saturating_add(1);

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

pub(crate) fn compare_option_i16(left: Option<i16>, right: Option<i16>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

/// Formats a timestamp as a human-readable relative time string.
///
/// # Examples
///
/// ```
/// use bluetooth_mapper::model::format_relative_time;
/// use chrono::Utc;
///
/// let result = format_relative_time(Utc::now());
/// assert_eq!(result, "just now");
/// ```
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

/// Formats elapsed time since `start` as a compact duration string (no "ago" suffix).
pub fn format_duration_since(start: DateTime<Utc>) -> String {
    let delta = Utc::now().signed_duration_since(start);
    if delta.num_seconds() < 60 {
        format!("{}s", delta.num_seconds().max(0))
    } else if delta.num_minutes() < 60 {
        format!("{}m", delta.num_minutes())
    } else if delta.num_hours() < 48 {
        format!("{}h", delta.num_hours())
    } else {
        format!("{}d", delta.num_days())
    }
}

/// Converts a byte slice to a lowercase hex string.
///
/// # Examples
///
/// ```
/// use bluetooth_mapper::model::bytes_to_hex;
///
/// assert_eq!(bytes_to_hex(&[0x0a, 0xff, 0x00]), "0aff00");
/// assert_eq!(bytes_to_hex(&[]), "");
/// ```
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()
    }

    fn make_device() -> DeviceRecord {
        DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", fixed_time())
    }

    // ── DeviceRecord::new initial state ─────────────────────────────

    #[test]
    fn new_device_initial_state() {
        let d = make_device();
        assert_eq!(d.address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(d.adapter_name, "hci0");
        assert_eq!(d.seen_count, 0);
        assert_eq!(d.advertisement_count, 0);
        assert_eq!(d.active_days, 1);
        assert!(d.name.is_none());
        assert!(d.alias.is_none());
        assert!(d.rssi.is_none());
        assert!(!d.stale);
        assert_eq!(d.last_seen_day, "2025-06-15");
    }

    #[test]
    fn new_device_first_last_seen_equal() {
        let d = make_device();
        assert_eq!(d.first_seen, d.last_seen);
    }

    // ── display_name fallbacks ──────────────────────────────────────

    #[test]
    fn display_name_prefers_alias() {
        let mut d = make_device();
        d.alias = Some("My Speaker".into());
        d.name = Some("JBL Flip".into());
        assert_eq!(d.display_name(), "My Speaker");
    }

    #[test]
    fn display_name_falls_back_to_name() {
        let mut d = make_device();
        d.alias = None;
        d.name = Some("JBL Flip".into());
        assert_eq!(d.display_name(), "JBL Flip");
    }

    #[test]
    fn display_name_empty_alias_falls_back_to_name() {
        let mut d = make_device();
        d.alias = Some("".into());
        d.name = Some("JBL Flip".into());
        assert_eq!(d.display_name(), "JBL Flip");
    }

    #[test]
    fn display_name_empty_both_yields_unknown() {
        let mut d = make_device();
        d.alias = Some("".into());
        d.name = Some("".into());
        assert_eq!(d.display_name(), "Unknown device");
    }

    #[test]
    fn display_name_none_both_yields_unknown() {
        let d = make_device();
        assert_eq!(d.display_name(), "Unknown device");
    }

    // ── note_advertisement ──────────────────────────────────────────

    #[test]
    fn note_advertisement_increments_counts() {
        let mut d = make_device();
        let t1 = fixed_time();
        d.note_advertisement(t1, Some(-60));
        assert_eq!(d.seen_count, 1);
        assert_eq!(d.advertisement_count, 1);

        d.note_advertisement(t1, Some(-65));
        assert_eq!(d.seen_count, 2);
        assert_eq!(d.advertisement_count, 2);
    }

    #[test]
    fn note_advertisement_clears_stale() {
        let mut d = make_device();
        d.stale = true;
        d.note_advertisement(fixed_time(), None);
        assert!(!d.stale);
    }

    #[test]
    fn note_advertisement_tracks_active_days() {
        let mut d = make_device();
        let day1 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let day2 = Utc.with_ymd_and_hms(2025, 6, 16, 8, 0, 0).unwrap();
        d.note_advertisement(day1, None);
        assert_eq!(d.active_days, 1);

        d.note_advertisement(day2, None);
        assert_eq!(d.active_days, 2);
    }

    #[test]
    fn note_advertisement_same_day_no_increment() {
        let mut d = make_device();
        let t1 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2025, 6, 15, 18, 0, 0).unwrap();
        d.note_advertisement(t1, None);
        d.note_advertisement(t2, None);
        assert_eq!(d.active_days, 1);
    }

    #[test]
    fn note_advertisement_tracks_rssi_stats() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), Some(-50));
        d.note_advertisement(fixed_time(), Some(-70));
        assert_eq!(d.rssi, Some(-70)); // last value
        assert_eq!(d.rssi_min, Some(-70));
        assert_eq!(d.rssi_max, Some(-50));
        assert_eq!(d.rssi_samples, 2);
        assert_eq!(d.rssi_sum, -120);
    }

    #[test]
    fn note_advertisement_none_rssi_no_stats_change() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), None);
        assert_eq!(d.rssi_samples, 0);
        assert!(d.rssi.is_none());
    }

    // ── RSSI statistics ─────────────────────────────────────────────

    #[test]
    fn avg_rssi_zero_samples() {
        let d = make_device();
        assert!(d.avg_rssi().is_none());
    }

    #[test]
    fn avg_rssi_single_sample() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), Some(-60));
        assert!((d.avg_rssi().unwrap() - (-60.0)).abs() < 0.01);
    }

    #[test]
    fn avg_rssi_multiple_samples() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), Some(-50));
        d.note_advertisement(fixed_time(), Some(-70));
        assert!((d.avg_rssi().unwrap() - (-60.0)).abs() < 0.01);
    }

    #[test]
    fn rssi_std_dev_zero_samples() {
        let d = make_device();
        assert!(d.rssi_std_dev().is_none());
    }

    #[test]
    fn rssi_std_dev_one_sample() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), Some(-60));
        assert!(d.rssi_std_dev().is_none());
    }

    #[test]
    fn rssi_std_dev_identical_samples() {
        let mut d = make_device();
        for _ in 0..10 {
            d.note_advertisement(fixed_time(), Some(-60));
        }
        let sd = d.rssi_std_dev().unwrap();
        assert!(sd.abs() < 0.001, "identical samples should have ~0 std dev, got {sd}");
    }

    #[test]
    fn rssi_std_dev_varied_samples() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), Some(-50));
        d.note_advertisement(fixed_time(), Some(-70));
        let sd = d.rssi_std_dev().unwrap();
        assert!(sd > 0.0);
        assert!(!sd.is_nan());
        // stddev of [-50, -70] = 10.0
        assert!((sd - 10.0).abs() < 0.01, "expected ~10.0, got {sd}");
    }

    #[test]
    fn rssi_std_dev_no_nan_or_negative() {
        let mut d = make_device();
        for v in [-100, -99, -100, -99, -100] {
            d.note_advertisement(fixed_time(), Some(v));
        }
        let sd = d.rssi_std_dev().unwrap();
        assert!(sd >= 0.0);
        assert!(!sd.is_nan());
    }

    // ── proximity_band ──────────────────────────────────────────────

    #[test]
    fn proximity_band_near_at_minus_58() {
        let mut d = make_device();
        d.rssi = Some(-58);
        assert_eq!(d.proximity_band(), "Near");
    }

    #[test]
    fn proximity_band_near_above_minus_58() {
        let mut d = make_device();
        d.rssi = Some(-30);
        assert_eq!(d.proximity_band(), "Near");
    }

    #[test]
    fn proximity_band_mid_at_minus_74() {
        let mut d = make_device();
        d.rssi = Some(-74);
        assert_eq!(d.proximity_band(), "Mid");
    }

    #[test]
    fn proximity_band_mid_at_minus_59() {
        let mut d = make_device();
        d.rssi = Some(-59);
        assert_eq!(d.proximity_band(), "Mid");
    }

    #[test]
    fn proximity_band_far_below_minus_74() {
        let mut d = make_device();
        d.rssi = Some(-75);
        assert_eq!(d.proximity_band(), "Far");
    }

    #[test]
    fn proximity_band_stale_no_rssi() {
        let mut d = make_device();
        d.mark_stale();
        assert_eq!(d.proximity_band(), "Stale");
    }

    #[test]
    fn proximity_band_unknown_no_rssi_not_stale() {
        let d = make_device();
        assert_eq!(d.proximity_band(), "Unknown");
    }

    #[test]
    fn proximity_band_falls_back_to_avg_rssi() {
        let mut d = make_device();
        d.rssi = None;
        d.rssi_sum = -120;
        d.rssi_sum_squares = 7200.0;
        d.rssi_samples = 2;
        // avg = -60, rounds to -60, >= -74 => "Mid"... wait, -60 >= -58? No. -60 < -58.
        // Actually -60 >= -74 is true, so "Mid" — but -60 >= -58 is false. So "Mid".
        assert_eq!(d.proximity_band(), "Mid");
    }

    // ── stability_score ─────────────────────────────────────────────

    #[test]
    fn stability_score_new_device() {
        let d = make_device();
        let score = d.stability_score();
        // Base 35.0 + seen_count(0)*0.35 + active_days(1)*2.0 = 37.0
        assert_eq!(score, 37);
    }

    #[test]
    fn stability_score_clamped_to_100() {
        let mut d = make_device();
        d.seen_count = 200;
        d.active_days = 20;
        d.connected = true;
        d.rssi_sum = -600;
        d.rssi_sum_squares = 36000.0;
        d.rssi_samples = 10;
        let score = d.stability_score();
        assert!(score <= 100);
    }

    #[test]
    fn stability_score_never_negative() {
        let d = make_device();
        // u8 is always >= 0, but verify the score is reasonable
        assert!(d.stability_score() <= 100);
    }

    // ── recurrence_label ────────────────────────────────────────────

    #[test]
    fn recurrence_label_transient() {
        let mut d = make_device();
        d.active_days = 1;
        assert_eq!(d.recurrence_label(), "Transient");
    }

    #[test]
    fn recurrence_label_recurring() {
        let mut d = make_device();
        d.active_days = 3;
        assert_eq!(d.recurrence_label(), "Recurring");
    }

    #[test]
    fn recurrence_label_resident() {
        let mut d = make_device();
        d.active_days = 5;
        assert_eq!(d.recurrence_label(), "Resident");
    }

    // ── manufacturer_summary ────────────────────────────────────────

    #[test]
    fn manufacturer_summary_empty() {
        let d = make_device();
        assert_eq!(d.manufacturer_summary(), "None");
    }

    #[test]
    fn manufacturer_summary_single() {
        let mut d = make_device();
        d.manufacturer_data.push(ManufacturerEntry {
            id: 0x004c,
            payload_hex: "0215".into(),
        });
        assert_eq!(d.manufacturer_summary(), "0x004c");
    }

    #[test]
    fn manufacturer_summary_multiple() {
        let mut d = make_device();
        d.manufacturer_data.push(ManufacturerEntry {
            id: 0x004c,
            payload_hex: "".into(),
        });
        d.manufacturer_data.push(ManufacturerEntry {
            id: 0x0006,
            payload_hex: "".into(),
        });
        assert_eq!(d.manufacturer_summary(), "0x004c, 0x0006");
    }

    // ── mark_stale ──────────────────────────────────────────────────

    #[test]
    fn mark_stale_clears_rssi_and_sets_flag() {
        let mut d = make_device();
        d.rssi = Some(-50);
        d.mark_stale();
        assert!(d.stale);
        assert!(d.rssi.is_none());
    }

    // ── is_public_address ───────────────────────────────────────────

    #[test]
    fn is_public_address_true() {
        let mut d = make_device();
        d.address_type = Some("public".into());
        assert!(d.is_public_address());
    }

    #[test]
    fn is_public_address_case_insensitive() {
        let mut d = make_device();
        d.address_type = Some("PUBLIC".into());
        assert!(d.is_public_address());
    }

    #[test]
    fn is_public_address_random() {
        let mut d = make_device();
        d.address_type = Some("random".into());
        assert!(!d.is_public_address());
    }

    #[test]
    fn is_public_address_none() {
        let d = make_device();
        assert!(!d.is_public_address());
    }

    // ── sorted_devices ──────────────────────────────────────────────

    #[test]
    fn sorted_devices_stability_descending() {
        let t = fixed_time();
        let mut d1 = DeviceRecord::new("AA:00:00:00:00:01", "hci0", t);
        let mut d2 = DeviceRecord::new("AA:00:00:00:00:02", "hci0", t);
        d1.active_days = 10;
        d2.active_days = 1;

        let snap = AppSnapshot {
            devices: vec![d2, d1],
            ..AppSnapshot::default()
        };
        let sorted = snap.sorted_devices();
        assert!(sorted[0].stability_score() >= sorted[1].stability_score());
    }

    #[test]
    fn sorted_devices_tiebreaker_last_seen() {
        let t1 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2025, 6, 15, 13, 0, 0).unwrap();
        let d1 = DeviceRecord::new("AA:00:00:00:00:01", "hci0", t1);
        let d2 = DeviceRecord::new("AA:00:00:00:00:02", "hci0", t2);

        let snap = AppSnapshot {
            devices: vec![d1, d2],
            ..AppSnapshot::default()
        };
        let sorted = snap.sorted_devices();
        assert_eq!(sorted[0].address, "AA:00:00:00:00:02");
    }

    #[test]
    fn sorted_devices_tiebreaker_rssi() {
        let t = fixed_time();
        let mut d1 = DeviceRecord::new("AA:00:00:00:00:01", "hci0", t);
        let mut d2 = DeviceRecord::new("AA:00:00:00:00:02", "hci0", t);
        d1.rssi = Some(-70);
        d2.rssi = Some(-50);

        let snap = AppSnapshot {
            devices: vec![d1, d2],
            ..AppSnapshot::default()
        };
        let sorted = snap.sorted_devices();
        assert_eq!(sorted[0].address, "AA:00:00:00:00:02");
    }

    #[test]
    fn sorted_devices_tiebreaker_address() {
        let t = fixed_time();
        let d1 = DeviceRecord::new("BB:00:00:00:00:01", "hci0", t);
        let d2 = DeviceRecord::new("AA:00:00:00:00:01", "hci0", t);

        let snap = AppSnapshot {
            devices: vec![d1, d2],
            ..AppSnapshot::default()
        };
        let sorted = snap.sorted_devices();
        assert_eq!(sorted[0].address, "AA:00:00:00:00:01");
    }

    // ── format_relative_time ────────────────────────────────────────

    #[test]
    fn format_relative_time_just_now() {
        let result = format_relative_time(Utc::now());
        assert_eq!(result, "just now");
    }

    #[test]
    fn format_relative_time_seconds() {
        let time = Utc::now() - chrono::Duration::seconds(30);
        let result = format_relative_time(time);
        assert!(result.ends_with("s ago"), "expected seconds format, got {result}");
    }

    #[test]
    fn format_relative_time_minutes() {
        let time = Utc::now() - chrono::Duration::minutes(5);
        let result = format_relative_time(time);
        assert!(result.ends_with("m ago"), "expected minutes format, got {result}");
    }

    #[test]
    fn format_relative_time_hours() {
        let time = Utc::now() - chrono::Duration::hours(3);
        let result = format_relative_time(time);
        assert!(result.ends_with("h ago"), "expected hours format, got {result}");
    }

    #[test]
    fn format_relative_time_days() {
        let time = Utc::now() - chrono::Duration::days(5);
        let result = format_relative_time(time);
        assert!(result.ends_with("d ago"), "expected days format, got {result}");
    }

    // ── bytes_to_hex ────────────────────────────────────────────────

    #[test]
    fn bytes_to_hex_empty() {
        assert_eq!(bytes_to_hex(&[]), "");
    }

    #[test]
    fn bytes_to_hex_basic() {
        assert_eq!(bytes_to_hex(&[0x0a, 0xff, 0x00]), "0aff00");
    }

    // ── LogLevel ────────────────────────────────────────────────────

    #[test]
    fn log_level_labels() {
        assert_eq!(LogLevel::Info.label(), "INFO");
        assert_eq!(LogLevel::Warn.label(), "WARN");
        assert_eq!(LogLevel::Error.label(), "ERROR");
    }

    // ── compare_option_i16 ──────────────────────────────────────────

    #[test]
    fn compare_option_i16_cases() {
        assert_eq!(compare_option_i16(Some(5), Some(3)), Ordering::Greater);
        assert_eq!(compare_option_i16(Some(3), Some(5)), Ordering::Less);
        assert_eq!(compare_option_i16(Some(3), Some(3)), Ordering::Equal);
        assert_eq!(compare_option_i16(Some(1), None), Ordering::Greater);
        assert_eq!(compare_option_i16(None, Some(1)), Ordering::Less);
        assert_eq!(compare_option_i16(None, None), Ordering::Equal);
    }

    // ── format_relative_time boundaries ─────────────────────────────

    #[test]
    fn format_relative_time_at_5_second_boundary() {
        let time = Utc::now() - chrono::Duration::seconds(5);
        let result = format_relative_time(time);
        assert!(result.ends_with("s ago"), "expected seconds at boundary, got: {result}");
    }

    #[test]
    fn format_relative_time_at_60_second_boundary() {
        let time = Utc::now() - chrono::Duration::seconds(60);
        let result = format_relative_time(time);
        assert!(result.ends_with("m ago"), "expected minutes at boundary, got: {result}");
    }

    #[test]
    fn format_relative_time_at_48_hour_boundary() {
        let time = Utc::now() - chrono::Duration::hours(48);
        let result = format_relative_time(time);
        assert!(result.ends_with("d ago"), "expected days at 48h boundary, got: {result}");
    }

    // ── bytes_to_hex additional cases ───────────────────────────────

    #[test]
    fn bytes_to_hex_single_byte() {
        assert_eq!(bytes_to_hex(&[0xab]), "ab");
    }

    #[test]
    fn bytes_to_hex_all_zeros() {
        assert_eq!(bytes_to_hex(&[0, 0, 0]), "000000");
    }

    #[test]
    fn bytes_to_hex_all_ff() {
        assert_eq!(bytes_to_hex(&[0xff, 0xff]), "ffff");
    }

    // ── sorted_devices edge cases ───────────────────────────────────

    #[test]
    fn sorted_devices_empty_snapshot() {
        let snap = AppSnapshot::default();
        assert!(snap.sorted_devices().is_empty());
    }

    #[test]
    fn sorted_devices_single_device() {
        let snap = AppSnapshot {
            devices: vec![make_device()],
            ..AppSnapshot::default()
        };
        assert_eq!(snap.sorted_devices().len(), 1);
    }

    // ── RSSI extreme values ─────────────────────────────────────────

    #[test]
    fn note_advertisement_extreme_rssi_values() {
        let mut d = make_device();
        d.note_advertisement(fixed_time(), Some(i16::MIN));
        d.note_advertisement(fixed_time(), Some(i16::MAX));
        assert_eq!(d.rssi_min, Some(i16::MIN));
        assert_eq!(d.rssi_max, Some(i16::MAX));
        assert!(d.avg_rssi().unwrap().is_finite());
    }

    // ── stability_score additional cases ────────────────────────────

    #[test]
    fn stability_score_connected_bonus() {
        let d1 = make_device();
        let mut d2 = make_device();
        d2.connected = true;
        assert!(d2.stability_score() > d1.stability_score());
    }

    #[test]
    fn stability_score_low_std_dev_bonus() {
        let t = fixed_time();
        let mut d = make_device();
        for _ in 0..10 {
            d.note_advertisement(t, Some(-60));
        }
        let score = d.stability_score();
        assert!(score >= 70, "expected >= 70 with stable RSSI, got {score}");
    }

    // ── recurrence_label boundaries ─────────────────────────────────

    #[test]
    fn recurrence_label_zero_days() {
        let mut d = make_device();
        d.active_days = 0;
        assert_eq!(d.recurrence_label(), "Transient");
    }

    #[test]
    fn recurrence_label_boundary_2_days() {
        let mut d = make_device();
        d.active_days = 2;
        assert_eq!(d.recurrence_label(), "Recurring");
    }

    #[test]
    fn recurrence_label_boundary_4_days() {
        let mut d = make_device();
        d.active_days = 4;
        assert_eq!(d.recurrence_label(), "Recurring");
    }
}
