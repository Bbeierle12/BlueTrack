/// Decode a BlueZ UUID string to a human-readable service name.
///
/// BlueZ returns UUIDs in full 128-bit lowercase format:
/// "0000XXXX-0000-1000-8000-00805f9b34fb" for Bluetooth SIG assigned numbers.
/// Vendor-specific UUIDs use different base UUIDs.
pub fn decode_uuid(uuid: &str) -> Option<&'static str> {
    let lower = uuid.to_ascii_lowercase();

    // Bluetooth SIG standard services: "0000XXXX-0000-1000-8000-00805f9b34fb"
    if lower.len() == 36 && lower[8..] == *"-0000-1000-8000-00805f9b34fb" {
        let hex = &lower[4..8];
        if let Ok(id) = u16::from_str_radix(hex, 16)
            && let Some(name) = lookup_service(id)
        {
            return Some(name);
        }
    }

    // Vendor / member UUIDs: "0000XXXX-0000-1000-8000-00805f9b34fb" won't match
    // all vendors. Check the full UUID against known vendor strings.
    match lower.as_str() {
        // Apple
        "0000fd43-0000-1000-8000-00805f9b34fb" => Some("Apple Find My"),
        "0000fd44-0000-1000-8000-00805f9b34fb" => Some("Apple Continuity"),
        "0000fd5a-0000-1000-8000-00805f9b34fb" => Some("Apple AirDrop"),
        "0000fd6f-0000-1000-8000-00805f9b34fb" => Some("Apple Exposure Notification"),
        "d0611e78-bbb4-4591-a5f8-487910ae4366" => Some("Apple Notification Center"),
        "7905f431-b5ce-4e99-a40f-4b1e122d00d0" => Some("Apple Media Remote"),
        // Tile
        "0000feed-0000-1000-8000-00805f9b34fb" => Some("Tile Tracker"),
        // Google
        "0000fe9f-0000-1000-8000-00805f9b34fb" => Some("Google Fast Pair"),
        "0000feaa-0000-1000-8000-00805f9b34fb" => Some("Eddystone Beacon"),
        "0000febe-0000-1000-8000-00805f9b34fb" => Some("Google Nearby"),
        // Samsung
        "0000fd51-0000-1000-8000-00805f9b34fb" => Some("Samsung SmartTag"),
        // Microsoft
        "0000fe01-0000-1000-8000-00805f9b34fb" => Some("Microsoft Swift Pair"),
        _ => None,
    }
}

/// Look up a Bluetooth SIG assigned 16-bit service UUID.
fn lookup_service(id: u16) -> Option<&'static str> {
    match id {
        // Generic profiles
        0x1800 => Some("Generic Access"),
        0x1801 => Some("Generic Attribute"),
        0x1802 => Some("Immediate Alert"),
        0x1803 => Some("Link Loss"),
        0x1804 => Some("TX Power"),
        0x1805 => Some("Current Time"),
        0x1806 => Some("Reference Time Update"),
        0x1807 => Some("Next DST Change"),
        // Health
        0x1808 => Some("Glucose"),
        0x1809 => Some("Health Thermometer"),
        0x180A => Some("Device Information"),
        0x180D => Some("Heart Rate"),
        0x180E => Some("Phone Alert Status"),
        0x180F => Some("Battery"),
        0x1810 => Some("Blood Pressure"),
        0x1811 => Some("Alert Notification"),
        0x1812 => Some("Human Interface Device"),
        0x1813 => Some("Scan Parameters"),
        0x1814 => Some("Running Speed & Cadence"),
        0x1815 => Some("Automation IO"),
        0x1816 => Some("Cycling Speed & Cadence"),
        0x1818 => Some("Cycling Power"),
        0x1819 => Some("Location & Navigation"),
        0x181A => Some("Environmental Sensing"),
        0x181B => Some("Body Composition"),
        0x181C => Some("User Data"),
        0x181D => Some("Weight Scale"),
        0x181E => Some("Bond Management"),
        0x181F => Some("Continuous Glucose Monitoring"),
        0x1820 => Some("Internet Protocol Support"),
        0x1821 => Some("Indoor Positioning"),
        0x1822 => Some("Pulse Oximeter"),
        0x1823 => Some("HTTP Proxy"),
        0x1824 => Some("Transport Discovery"),
        0x1825 => Some("Object Transfer"),
        0x1826 => Some("Fitness Machine"),
        0x1827 => Some("Mesh Provisioning"),
        0x1828 => Some("Mesh Proxy"),
        0x1829 => Some("Reconnection Configuration"),
        0x183A => Some("Insulin Delivery"),
        0x183E => Some("Physical Activity Monitor"),
        0x1843 => Some("Audio Stream Control"),
        0x1844 => Some("Broadcast Audio Scan"),
        0x1845 => Some("Published Audio Capabilities"),
        0x1848 => Some("Common Audio"),
        // Classic BT profiles
        0x1101 => Some("Serial Port"),
        0x1104 => Some("IrMC Sync"),
        0x1105 => Some("OBEX Object Push"),
        0x1106 => Some("OBEX File Transfer"),
        0x110A => Some("Audio Source (A2DP)"),
        0x110B => Some("Audio Sink (A2DP)"),
        0x110C => Some("AV Remote Target (AVRCP)"),
        0x110D => Some("Advanced Audio (A2DP)"),
        0x110E => Some("AV Remote (AVRCP)"),
        0x110F => Some("AV Remote Controller"),
        0x1112 => Some("Headset Audio Gateway"),
        0x1115 => Some("Personal Area Network"),
        0x1116 => Some("Network Access Point"),
        0x111E => Some("Hands-Free"),
        0x111F => Some("Hands-Free Audio Gateway"),
        0x1124 => Some("Human Interface Device"),
        0x112D => Some("SIM Access"),
        0x112F => Some("Phonebook Access"),
        0x1132 => Some("Message Access"),
        0x1200 => Some("PnP Information"),
        0x1203 => Some("Generic Audio"),
        _ => None,
    }
}

