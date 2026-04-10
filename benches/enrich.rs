use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use bluetrack::{
    intelligence::enrich,
    model::{DeviceRecord, ManufacturerEntry, ServiceDataEntry},
};
use chrono::Utc;

fn make_simple_device() -> DeviceRecord {
    DeviceRecord::new("00:03:93:11:22:33", "hci0", Utc::now())
}

fn make_airtag() -> DeviceRecord {
    let mut d = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", Utc::now());
    d.manufacturer_data = vec![ManufacturerEntry {
        id: 0x004C,
        payload_hex: "1219AABBCCDDEEFF001122334455667788990011223344".to_string(),
        company_name: None,
    }];
    d
}

fn make_rich_device() -> DeviceRecord {
    let mut d = DeviceRecord::new("00:03:93:AA:BB:CC", "hci0", Utc::now());
    d.uuids = vec![
        "00001812-0000-1000-8000-00805f9b34fb".to_string(), // HID
        "0000180d-0000-1000-8000-00805f9b34fb".to_string(), // Heart Rate
        "0000110b-0000-1000-8000-00805f9b34fb".to_string(), // A2DP Sink
        "0000111e-0000-1000-8000-00805f9b34fb".to_string(), // HFP
        "00001800-0000-1000-8000-00805f9b34fb".to_string(), // Generic Access
    ];
    d.manufacturer_data = vec![ManufacturerEntry {
        id: 0x004C,
        payload_hex: "07AABB".to_string(), // AirPods type
        company_name: None,
    }];
    d.service_data = vec![ServiceDataEntry {
        uuid: "0000fe9f-0000-1000-8000-00805f9b34fb".to_string(), // Google Fast Pair
        payload_hex: "AABBCC".to_string(),
    }];
    d.icon = Some("audio-headphones".to_string());
    d
}

fn bench_enrich_simple(c: &mut Criterion) {
    c.bench_function("enrich/simple_device", |b| {
        b.iter(|| {
            let mut d = make_simple_device();
            enrich(&mut d);
            d
        });
    });
}

fn bench_enrich_airtag(c: &mut Criterion) {
    c.bench_function("enrich/airtag_tracker", |b| {
        b.iter(|| {
            let mut d = make_airtag();
            enrich(&mut d);
            d
        });
    });
}

fn bench_enrich_rich(c: &mut Criterion) {
    c.bench_function("enrich/rich_device_5_uuids", |b| {
        b.iter(|| {
            let mut d = make_rich_device();
            enrich(&mut d);
            d
        });
    });
}

fn bench_enrich_burst(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrich/burst");
    for n in [10u32, 50, 100, 300] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut devices: Vec<DeviceRecord> = (0..n)
                    .map(|i| {
                        let addr = format!("AA:BB:CC:DD:{:02X}:{:02X}", i / 256, i % 256);
                        DeviceRecord::new(&addr, "hci0", Utc::now())
                    })
                    .collect();
                for d in &mut devices {
                    enrich(d);
                }
                devices
            });
        });
    }
    group.finish();
}

fn bench_snapshot_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot/clone_vec");
    for n in [50u32, 150, 300] {
        let devices: Vec<DeviceRecord> = (0..n)
            .map(|i| {
                let addr = format!("AA:BB:CC:DD:{:02X}:{:02X}", i / 256, i % 256);
                let mut d = DeviceRecord::new(&addr, "hci0", Utc::now());
                enrich(&mut d);
                d
            })
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(n), &devices, |b, devices| {
            b.iter(|| devices.clone());
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_enrich_simple,
    bench_enrich_airtag,
    bench_enrich_rich,
    bench_enrich_burst,
    bench_snapshot_clone,
);
criterion_main!(benches);
