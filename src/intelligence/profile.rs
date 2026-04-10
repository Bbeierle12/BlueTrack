use serde::{Deserialize, Serialize};

use crate::model::DeviceRecord;

// ── Address type classification ──────────────────────────────────────────────

/// Precise Bluetooth address type, derived from the raw address_type field
/// and the top two bits of the first MAC octet (for random addresses).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AddressTypeDetail {
    /// Permanent IEEE-assigned MAC. Stable across power cycles; reliable for tracking.
    Public,
    /// Random static address — set once per power cycle, stable for session.
    /// Top 2 bits of first octet = 11 (≥ 0xC0).
    RandomStatic,
    /// Resolvable private address — rotates periodically using an Identity Resolving Key.
    /// Cannot be tracked across rotation windows without the IRK.
    /// Top 2 bits of first octet = 01 (0x40–0x7F).
    RandomResolvable,
    /// Non-resolvable private address — fully random, no rotation key.
    /// Top 2 bits of first octet = 00 (0x00–0x3F).
    RandomNonResolvable,
    #[default]
    Unknown,
}

impl AddressTypeDetail {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Public => "Public (permanent)",
            Self::RandomStatic => "Random Static",
            Self::RandomResolvable => "Random Resolvable (privacy)",
            Self::RandomNonResolvable => "Random Non-resolvable",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_trackable(&self) -> bool {
        matches!(self, Self::Public | Self::RandomStatic)
    }
}

/// Derive precise address type from a device record.
pub fn classify_address_type(record: &DeviceRecord) -> AddressTypeDetail {
    match record.address_type.as_deref() {
        Some("public") => AddressTypeDetail::Public,
        Some("random") => {
            // Parse first octet of "AA:BB:CC:DD:EE:FF"
            let first_byte = record
                .address
                .split(':')
                .next()
                .and_then(|s| u8::from_str_radix(s, 16).ok());
            match first_byte.map(|b| b & 0xC0) {
                Some(0xC0) => AddressTypeDetail::RandomStatic,
                Some(0x40) => AddressTypeDetail::RandomResolvable,
                Some(0x00) => AddressTypeDetail::RandomNonResolvable,
                _ => AddressTypeDetail::Unknown,
            }
        }
        _ => AddressTypeDetail::Unknown,
    }
}

// ── Beacon payload parsing ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BeaconFormat {
    IBeacon,
    AltBeacon,
    EddystoneUID,
    EddystoneURL,
    EddystoneTLM,
    EddystoneEID,
}