/// Look up a Bluetooth SIG assigned company ID (used in manufacturer-specific data).
pub fn lookup_company(id: u16) -> Option<&'static str> {
    match id {
        0x0000 => Some("Ericsson Technology"),
        0x0001 => Some("Nokia Mobile Phones"),
        0x0002 => Some("Intel Corp."),
        0x0003 => Some("IBM Corp."),
        0x0004 => Some("Toshiba Corp."),
        0x0005 => Some("3Com"),
        0x0006 => Some("Microsoft"),
        0x0009 => Some("Infineon Technologies"),
        0x000D => Some("Motorola"),
        0x000F => Some("Broadcom"),
        0x0010 => Some("Mitsumi Electric"),
        0x001D => Some("Qualcomm"),
        0x0023 => Some("Broadcom"),
        0x002D => Some("Zarlink Semiconductor"),
        0x0036 => Some("Alps Electric"),
        0x003B => Some("Acer"),
        0x0046 => Some("Sony Ericsson"),
        0x0059 => Some("Nordic Semiconductor"),
        0x0061 => Some("Logitech"),
        0x0069 => Some("Cambridge Silicon Radio"),
        0x006B => Some("Mediatek"),
        0x0075 => Some("Samsung Electronics"),
        0x0087 => Some("Garmin International"),
        0x00D7 => Some("Tile"),
        0x00E0 => Some("Google"),
        0x00F0 => Some("Samsung Electro-Mechanics"),
        0x0101 => Some("Nokia"),
        0x010F => Some("Qualcomm Labs"),
        0x0118 => Some("Polar Electro"),
        0x0122 => Some("Beats Electronics"),
        0x0133 => Some("Jabra"),
        0x015B => Some("Microchip Technology"),
        0x0157 => Some("Huawei Technologies"),
        0x0170 => Some("Amazon"),
        0x017E => Some("Xiaomi"),
        0x01E3 => Some("Fitbit"),
        0x01FF => Some("Microsoft"),
        0x0220 => Some("Pebble Technology"),
        0x0244 => Some("Plantronics"),
        0x02FF => Some("Realtek Semiconductor"),
        0x0393 => Some("Suunto"),
        0x0499 => Some("Ruuvi Innovations"),
        0x004C => Some("Apple"),
        0x04C0 => Some("Motorola Mobility"),
        0x05A7 => Some("Sonos"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hid_uuid() {
        assert_eq!(
            decode_uuid("00001812-0000-1000-8000-00805f9b34fb"),
            Some("Human Interface Device")
        );
    }

    #[test]
    fn decode_heart_rate_uuid() {
        assert_eq!(
            decode_uuid("0000180d-0000-1000-8000-00805f9b34fb"),
            Some("Heart Rate")
        );
    }

    #[test]
    fn decode_tile_uuid() {
        assert_eq!(
            decode_uuid("0000feed-0000-1000-8000-00805f9b34fb"),
            Some("Tile Tracker")
        );
    }

    #[test]
    fn decode_apple_findmy_uuid() {
        assert_eq!(
            decode_uuid("0000fd43-0000-1000-8000-00805f9b34fb"),
            Some("Apple Find My")
        );
    }

    #[test]
    fn decode_google_fast_pair() {
        assert_eq!(
            decode_uuid("0000fe9f-0000-1000-8000-00805f9b34fb"),
            Some("Google Fast Pair")
        );
    }

    #[test]
    fn decode_unknown_uuid() {
        assert_eq!(decode_uuid("deadbeef-dead-beef-dead-beefdeadbeef"), None);
    }

    #[test]
    fn decode_uuid_case_insensitive() {
        assert_eq!(
            decode_uuid("00001812-0000-1000-8000-00805F9B34FB"),
            Some("Human Interface Device")
        );
    }

    #[test]
    fn lookup_company_apple() {
        assert_eq!(lookup_company(0x004C), Some("Apple"));
    }

    #[test]
    fn lookup_company_tile() {
        assert_eq!(lookup_company(0x00D7), Some("Tile"));
    }

    #[test]
    fn lookup_company_unknown() {
        assert_eq!(lookup_company(0xFFFF), None);
    }
}
