pub mod classify;
pub mod oui;
pub mod profile;
pub mod uuids;

pub use classify::{DeviceCategory, TrackerAlert, TrackerConfidence};
pub use profile::{AddressTypeDetail, BeaconFormat, BeaconPayload};

use crate::model::DeviceRecord;

/// Enrich a device record with all intelligence fields derived from its raw data.
/// Call this whenever a device's properties are updated.
pub fn enrich(record: &mut DeviceRecord) {
    record.oui_manufacturer = oui::lookup_oui(&record.address).map(str::to_string);
    record.decoded_services = record
        .uuids
        .iter()
        .filter_map(|uuid| uuids::decode_uuid(uuid).map(str::to_string))
        .collect();
    for entry in &mut record.manufacturer_data {
        entry.company_name = uuids::lookup_company(entry.id).map(str::to_string);
    }
    record.category = classify::classify(record);
    record.tracker_alert = classify::detect_tracker(record);
    // Extended profiling
    record.address_type_detail = profile::classify_address_type(record);
    record.beacon = profile::parse_beacon(record);
    record.apple_device_type = profile::decode_apple_device_type(record);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceRecord, ManufacturerEntry, ServiceDataEntry};
    use chrono::Utc;

    fn device(addr: &str) -> DeviceRecord {
        DeviceRecord::new(addr, "hci0", Utc::now())
    }

    #[test]
    fn enrich_resolves_apple_oui() {
        // "00:03:93" is in the Apple OUI table
        let mut d = device("00:03:93:11:22:33");
        enrich(&mut d);
        assert_eq!(d.oui_manufacturer.as_deref(), Some("Apple"));
    }

    #[test]
    fn enrich_unknown_oui_leaves_none() {
        let mut d = device("02:00:00:00:00:01");
        enrich(&mut d);
        assert!(d.oui_manufacturer.is_none());
    }

    #[test]
    fn enrich_decodes_service_uuids() {
        let mut d = device("02:00:00:00:00:01");
        d.uuids = vec![
            "00001812-0000-1000-8000-00805f9b34fb".to_string(), // HID
            "0000180d-0000-1000-8000-00805f9b34fb".to_string(), // Heart Rate
        ];
        enrich(&mut d);
        assert!(d.decoded_services.iter().any(|s| s.contains("Interface Device")));
        assert!(d.decoded_services.iter().any(|s| s.contains("Heart Rate")));
    }

    #[test]
    fn enrich_resolves_company_name_in_manufacturer_data() {
        let mut d = device("02:00:00:00:00:01");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C, // Apple company ID
            payload_hex: "07AABB".to_string(),
            company_name: None,
        }];
        enrich(&mut d);
        assert_eq!(d.manufacturer_data[0].company_name.as_deref(), Some("Apple"));
    }

    #[test]
    fn enrich_airtag_end_to_end() {
        let mut d = device("02:00:00:00:00:01");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "1219AABBCCDDEEFF".to_string(), // type=0x12 sub=0x19 → AirTag
            company_name: None,
        }];
        enrich(&mut d);
        let alert = d.tracker_alert.as_ref().expect("AirTag should produce tracker alert");
        assert_eq!(alert.name, "Apple AirTag");
        assert_eq!(d.category, classify::DeviceCategory::Tracker);
        assert_eq!(d.manufacturer_data[0].company_name.as_deref(), Some("Apple"));
    }

    #[test]
    fn enrich_tile_uuid_end_to_end() {
        let mut d = device("02:00:00:00:00:01");
        d.uuids = vec!["0000feed-0000-1000-8000-00805f9b34fb".to_string()];
        enrich(&mut d);
        let alert = d.tracker_alert.as_ref().expect("Tile should produce tracker alert");
        assert_eq!(alert.name, "Tile Tracker");
        assert_eq!(d.category, classify::DeviceCategory::Tracker);
        assert!(d.decoded_services.iter().any(|s| s.contains("Tile")));
    }

    #[test]
    fn enrich_samsung_smarttag_end_to_end() {
        let mut d = device("02:00:00:00:00:01");
        d.uuids = vec!["0000fd51-0000-1000-8000-00805f9b34fb".to_string()];
        enrich(&mut d);
        let alert = d.tracker_alert.as_ref().expect("SmartTag should produce tracker alert");
        assert_eq!(alert.name, "Samsung SmartTag");
        assert_eq!(d.category, classify::DeviceCategory::Tracker);
    }

    #[test]
    fn enrich_findmy_service_data_end_to_end() {
        let mut d = device("02:00:00:00:00:01");
        d.service_data = vec![ServiceDataEntry {
            uuid: "0000fd43-0000-1000-8000-00805f9b34fb".to_string(),
            payload_hex: "AABBCC".to_string(),
        }];
        enrich(&mut d);
        let alert = d.tracker_alert.as_ref().expect("FindMy service data should produce alert");
        assert_eq!(alert.name, "Apple Find My Device");
    }

    #[test]
    fn enrich_sets_category_from_icon() {
        let mut d = device("02:00:00:00:00:01");
        d.icon = Some("phone".to_string());
        enrich(&mut d);
        assert_eq!(d.category, classify::DeviceCategory::Phone);
        assert!(d.tracker_alert.is_none());
    }

    #[test]
    fn enrich_no_tracker_for_nearby_iphone() {
        let mut d = device("02:00:00:00:00:01");
        d.manufacturer_data = vec![ManufacturerEntry {
            id: 0x004C,
            payload_hex: "10AABBCC".to_string(), // 0x10 = Nearby Interaction (iPhone)
            company_name: None,
        }];
        enrich(&mut d);
        assert!(d.tracker_alert.is_none());
    }
}