impl BeaconFormat {
    pub fn label(&self) -> &'static str {
        match self {
            Self::IBeacon => "iBeacon",
            Self::AltBeacon => "AltBeacon",
            Self::EddystoneUID => "Eddystone-UID",
            Self::EddystoneURL => "Eddystone-URL",
            Self::EddystoneTLM => "Eddystone-TLM",
            Self::EddystoneEID => "Eddystone-EID",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeaconPayload {
    pub format: BeaconFormat,
    /// iBeacon: Proximity UUID (128-bit).
    pub uuid: Option<String>,
    /// iBeacon: Major value.
    pub major: Option<u16>,
    /// iBeacon: Minor value.
    pub minor: Option<u16>,
    /// Calibrated TX power at 1 metre (dBm), from beacon payload.
    pub calibrated_power_dbm: Option<i8>,
    /// Estimated distance in metres computed from RSSI and calibrated power.
    pub estimated_distance_m: Option<f32>,
    /// Eddystone-UID: 10-byte namespace as hex string.
    pub namespace: Option<String>,
    /// Eddystone-UID: 6-byte instance as hex string.
    pub instance: Option<String>,
    /// Eddystone-URL: decoded URL string.
    pub url: Option<String>,
    /// Eddystone-TLM: battery voltage in millivolts.
    pub battery_mv: Option<u16>,
    /// Eddystone-TLM: temperature in degrees Celsius.
    pub temperature_c: Option<f32>,
}

/// Attempt to parse beacon payload from device advertising data.
pub fn parse_beacon(record: &DeviceRecord) -> Option<BeaconPayload> {
    // Try iBeacon from manufacturer data (Apple company ID 0x004C, type 0x02 or 0x03)
    for entry in &record.manufacturer_data {
        if entry.id == 0x004C {
            if let Some(beacon) = parse_ibeacon(&entry.payload_hex, record.rssi) {
                return Some(beacon);
            }
        }
        // AltBeacon: company-independent, type indicator in first two bytes
        if let Some(beacon) = parse_altbeacon(&entry.payload_hex, entry.id, record.rssi) {
            return Some(beacon);
        }
    }
    // Try Eddystone from service data (UUID 0xFEAA)
    for entry in &record.service_data {
        let lower = entry.uuid.to_ascii_lowercase();
        if lower.starts_with("0000feaa") {
            if let Some(beacon) = parse_eddystone(&entry.payload_hex, record.rssi) {
                return Some(beacon);
            }
        }
    }
    None
}

fn parse_ibeacon(payload_hex: &str, rssi: Option<i16>) -> Option<BeaconPayload> {
    let bytes = parse_hex_pairs(payload_hex);
    // iBeacon format: [type=0x02] [length=0x15] [UUID 16 bytes] [major 2] [minor 2] [tx_power 1]
    // Total: 21 bytes of meaningful data after the type+length prefix
    if bytes.len() < 23 {
        return None;
    }
    if bytes[0] != 0x02 && bytes[0] != 0x03 {
        return None;
    }
    if bytes[1] != 0x15 {
        return None;
    }
    let uuid_bytes = &bytes[2..18];
    let uuid = format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        uuid_bytes[0], uuid_bytes[1], uuid_bytes[2], uuid_bytes[3],
        uuid_bytes[4], uuid_bytes[5],
        uuid_bytes[6], uuid_bytes[7],
        uuid_bytes[8], uuid_bytes[9],
        uuid_bytes[10], uuid_bytes[11], uuid_bytes[12], uuid_bytes[13], uuid_bytes[14], uuid_bytes[15]
    );
    let major = u16::from_be_bytes([bytes[18], bytes[19]]);
    let minor = u16::from_be_bytes([bytes[20], bytes[21]]);
    let tx_power = bytes[22] as i8;
    let distance = estimate_distance(tx_power, rssi);
    Some(BeaconPayload {
        format: BeaconFormat::IBeacon,
        uuid: Some(uuid),
        major: Some(major),
        minor: Some(minor),
        calibrated_power_dbm: Some(tx_power),
        estimated_distance_m: distance,
        namespace: None,
        instance: None,
        url: None,
        battery_mv: None,
        temperature_c: None,
    })
}

fn parse_altbeacon(payload_hex: &str, company_id: u16, rssi: Option<i16>) -> Option<BeaconPayload> {
    let bytes = parse_hex_pairs(payload_hex);
    // AltBeacon: [BEACON_CODE=0xBEAC 2 bytes] [BEACON_ID 20 bytes] [reserved] [tx_power]
    if bytes.len() < 24 {
        return None;
    }
    if bytes[0] != 0xBE || bytes[1] != 0xAC {
        return None;
    }
    // First 16 bytes of BEACON_ID as UUID (common convention)
    let uuid_bytes = &bytes[2..18];
    let uuid = format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        uuid_bytes[0], uuid_bytes[1], uuid_bytes[2], uuid_bytes[3],
        uuid_bytes[4], uuid_bytes[5],
        uuid_bytes[6], uuid_bytes[7],
        uuid_bytes[8], uuid_bytes[9],
        uuid_bytes[10], uuid_bytes[11], uuid_bytes[12], uuid_bytes[13], uuid_bytes[14], uuid_bytes[15]
    );
    let major = u16::from_be_bytes([bytes[18], bytes[19]]);
    let minor = u16::from_be_bytes([bytes[20], bytes[21]]);
    let tx_power = bytes[23] as i8;
    let distance = estimate_distance(tx_power, rssi);
    let _ = company_id; // captured in struct context via manufacturer_data
    Some(BeaconPayload {
        format: BeaconFormat::AltBeacon,
        uuid: Some(uuid),
        major: Some(major),
        minor: Some(minor),
        calibrated_power_dbm: Some(tx_power),
        estimated_distance_m: distance,
        namespace: None,
        instance: None,
        url: None,
        battery_mv: None,
        temperature_c: None,
    })
}

