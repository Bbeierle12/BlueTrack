use serde::{Deserialize, Serialize};

use crate::model::DeviceRecord;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DeviceCategory {
    Phone,
    Tablet,
    Computer,
    Headphone,
    Headset,
    Speaker,
    Wearable,
    FitnessTracker,
    HealthSensor,
    InputDevice,
    Beacon,
    Tracker,
    NetworkDevice,
    Printer,
    Vehicle,
    SmartHome,
    #[default]
    Unknown,
}

impl DeviceCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Phone => "Phone",
            Self::Tablet => "Tablet",
            Self::Computer => "Computer",
            Self::Headphone => "Headphones",
            Self::Headset => "Headset",
            Self::Speaker => "Speaker",
            Self::Wearable => "Wearable",
            Self::FitnessTracker => "Fitness",
            Self::HealthSensor => "Health",
            Self::InputDevice => "Input Device",
            Self::Beacon => "Beacon",
            Self::Tracker => "Tracker",
            Self::NetworkDevice => "Networking",
            Self::Printer => "Printer",
            Self::Vehicle => "Vehicle",
            Self::SmartHome => "Smart Home",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrackerConfidence {
    /// Matches a specific, documented tracker signature.
    Definite,
    /// Strong indicators but not a perfect signature match.
    Likely,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackerAlert {
    pub name: String,
    pub confidence: TrackerConfidence,
    pub detail: String,
}

/// Classify a device into a category based on its properties.
pub fn classify(record: &DeviceRecord) -> DeviceCategory {
    // 1. BlueZ icon field — set when the remote device advertises a BT class / appearance.
    if let Some(icon) = &record.icon {
        match icon.as_str() {
            "phone" | "phone-cellular" => return DeviceCategory::Phone,
            "computer" => return DeviceCategory::Computer,
            "modem" | "network-wireless" => return DeviceCategory::NetworkDevice,
            "audio-headphones" | "headphones" => return DeviceCategory::Headphone,
            "audio-headset" | "headset" => return DeviceCategory::Headset,
            "audio-card" | "audio-speakers" => return DeviceCategory::Speaker,
            "input-keyboard" => return DeviceCategory::InputDevice,
            "input-mouse" | "input-tablet" | "input-gaming" => {
                return DeviceCategory::InputDevice
            }
            "printer" => return DeviceCategory::Printer,
            "camera-video" | "camera-photo" => return DeviceCategory::Unknown, // camera
            _ => {}
        }
    }

    // 2. Bluetooth device class bits (classic BT).
    if let Some(class) = record.class {
        if let Some(cat) = classify_bt_class(class) {
            return cat;
        }
    }

    // 3. Appearance value (BLE GATT characteristic 0x2A01).
    if let Some(appearance) = record.appearance {
        if let Some(cat) = classify_appearance(appearance) {
            return cat;
        }
    }

    // 4. Advertised service UUIDs.
    for uuid in &record.uuids {
        let lower = uuid.to_ascii_lowercase();
        // Tracker profiles
        if lower.starts_with("0000fd43") || lower.starts_with("0000feed") || lower.starts_with("0000fd51") {
            return DeviceCategory::Tracker;
        }
        // HID
        if lower.starts_with("00001812") {
            return DeviceCategory::InputDevice;
        }
        // A2DP audio source/sink/profile
        if lower.starts_with("0000110a")
            || lower.starts_with("0000110b")
            || lower.starts_with("0000110d")
        {
            return DeviceCategory::Speaker;
        }
        // Hands-Free Profile → headset
        if lower.starts_with("0000111e") || lower.starts_with("0000111f") {
            return DeviceCategory::Headset;
        }
        // Fitness / health
        if lower.starts_with("0000180d")   // Heart Rate
            || lower.starts_with("00001814") // Running Speed
            || lower.starts_with("00001816") // Cycling Speed
            || lower.starts_with("00001818") // Cycling Power
            || lower.starts_with("00001826") // Fitness Machine
            || lower.starts_with("0000183e") // Physical Activity Monitor
        {
            return DeviceCategory::FitnessTracker;
        }
        // Health sensors
        if lower.starts_with("00001809")   // Health Thermometer
            || lower.starts_with("00001810") // Blood Pressure
            || lower.starts_with("00001808") // Glucose
            || lower.starts_with("00001822") // Pulse Oximeter
            || lower.starts_with("0000183a") // Insulin Delivery
        {
            return DeviceCategory::HealthSensor;
        }
        // Eddystone / iBeacon → Beacon
        if lower.starts_with("0000feaa") {
            return DeviceCategory::Beacon;
        }
    }

    // 5. Manufacturer-specific data heuristics.
    for entry in &record.manufacturer_data {
        match entry.id {
            0x004C => {
                // Apple: inspect payload type byte
                return classify_apple_payload(&entry.payload_hex);
            }
            0x00D7 => return DeviceCategory::Tracker, // Tile
            0x0087 => return DeviceCategory::Wearable, // Garmin
            0x0118 => return DeviceCategory::Wearable, // Polar Electro
            0x0393 => return DeviceCategory::Wearable, // Suunto
            0x01E3 => return DeviceCategory::Wearable, // Fitbit
            0x0499 => return DeviceCategory::Beacon,   // Ruuvi beacon
            _ => {}
        }
    }

    // 6. OUI-based fallback.
    match record.oui_manufacturer.as_deref() {
        Some("Apple") => DeviceCategory::Phone,
        Some("Samsung Electronics") => DeviceCategory::Phone,
        Some("Google") => DeviceCategory::Phone,
        Some("Xiaomi") => DeviceCategory::Phone,
        Some("Huawei Technologies") => DeviceCategory::Phone,
        Some("OnePlus") | Some("OPPO") | Some("Realme") => DeviceCategory::Phone,
        Some("LG Electronics") | Some("Motorola") => DeviceCategory::Phone,
        Some("Microsoft") => DeviceCategory::Computer,
        Some("HP") | Some("Dell") | Some("Lenovo") | Some("Intel") => DeviceCategory::Computer,
        Some("Jabra") | Some("Plantronics") | Some("Sennheiser") => DeviceCategory::Headset,
        Some("Bose") => DeviceCategory::Headphone,
        Some("Sony") => DeviceCategory::Headphone,
        Some("Logitech") => DeviceCategory::InputDevice,
        Some("Fitbit") => DeviceCategory::Wearable,
        Some("Garmin") | Some("Polar Electro") | Some("Suunto") => DeviceCategory::Wearable,
        Some("Tile") => DeviceCategory::Tracker,
        Some("Nordic Semiconductor") | Some("Texas Instruments") => DeviceCategory::Beacon,
        _ => DeviceCategory::Unknown,
    }
}

/// Detect known tracking devices from advertising data.
pub fn detect_tracker(record: &DeviceRecord) -> Option<TrackerAlert> {
    // ── Tile ─────────────────────────────────────────────────────────────────
    for uuid in &record.uuids {
        let lower = uuid.to_ascii_lowercase();
        if lower.starts_with("0000feed") {
            return Some(TrackerAlert {
                name: "Tile Tracker".to_string(),
                confidence: TrackerConfidence::Definite,
                detail: "Advertising Tile Find service (0xFEED)".to_string(),
            });
        }
        // Samsung SmartTag
        if lower.starts_with("0000fd51") {
            return Some(TrackerAlert {
                name: "Samsung SmartTag".to_string(),
                confidence: TrackerConfidence::Definite,
                detail: "Advertising Samsung SmartTag service (0xFD51)".to_string(),
            });
        }
        // Apple Find My accessories (AirTag, AirPods, 3rd-party FindMy)
        if lower.starts_with("0000fd43") {
            return Some(TrackerAlert {
                name: "Apple Find My Device".to_string(),
                confidence: TrackerConfidence::Definite,
                detail: "Advertising Apple Find My service (0xFD43)".to_string(),
            });
        }
    }

    // ── Tile via company ID ───────────────────────────────────────────────────
    for entry in &record.manufacturer_data {
        if entry.id == 0x00D7 {
            return Some(TrackerAlert {
                name: "Tile Tracker".to_string(),
                confidence: TrackerConfidence::Definite,
                detail: format!("Tile company ID (0x00D7), payload: {}", entry.payload_hex),
            });
        }

        // Apple Find My Nearby Interaction: company ID 0x004C, type byte 0x12
        if entry.id == 0x004C {
            let bytes = parse_hex_pairs(&entry.payload_hex);
            if bytes.first().copied() == Some(0x12) {
                let subtype = bytes.get(1).copied().unwrap_or(0);
                // 0x19 = AirTag, other lengths may be other FindMy accessories
                let name = if subtype == 0x19 {
                    "Apple AirTag"
                } else {
                    "Apple Find My Accessory"
                };
                return Some(TrackerAlert {
                    name: name.to_string(),
                    confidence: TrackerConfidence::Definite,
                    detail: format!(
                        "Apple Find My Nearby Interaction (company 0x004C, type 0x12, sub 0x{subtype:02X})"
                    ),
                });
            }
        }
    }

    // ── Service data for known tracker UUIDs ─────────────────────────────────
    for entry in &record.service_data {
        let lower = entry.uuid.to_ascii_lowercase();
        if lower.starts_with("0000feed") {
            return Some(TrackerAlert {
                name: "Tile Tracker".to_string(),
                confidence: TrackerConfidence::Definite,
                detail: "Tile service data present".to_string(),
            });
        }
        if lower.starts_with("0000fd43") {
            return Some(TrackerAlert {
                name: "Apple Find My Device".to_string(),
                confidence: TrackerConfidence::Definite,
                detail: "Apple Find My service data present".to_string(),
            });
        }
    }

    None
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// Classify an Apple device by its manufacturer data payload type byte.
fn classify_apple_payload(payload_hex: &str) -> DeviceCategory {
    let bytes = parse_hex_pairs(payload_hex);
    match bytes.first().copied() {
        Some(0x02) | Some(0x03) => DeviceCategory::Beacon,  // iBeacon
        Some(0x07) => DeviceCategory::Headphone,             // AirPods
        Some(0x08) => DeviceCategory::Wearable,              // Apple Watch
        Some(0x09) => DeviceCategory::Wearable,              // AirPods Pro/Max
        Some(0x0B) | Some(0x0C) => DeviceCategory::Wearable, // Apple Watch newer
        Some(0x10) => DeviceCategory::Phone,                 // Nearby Interaction (iPhone/iPad)
        Some(0x12) => DeviceCategory::Tracker,               // Find My Nearby
        _ => DeviceCategory::Phone,                          // Generic Apple device
    }
}

/// Classify by Bluetooth device class (major device class field, bits 12–8).
fn classify_bt_class(class: u32) -> Option<DeviceCategory> {
    let major = (class >> 8) & 0x1F;
    let minor = (class >> 2) & 0x3F;
    match major {
        0x01 => Some(DeviceCategory::Computer),
        0x02 => Some(DeviceCategory::Phone),
        0x03 => Some(DeviceCategory::NetworkDevice),
        0x04 => {
            // Audio/Video — refine by minor class
            match minor {
                0x01 | 0x02 => Some(DeviceCategory::Headset),
                0x06 | 0x07 => Some(DeviceCategory::Speaker),
                0x09 | 0x0A => Some(DeviceCategory::Headphone),
                _ => Some(DeviceCategory::Speaker),
            }
        }
        0x05 => {
            // Peripheral — refine by minor class
            match (minor >> 4) & 0x03 {
                0x01 => Some(DeviceCategory::InputDevice), // keyboard
                0x02 => Some(DeviceCategory::InputDevice), // pointing device
                _ => Some(DeviceCategory::InputDevice),
            }
        }
        0x06 => Some(DeviceCategory::Printer), // Imaging
        0x07 => Some(DeviceCategory::Wearable),
        _ => None,
    }
}

/// Classify by BLE GATT Appearance value (0x2A01).
fn classify_appearance(appearance: u16) -> Option<DeviceCategory> {
    let category = (appearance >> 6) & 0x3FF;
    let _subcategory = appearance & 0x3F;
    match category {
        0x01 => Some(DeviceCategory::Phone),
        0x02 => Some(DeviceCategory::Computer),
        0x03 => Some(DeviceCategory::Wearable),   // Watch
        0x04 => Some(DeviceCategory::Wearable),   // Clock
        0x05 => Some(DeviceCategory::Speaker),    // Display
        0x06 => Some(DeviceCategory::Wearable),   // Remote Control
        0x07 => Some(DeviceCategory::Speaker),    // Eye-glasses
        0x08 => Some(DeviceCategory::FitnessTracker), // Tag
        0x09 => Some(DeviceCategory::Wearable),   // Keyring
        0x0A => Some(DeviceCategory::Phone),      // Media Player
        0x0B => Some(DeviceCategory::Beacon),     // Barcode Scanner
        0x0C => Some(DeviceCategory::Wearable),   // Thermometer (wearable)
        0x0D => Some(DeviceCategory::HealthSensor), // Heart Rate
        0x0E => Some(DeviceCategory::HealthSensor), // Blood Pressure
        0x0F => Some(DeviceCategory::HealthSensor), // HID (generic)
        0x10 => Some(DeviceCategory::FitnessTracker), // Glucose
        0x11 => Some(DeviceCategory::FitnessTracker), // Running/Walking
        0x12 => Some(DeviceCategory::FitnessTracker), // Cycling
        0x13 => Some(DeviceCategory::FitnessTracker), // Control Device
        0x14 => Some(DeviceCategory::NetworkDevice),  // Network Device
        0x15 => Some(DeviceCategory::Wearable),  // Sensor
        0x16 => Some(DeviceCategory::InputDevice), // Light Fixtures
        0x17 => Some(DeviceCategory::FitnessTracker), // Fan
        0x18 => Some(DeviceCategory::SmartHome),  // HVAC
        0x19 => Some(DeviceCategory::SmartHome),  // Air Conditioning
        0x1A => Some(DeviceCategory::SmartHome),  // Humidifier
        0x1B => Some(DeviceCategory::SmartHome),  // Heating
        0x1C => Some(DeviceCategory::SmartHome),  // Access Control
        0x1D => Some(DeviceCategory::SmartHome),  // Motorized Device
        0x1E => Some(DeviceCategory::SmartHome),  // Power Device
        0x1F => Some(DeviceCategory::HealthSensor), // Light Source
        0x20 => Some(DeviceCategory::InputDevice), // Window Covering
        0x21 => Some(DeviceCategory::SmartHome),  // Audio/Video
        0x22 => Some(DeviceCategory::Wearable),   // Outdoor Sports
        _ => None,
    }
}

/// Parse a hex string of space-delimited or continuous pairs into bytes.
/// Handles both "1219AB" and "12 19 AB" formats.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceRecord, ManufacturerEntry};
    use chrono::Utc;

    fn base_device() -> DeviceRecord {
        DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", Utc::now())
    }

    #[test]
    fn classify_hid_uuid() {
        let mut d = base_device();
        d.uuids = vec!["00001812-0000-1000-8000-00805f9b34fb".to_string()];
        assert_eq!(classify(&d), DeviceCategory::InputDevice);
    }

    #[test]
    fn classify_heart_rate_uuid() {
        let mut d = base_device();
        d.uuids = vec!["0000180d-0000-1000-8000-00805f9b34fb".to_string()];
        assert_eq!(classify(&d), DeviceCategory::FitnessTracker);
    }

    #[test]
    fn classify_a2dp_uuid() {
        let mut d = base_device();
        d.uuids = vec!["0000110b-0000-1000-8000-00805f9b34fb".to_string()];
        assert_eq!(classify(&d), DeviceCategory::Speaker);
    }

    #[test]
    fn classify_tile_uuid() {
        let mut d = base_device();
        d.uuids = vec!["0000feed-0000-1000-8000-00805f9b34fb".to_string()];
        assert_eq!(classify(&d), DeviceCategory::Tracker);
    }

    #[test]
    fn classify_apple_findmy_uuid() {
        let mut d = base_device();
        d.uuids = vec!["0000fd43-0000-1000-8000-00805f9b34fb".to_string()];
        assert_eq!(classify(&d), DeviceCategory::Tracker);
    }

    #[test]
    fn classify_icon_phone() {
        let mut d = base_device();
        d.icon = Some("phone".to_string());
        assert_eq!(classify(&d), DeviceCategory::Phone);
    }

    #[test]
    fn classify_icon_headphones() {
        let mut d = base_device();
        d.icon = Some("audio-headphones".to_string());
        assert_eq!(classify(&d), DeviceCategory::Headphone);
    }

    #[test]
    fn detect_tile_uuid() {
        let mut d = base_device();
        d.uuids = vec!["0000feed-0000-1000-8000-00805f9b34fb".to_string()];
        let alert = detect_tracker(&d).unwrap();
        assert_eq!(alert.name, "Tile Tracker");
        assert_eq!(alert.confidence, TrackerConfidence::Definite);
    }

    #[test]
    fn detect_airtag_payload() {
        let mut d = base_device();
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "1219AABBCCDDEEFF001122334455667788990011223344".to_string(),
            company_name: None,
        }];
        let alert = detect_tracker(&d).unwrap();
        assert_eq!(alert.name, "Apple AirTag");
        assert_eq!(alert.confidence, TrackerConfidence::Definite);
    }

    #[test]
    fn detect_tile_company_id() {
        let mut d = base_device();
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x00D7,
            payload_hex: "AABBCC".to_string(),
            company_name: None,
        }];
        let alert = detect_tracker(&d).unwrap();
        assert_eq!(alert.name, "Tile Tracker");
    }

    #[test]
    fn no_tracker_for_regular_phone() {
        let mut d = base_device();
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "10AABBCC".to_string(), // 0x10 = iPhone nearby
            company_name: None,
        }];
        assert!(detect_tracker(&d).is_none());
    }

    #[test]
    fn parse_hex_pairs_basic() {
        assert_eq!(parse_hex_pairs("1219AB"), vec![0x12, 0x19, 0xAB]);
    }

    #[test]
    fn parse_hex_pairs_odd_length() {
        let result = parse_hex_pairs("123");
        assert_eq!(result, vec![0x12]); // trailing nibble dropped
    }
}
