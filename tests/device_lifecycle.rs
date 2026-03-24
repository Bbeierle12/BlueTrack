use bluetooth_mapper::model::{AppSnapshot, DeviceRecord, bytes_to_hex};
use chrono::{TimeZone, Utc};

#[test]
fn device_discover_advertise_stale_cycle() {
    let t0 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 5).unwrap();
    let t2 = Utc.with_ymd_and_hms(2025, 6, 16, 8, 0, 0).unwrap();

    let mut device = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", t0);
    assert_eq!(device.proximity_band(), "Unknown");
    assert_eq!(device.recurrence_label(), "Transient");

    device.note_advertisement(t0, Some(-55));
    assert_eq!(device.proximity_band(), "Near");
    assert_eq!(device.seen_count, 1);

    device.note_advertisement(t1, Some(-65));
    assert_eq!(device.seen_count, 2);
    assert_eq!(device.active_days, 1);

    device.note_advertisement(t2, Some(-70));
    assert_eq!(device.active_days, 2);
    assert_eq!(device.recurrence_label(), "Recurring");

    device.mark_stale();
    assert!(device.stale);
    assert!(device.rssi.is_none());
    // avg_rssi still exists from prior advertisements, so proximity_band
    // falls back to average (-63 dBm) → "Mid", not "Stale"
    assert_eq!(device.proximity_band(), "Mid");

    device.note_advertisement(t2, Some(-60));
    assert!(!device.stale);
    assert_eq!(device.proximity_band(), "Mid");
}

#[test]
fn snapshot_sorting_reflects_stability() {
    let t = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();

    let mut active = DeviceRecord::new("AA:00:00:00:00:01", "hci0", t);
    active.active_days = 10;
    for _ in 0..50 {
        active.note_advertisement(t, Some(-55));
    }

    let mut transient = DeviceRecord::new("AA:00:00:00:00:02", "hci0", t);
    transient.note_advertisement(t, Some(-80));

    let snapshot = AppSnapshot {
        devices: vec![transient, active],
        ..AppSnapshot::default()
    };

    let sorted = snapshot.sorted_devices();
    assert_eq!(sorted[0].address, "AA:00:00:00:00:01");
    assert!(sorted[0].stability_score() > sorted[1].stability_score());
}

#[test]
fn bytes_to_hex_integration() {
    let payload = vec![0x02, 0x15, 0xab, 0xcd, 0xef];
    assert_eq!(bytes_to_hex(&payload), "0215abcdef");
}