fn parse_eddystone(payload_hex: &str, rssi: Option<i16>) -> Option<BeaconPayload> {
    let bytes = parse_hex_pairs(payload_hex);
    if bytes.is_empty() {
        return None;
    }
    match bytes[0] {
        0x00 => parse_eddystone_uid(&bytes, rssi),
        0x10 => parse_eddystone_url(&bytes, rssi),
        0x20 => parse_eddystone_tlm(&bytes),
        0x30 => Some(BeaconPayload {
            format: BeaconFormat::EddystoneEID,
            uuid: None, major: None, minor: None,
            calibrated_power_dbm: bytes.get(1).copied().map(|b| b as i8),
            estimated_distance_m: None,
            namespace: None, instance: None, url: None,
            battery_mv: None, temperature_c: None,
        }),
        _ => None,
    }
}

fn parse_eddystone_uid(bytes: &[u8], rssi: Option<i16>) -> Option<BeaconPayload> {
    // [frame=0x00] [tx_power] [namespace 10 bytes] [instance 6 bytes] [rfu 2 bytes]
    if bytes.len() < 18 {
        return None;
    }
    let tx_power = bytes[1] as i8;
    let namespace = bytes[2..12]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>();
    let instance = bytes[12..18]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>();
    Some(BeaconPayload {
        format: BeaconFormat::EddystoneUID,
        uuid: None,
        major: None,
        minor: None,
        calibrated_power_dbm: Some(tx_power),
        estimated_distance_m: estimate_distance(tx_power, rssi),
        namespace: Some(namespace),
        instance: Some(instance),
        url: None,
        battery_mv: None,
        temperature_c: None,
    })
}

fn parse_eddystone_url(bytes: &[u8], rssi: Option<i16>) -> Option<BeaconPayload> {
    // [frame=0x10] [tx_power] [url_scheme] [encoded_url...]
    if bytes.len() < 3 {
        return None;
    }
    let tx_power = bytes[1] as i8;
    let scheme = match bytes[2] {
        0x00 => "http://www.",
        0x01 => "https://www.",
        0x02 => "http://",
        0x03 => "https://",
        _ => "unknown://",
    };
    let mut url = scheme.to_string();
    for &b in &bytes[3..] {
        let expansion = match b {
            0x00 => ".com/",
            0x01 => ".org/",
            0x02 => ".edu/",
            0x03 => ".net/",
            0x04 => ".info/",
            0x05 => ".biz/",
            0x06 => ".gov/",
            0x07 => ".com",
            0x08 => ".org",
            0x09 => ".edu",
            0x0A => ".net",
            0x0B => ".info",
            0x0C => ".biz",
            0x0D => ".gov",
            _ if b >= 0x20 && b < 0x7F => {
                url.push(b as char);
                continue;
            }
            _ => continue,
        };
        url.push_str(expansion);
    }
    Some(BeaconPayload {
        format: BeaconFormat::EddystoneURL,
        uuid: None,
        major: None,
        minor: None,
        calibrated_power_dbm: Some(tx_power),
        estimated_distance_m: estimate_distance(tx_power, rssi),
        namespace: None,
        instance: None,
        url: Some(url),
        battery_mv: None,
        temperature_c: None,
    })
}

fn parse_eddystone_tlm(bytes: &[u8]) -> Option<BeaconPayload> {
    // [frame=0x20] [version=0x00] [battery_mV 2 BE] [temperature 2 BE fixed-point 8.8] [adv_cnt 4] [sec_cnt 4]
    if bytes.len() < 14 {
        return None;
    }
    let battery_mv = u16::from_be_bytes([bytes[2], bytes[3]]);
    // Temperature: 8.8 fixed point signed
    let temp_raw = i16::from_be_bytes([bytes[4], bytes[5]]);
    let temperature_c = temp_raw as f32 / 256.0;
    Some(BeaconPayload {
        format: BeaconFormat::EddystoneTLM,
        uuid: None,
        major: None,
        minor: None,
        calibrated_power_dbm: None,
        estimated_distance_m: None,
        namespace: None,
        instance: None,
        url: None,
        battery_mv: Some(battery_mv),
        temperature_c: Some(temperature_c),
    })
}

/// Estimate distance in metres from calibrated TX power and observed RSSI.
/// Uses the log-distance path loss model with n=2 (free-space).
fn estimate_distance(calibrated_power_dbm: i8, rssi: Option<i16>) -> Option<f32> {
    let rssi = rssi?;
    let ratio = (calibrated_power_dbm as f32 - rssi as f32) / 20.0;
    let distance = 10f32.powf(ratio);
    // Clamp to a sane range (1 cm – 100 m)
    Some(distance.clamp(0.01, 100.0))
}

// ── Apple Continuity payload decoding ───────────────────────────────────────

/// Decode the specific Apple device type from Continuity Protocol manufacturer data.
/// Returns a human-readable label, or None if not Apple or type is unknown.
pub fn decode_apple_device_type(record: &DeviceRecord) -> Option<String> {
    for entry in &record.manufacturer_data {
        if entry.id != 0x004C {
            continue;
        }
        let bytes = parse_hex_pairs(&entry.payload_hex);
        let type_byte = bytes.first().copied()?;
        let label = match type_byte {
            0x01 => "AirPrint",
            0x04 => "HomeKit Accessory",
            0x05 => "AirDrop",
            0x06 => "HomeKit (Pairing)",
            0x07 => "AirPods",
            0x08 => "Apple Watch",
            0x09 => {
                // Distinguish AirPods Pro vs AirPods Max by payload length.
                // AirPods Max payload length byte is typically > 0x16.
                let len = bytes.get(1).copied().unwrap_or(0);
                if len >= 0x1E { "AirPods Max" } else { "AirPods Pro" }
            }
            0x0A => "Apple Device (Pairing)",
            0x0B | 0x0C => "Apple Watch (Series 6+)",
            0x0D => "HomeKit (Encrypted Notification)",
            0x0E => "Personal Hotspot",
            0x0F => "Apple Nearby (Continuation)",
            0x10 => "iPhone / iPad",
            0x11 => "HomePod",
            0x12 => {
                let sub = bytes.get(1).copied().unwrap_or(0);
                if sub == 0x19 { "Apple AirTag" } else { "Apple Find My Accessory" }
            }
            0x13 => "Apple Magic Switch",
            0x14 => "Apple Handoff",
            0x15 => "WiFi Network Join",
            0x16 => "Handwritten Note",
            0x17 => "AirPlay Target",
            0x18 => "AirPlay Source",
            0x1E => "Apple TV",
            0x27 => "AirPods (3rd gen)",
            0x2B => "AirPods (4th gen)",
            // iBeacon (0x02/0x03) is handled by parse_beacon, not here
            _ => return None,
        };
        return Some(label.to_string());
    }
    None
}

// ── Advertising interval estimate ───────────────────────────────────────────

/// Estimate the average advertising interval in milliseconds.
/// Requires at least 3 advertisement events for a meaningful estimate.
pub fn estimate_adv_interval_ms(record: &DeviceRecord) -> Option<u32> {
    if record.advertisement_count < 3 {
        return None;
    }
    let elapsed_ms = record
        .last_seen
        .signed_duration_since(record.first_seen)
        .num_milliseconds();
    if elapsed_ms <= 0 {
        return None;
    }
    let avg = elapsed_ms / record.advertisement_count as i64;
    Some(avg.clamp(1, u32::MAX as i64) as u32)
}

// ── Private helpers ──────────────────────────────────────────────────────────

fn parse_hex_pairs(hex: &str) -> Vec<u8> {
    let clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    clean
        .as_bytes()
        .chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                let pair = std::str::from_utf8(chunk).ok()?;
                u8::from_str_radix(pair, 16).ok()
            } else {
                None
            }
        })
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceRecord, ManufacturerEntry, ServiceDataEntry};
    use chrono::Utc;

    fn device(addr: &str) -> DeviceRecord {
        DeviceRecord::new(addr, "hci0", Utc::now())
    }

    // ── address type ──────────────────────────────────────────────────

    #[test]
    fn public_address() {
        let mut d = device("E4:58:BC:5C:8F:BF");
        d.address_type = Some("public".to_string());
        assert_eq!(classify_address_type(&d), AddressTypeDetail::Public);
        assert!(classify_address_type(&d).is_trackable());
    }

    #[test]
    fn random_static_address() {
        // First octet 0xD8 = 0b11011000 → top 2 bits = 11 → static
        let mut d = device("D8:00:00:00:00:01");
        d.address_type = Some("random".to_string());
        assert_eq!(classify_address_type(&d), AddressTypeDetail::RandomStatic);
        assert!(classify_address_type(&d).is_trackable());
    }

    #[test]
    fn random_resolvable_address() {
        // First octet 0x40 = 0b01000000 → top 2 bits = 01 → resolvable
        let mut d = device("40:00:00:00:00:01");
        d.address_type = Some("random".to_string());
        assert_eq!(classify_address_type(&d), AddressTypeDetail::RandomResolvable);
        assert!(!classify_address_type(&d).is_trackable());
    }

    #[test]
    fn random_non_resolvable_address() {
        // First octet 0x1A = 0b00011010 → top 2 bits = 00 → non-resolvable
        let mut d = device("1A:00:00:00:00:01");
        d.address_type = Some("random".to_string());
        assert_eq!(classify_address_type(&d), AddressTypeDetail::RandomNonResolvable);
        assert!(!classify_address_type(&d).is_trackable());
    }

    // ── iBeacon parsing ───────────────────────────────────────────────

    #[test]
    fn ibeacon_parses_uuid_major_minor() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        // type=0x02 len=0x15, UUID=550E8400-E29B-41D4-A716-446655440000, major=1, minor=2, power=-59
        let payload = "0215550E8400E29B41D4A716446655440000000100021005A";
        // Let's build a proper payload: 02 15 [16 UUID bytes] [2 major] [2 minor] [1 power]
        // UUID: AABBCCDD-EEFF-0011-2233-445566778899
        let payload = "0215AABBCCDDEEFF001122334455667788990001000265";
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: payload.to_string(),
            company_name: Some("Apple".to_string()),
        }];
        d.rssi = Some(-70);
        let beacon = parse_beacon(&d).unwrap();
        assert_eq!(beacon.format, BeaconFormat::IBeacon);
        assert_eq!(beacon.major, Some(1));
        assert_eq!(beacon.minor, Some(2));
        assert!(beacon.uuid.is_some());
        assert!(beacon.calibrated_power_dbm.is_some());
        assert!(beacon.estimated_distance_m.is_some());
    }

    #[test]
    fn non_ibeacon_apple_payload_not_parsed_as_beacon() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "10AABBCC".to_string(), // iPhone nearby, not iBeacon
            company_name: Some("Apple".to_string()),
        }];
        assert!(parse_beacon(&d).is_none());
    }

    // ── Eddystone-URL parsing ─────────────────────────────────────────

    #[test]
    fn eddystone_url_parses_https_www() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        // frame=0x10, tx_power=0xEE(-18), scheme=0x01(https://www.), "example", 0x07(.com)
        // "example" = 65 78 61 6D 70 6C 65
        d.service_data = vec![ServiceDataEntry {
            uuid: "0000feaa-0000-1000-8000-00805f9b34fb".to_string(),
            payload_hex: "10EE016578616D706C6507".to_string(),
        }];
        let beacon = parse_beacon(&d).unwrap();
        assert_eq!(beacon.format, BeaconFormat::EddystoneURL);
        let url = beacon.url.unwrap();
        assert!(url.starts_with("https://www."));
        assert!(url.contains("example"));
        assert!(url.ends_with(".com"));
    }

    // ── Eddystone-UID parsing ─────────────────────────────────────────

    #[test]
    fn eddystone_uid_parses_namespace_and_instance() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        // frame=0x00, tx_power=0xF0(-16), namespace=AABBCCDDEEFF00112233, instance=445566778899, rfu=0000
        d.service_data = vec![ServiceDataEntry {
            uuid: "0000feaa-0000-1000-8000-00805f9b34fb".to_string(),
            payload_hex: "00F0AABBCCDDEEFF001122334455667788990000".to_string(),
        }];
        let beacon = parse_beacon(&d).unwrap();
        assert_eq!(beacon.format, BeaconFormat::EddystoneUID);
        assert!(beacon.namespace.is_some());
        assert!(beacon.instance.is_some());
        assert_eq!(beacon.namespace.as_deref().unwrap().len(), 20); // 10 bytes → 20 hex chars
    }

    // ── Apple device type decoding ────────────────────────────────────

    #[test]
    fn decode_airpods_type() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "07AABBCC".to_string(),
            company_name: Some("Apple".to_string()),
        }];
        assert_eq!(decode_apple_device_type(&d).as_deref(), Some("AirPods"));
    }

    #[test]
    fn decode_apple_watch_type() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "08AABBCC".to_string(),
            company_name: Some("Apple".to_string()),
        }];
        assert_eq!(decode_apple_device_type(&d).as_deref(), Some("Apple Watch"));
    }

    #[test]
    fn decode_airtag_type() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "1219AABBCC".to_string(),
            company_name: Some("Apple".to_string()),
        }];
        assert_eq!(decode_apple_device_type(&d).as_deref(), Some("Apple AirTag"));
    }

    #[test]
    fn decode_iphone_type() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "10AABBCC".to_string(),
            company_name: Some("Apple".to_string()),
        }];
        assert_eq!(decode_apple_device_type(&d).as_deref(), Some("iPhone / iPad"));
    }

    #[test]
    fn no_apple_type_for_non_apple() {
        let mut d = device("AA:BB:CC:DD:EE:FF");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x00D7, // Tile
            payload_hex: "AABBCC".to_string(),
            company_name: None,
        }];
        assert!(decode_apple_device_type(&d).is_none());
    }

    // ── Advertising interval estimate ─────────────────────────────────

    #[test]
    fn adv_interval_too_few_samples_returns_none() {
        let d = device("AA:BB:CC:DD:EE:FF");
        // advertisement_count = 0 by default
        assert!(estimate_adv_interval_ms(&d).is_none());
    }

    #[test]
    fn adv_interval_estimate_reasonable() {
        use chrono::Duration;
        let mut d = device("AA:BB:CC:DD:EE:FF");
        let now = Utc::now();
        d.first_seen = now - Duration::seconds(30);
        d.last_seen = now;
        d.advertisement_count = 15;
        let interval = estimate_adv_interval_ms(&d).unwrap();
        // 30000ms / 15 = 2000ms
        assert_eq!(interval, 2000);
    }

    // ── Distance estimation ───────────────────────────────────────────

    #[test]
    fn distance_at_calibration_point_is_one_metre() {
        // When RSSI == calibrated TX power, distance should be 1.0m
        let d = estimate_distance(-59, Some(-59)).unwrap();
        assert!((d - 1.0).abs() < 0.01, "expected ~1.0m, got {d}");
    }

    #[test]
    fn distance_increases_with_lower_rssi() {
        let near = estimate_distance(-59, Some(-59)).unwrap();
        let far = estimate_distance(-59, Some(-79)).unwrap();
        assert!(far > near);
    }
}
