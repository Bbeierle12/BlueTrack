use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use bluer::{
    Adapter, AdapterEvent, Address, DeviceProperty, DiscoveryFilter, DiscoveryTransport, Session,
};
use eframe::egui;
use chrono::Utc;
use futures::StreamExt;
use tokio::{
    runtime::Builder,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        watch,
    },
    task::JoinHandle,
};

use crate::{
    model::{
        AdapterStatus, AppSnapshot, DeviceRecord, EventLogEntry, LogLevel, ManufacturerEntry,
        RuntimeMetrics, ServiceDataEntry, bytes_to_hex, compare_option_i16,
    },
    settings::Settings,
    store::Store,
};

pub struct ScannerHandle {
    pub snapshot: watch::Receiver<AppSnapshot>,
    commands: UnboundedSender<WorkerCommand>,
}

impl ScannerHandle {
    pub fn spawn(settings: Settings, repaint_ctx: egui::Context) -> Self {
        let (command_tx, command_rx) = unbounded_channel();
        let (snapshot_tx, snapshot_rx) = watch::channel(AppSnapshot::default());

        std::thread::Builder::new()
            .name("bluetooth-worker".to_string())
            .spawn(move || {
                let runtime = Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(2)
                    .build()
                    .expect("runtime");

                runtime.block_on(async move {
                    let mut worker = Worker::new(settings, command_rx, snapshot_tx, repaint_ctx);
                    worker.run().await;
                });
            })
            .expect("spawn worker");

        Self {
            snapshot: snapshot_rx,
            commands: command_tx,
        }
    }

    pub fn start_scan(&self) {
        let _ = self.commands.send(WorkerCommand::StartScan);
    }

    pub fn stop_scan(&self) {
        let _ = self.commands.send(WorkerCommand::StopScan);
    }

    pub fn refresh(&self) {
        let _ = self.commands.send(WorkerCommand::Refresh);
    }

    pub fn apply_settings(&self, settings: Settings) {
        let _ = self.commands.send(WorkerCommand::ApplySettings(settings));
    }
}

enum WorkerCommand {
    StartScan,
    StopScan,
    Refresh,
    ApplySettings(Settings),
}

enum InternalEvent {
    AdapterEvent(AdapterEvent),
    ScanEnded,
}

struct Worker {
    settings: Settings,
    commands: UnboundedReceiver<WorkerCommand>,
    snapshot_tx: watch::Sender<AppSnapshot>,
    repaint_ctx: egui::Context,
    internal_tx: UnboundedSender<InternalEvent>,
    internal_rx: UnboundedReceiver<InternalEvent>,
    store: Option<Store>,
    session: Option<Session>,
    devices: HashMap<String, DeviceRecord>,
    adapters: Vec<AdapterStatus>,
    scan_task: Option<JoinHandle<()>>,
    active_adapter: Option<String>,
    metrics: RuntimeMetrics,
    event_log: VecDeque<EventLogEntry>,
    last_error: Option<String>,
    scan_active: bool,
    status_line: String,
    sighting_throttle: HashMap<String, chrono::DateTime<Utc>>,
    upsert_throttle: HashMap<String, chrono::DateTime<Utc>>,
    last_snapshot_at: chrono::DateTime<Utc>,
    tick_count: u64,
}

impl Worker {
    fn new(
        settings: Settings,
        commands: UnboundedReceiver<WorkerCommand>,
        snapshot_tx: watch::Sender<AppSnapshot>,
        repaint_ctx: egui::Context,
    ) -> Self {
        let (internal_tx, internal_rx) = unbounded_channel();
        let store = Store::open(settings.database_path().as_path()).ok();
        let mut devices = HashMap::new();
        if let Some(store) = &store
            && let Ok(restored) = store.load_devices()
        {
            for mut device in restored {
                // Only restore public-address devices; random MACs are ephemeral.
                if device.is_public_address() {
                    device.mark_stale();
                    devices.insert(device.address.clone(), device);
                }
            }
        }

        let metrics = RuntimeMetrics::default();

        Self {
            settings,
            commands,
            snapshot_tx,
            repaint_ctx,
            internal_tx,
            internal_rx,
            store,
            session: None,
            devices,
            adapters: Vec::new(),
            scan_task: None,
            active_adapter: None,
            metrics,
            event_log: VecDeque::with_capacity(120),
            last_error: None,
            scan_active: false,
            status_line: "Starting worker".to_string(),
            sighting_throttle: HashMap::new(),
            upsert_throttle: HashMap::new(),
            last_snapshot_at: chrono::DateTime::<Utc>::MIN_UTC,
            tick_count: 0,
        }
    }

    async fn run(&mut self) {
        match Session::new().await {
            Ok(session) => {
                self.session = Some(session);
                self.log(LogLevel::Info, "BlueZ session established");
            }
            Err(error) => {
                self.fail(format!("BlueZ session failed: {error}"));
            }
        }

        self.refresh_state().await;
        if self.settings.auto_start_scan {
            self.start_scan().await;
        } else {
            self.emit_snapshot();
        }

        let mut tick = tokio::time::interval(Duration::from_secs(
            self.settings.refresh_interval_seconds.max(1),
        ));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    self.tick_count = self.tick_count.wrapping_add(1);
                    self.mark_stale_devices();
                    self.refresh_adapters().await;
                    // Prune old sightings every ~60 ticks (~5 min at default interval)
                    // instead of every tick, to avoid unnecessary DB churn.
                    if self.tick_count % 60 == 0 {
                        if let Some(store) = &self.store {
                            let _ = store.prune_old_sightings(self.settings.retention_days);
                        }
                    }
                    self.sighting_throttle.retain(|address, _| self.devices.contains_key(address));
                    self.upsert_throttle.retain(|address, _| self.devices.contains_key(address));
                    self.emit_snapshot();
                }
                Some(command) = self.commands.recv() => {
                    match command {
                        WorkerCommand::StartScan => self.start_scan().await,
                        WorkerCommand::StopScan => self.stop_scan(),
                        WorkerCommand::Refresh => self.refresh_state().await,
                        WorkerCommand::ApplySettings(settings) => {
                            self.settings = settings;
                            self.log(LogLevel::Info, "Settings applied");
                            self.refresh_state().await;
                            if self.scan_active {
                                self.start_scan().await;
                            }
                        }
                    }
                }
                Some(event) = self.internal_rx.recv() => {
                    match event {
                        InternalEvent::AdapterEvent(event) => self.handle_adapter_event(event).await,
                        InternalEvent::ScanEnded => {
                            self.scan_task.take();
                            self.scan_active = false;
                            self.log(LogLevel::Warn, "Discovery ended unexpectedly (adapter removed or BlueZ stopped)");
                            self.status_line = "Scan ended unexpectedly".to_string();
                            self.emit_snapshot();
                        }
                    }
                }
                else => break,
            }
        }
    }

    async fn refresh_state(&mut self) {
        self.refresh_adapters().await;
        self.mark_stale_devices();
        self.status_line = if self.scan_active {
            format!(
                "Scanning on {}",
                self.active_adapter.as_deref().unwrap_or("unknown")
            )
        } else if self.session.is_some() {
            "Ready".to_string()
        } else {
            "BlueZ unavailable".to_string()
        };
        self.emit_snapshot();
    }

    async fn refresh_adapters(&mut self) {
        let Some(session) = &self.session else {
            self.adapters.clear();
            return;
        };

        match session.adapter_names().await {
            Ok(names) => {
                let mut adapters = Vec::new();
                let mut warnings = Vec::new();
                for name in names {
                    match session.adapter(&name) {
                        Ok(adapter) => {
                            adapters.push(self.read_adapter_status(&adapter).await);
                        }
                        Err(error) => {
                            warnings.push(format!("Failed to read adapter {name}: {error}"));
                        }
                    }
                }
                self.adapters = adapters;
                if self.active_adapter.is_none() {
                    self.active_adapter = self.select_adapter_name();
                }
                for warning in warnings {
                    self.log(LogLevel::Warn, warning);
                }
            }
            Err(error) => self.fail(format!("Failed to enumerate adapters: {error}")),
        }
    }

    async fn start_scan(&mut self) {
        if self.scan_task.is_some() {
            self.stop_scan();
            // Allow BlueZ time to tear down the previous HCI scan.
            // Without this, StartDiscovery can race with StopDiscovery
            // and return org.bluez.Error.InProgress (bluer issue #47).
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        let Some(session) = &self.session else {
            self.fail("Scan start requested before BlueZ session existed".to_string());
            self.emit_snapshot();
            return;
        };

        let adapter_name = match self.select_adapter_name() {
            Some(name) => name,
            None => {
                self.fail("No Bluetooth adapter found".to_string());
                self.emit_snapshot();
                return;
            }
        };

        let adapter = match session.adapter(&adapter_name) {
            Ok(adapter) => adapter,
            Err(error) => {
                self.fail(format!("Failed to open adapter {adapter_name}: {error}"));
                self.emit_snapshot();
                return;
            }
        };

        if let Err(error) = adapter.set_powered(true).await {
            self.fail(format!("Failed to power adapter {adapter_name}: {error}"));
            self.emit_snapshot();
            return;
        }

        let filter = DiscoveryFilter {
            transport: self.settings.scan_transport,
            duplicate_data: self.settings.allow_duplicate_data,
            rssi: Some(self.settings.min_rssi),
            ..Default::default()
        };

        if let Err(error) = adapter.set_discovery_filter(filter).await {
            self.fail(format!("Failed to configure discovery filter: {error}"));
            self.emit_snapshot();
            return;
        }

        match adapter.discover_devices_with_changes().await {
            Ok(mut stream) => {
                self.metrics.scan_restarts = self.metrics.scan_restarts.saturating_add(1);
                self.active_adapter = Some(adapter_name.clone());
                self.scan_active = true;
                self.last_error = None;
                self.status_line = format!("Scanning on {adapter_name}");
                self.log(
                    LogLevel::Info,
                    format!(
                        "Discovery started on {adapter_name} with {} transport",
                        transport_label(self.settings.scan_transport)
                    ),
                );
                let tx = self.internal_tx.clone();
                self.scan_task = Some(tokio::spawn(async move {
                    while let Some(event) = stream.next().await {
                        if tx.send(InternalEvent::AdapterEvent(event)).is_err() {
                            return;
                        }
                    }
                    // Stream ended — BlueZ stopped discovery externally,
                    // adapter was removed, or the session was terminated.
                    let _ = tx.send(InternalEvent::ScanEnded);
                }));
            }
            Err(error) => {
                self.fail(format!(
                    "Failed to start discovery on {adapter_name}: {error}"
                ));
            }
        }

        self.emit_snapshot();
    }

    fn stop_scan(&mut self) {
        if let Some(task) = self.scan_task.take() {
            task.abort();
        }
        self.scan_active = false;
        self.status_line = "Scan stopped".to_string();
        self.log(LogLevel::Info, "Discovery stopped");
        self.emit_snapshot();
    }

    async fn handle_adapter_event(&mut self, event: AdapterEvent) {
        let mut snapshot_needed = false;

        match event {
            AdapterEvent::DeviceAdded(address) => {
                if let Some((device, is_new)) = self.refresh_device(address).await
                    && is_new
                {
                    // Persist a sighting row on genuine new appearances only.
                    let now = device.last_seen;
                    if should_persist_sighting(
                        self.sighting_throttle.get(&device.address),
                        now,
                    ) {
                        if let Some(store) = &self.store {
                            let _ = store.insert_sighting(&device, now);
                        }
                        self.sighting_throttle.insert(device.address.clone(), now);
                    }
                    snapshot_needed = true;
                }
            }
            AdapterEvent::DeviceRemoved(address) => {
                if let Some(device) = self.devices.get_mut(&address.to_string()) {
                    let removed_address = device.address.clone();
                    device.mark_stale();
                    if let Some(store) = &self.store {
                        let _ = store.upsert_device(device);
                    }
                    let _ = device;
                    self.log(LogLevel::Info, format!("Device removed {removed_address}"));
                    snapshot_needed = true;
                }
            }
            AdapterEvent::PropertyChanged(_) => {}
        }

        // Emit immediately for new/removed devices; otherwise throttle to 1/sec
        // to avoid rebuilding the snapshot on every RSSI fluctuation.
        let now = Utc::now();
        if snapshot_needed
            || now.signed_duration_since(self.last_snapshot_at) >= chrono::Duration::seconds(1)
        {
            self.emit_snapshot();
            self.last_snapshot_at = now;
        }
    }

    async fn refresh_device(&mut self, address: Address) -> Option<(DeviceRecord, bool)> {
        let session = self.session.as_ref()?;
        let adapter_name = self
            .active_adapter
            .clone()
            .or_else(|| self.select_adapter_name())
            .unwrap_or_else(|| "hci0".to_string());
        let adapter = session.adapter(&adapter_name).ok()?;
        let device = adapter.device(address).ok()?;

        let properties = match device.all_properties().await {
            Ok(properties) => properties,
            Err(error) => {
                self.fail(format!("Failed to query device {address}: {error}"));
                return None;
            }
        };

        let now = Utc::now();
        let address_key = address.to_string();
        let is_first = !self.devices.contains_key(&address_key);
        let record = self
            .devices
            .entry(address_key.clone())
            .or_insert_with(|| DeviceRecord::new(address_key.clone(), adapter_name.clone(), now));

        record.adapter_name = adapter_name;

        let mut current_rssi = None;
        for property in properties {
            match property {
                DeviceProperty::Name(value) => record.name = Some(value),
                DeviceProperty::Alias(value) => record.alias = Some(value),
                DeviceProperty::AddressType(value) => record.address_type = Some(value.to_string()),
                DeviceProperty::Icon(value) => record.icon = Some(value),
                DeviceProperty::Class(value) => record.class = Some(value),
                DeviceProperty::Appearance(value) => record.appearance = Some(value),
                DeviceProperty::Modalias(value) => {
                    record.modalias = Some(format!(
                        "{}:{:04x}:{:04x}:{:04x}",
                        value.source, value.vendor, value.product, value.device
                    ))
                }
                DeviceProperty::Rssi(value) => current_rssi = Some(value),
                DeviceProperty::TxPower(value) => record.tx_power = Some(value),
                DeviceProperty::BatteryPercentage(value) => record.battery_percentage = Some(value),
                DeviceProperty::Paired(value) => record.paired = value,
                DeviceProperty::Trusted(value) => record.trusted = value,
                DeviceProperty::Connected(value) => record.connected = value,
                DeviceProperty::Blocked(value) => record.blocked = value,
                DeviceProperty::ServicesResolved(value) => record.services_resolved = value,
                DeviceProperty::LegacyPairing(value) => record.legacy_pairing = value,
                DeviceProperty::WakeAllowed(value) => record.wake_allowed = value,
                DeviceProperty::Uuids(values) => {
                    let mut uuids = values
                        .into_iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>();
                    uuids.sort();
                    record.uuids = uuids;
                }
                DeviceProperty::ManufacturerData(values) => {
                    let mut items = values
                        .into_iter()
                        .map(|(id, bytes)| ManufacturerEntry {
                            id,
                            payload_hex: bytes_to_hex(&bytes),
                        })
                        .collect::<Vec<_>>();
                    items.sort_by_key(|entry| entry.id);
                    record.manufacturer_data = items;
                }
                DeviceProperty::ServiceData(values) => {
                    let mut items = values
                        .into_iter()
                        .map(|(uuid, bytes)| ServiceDataEntry {
                            uuid: uuid.to_string(),
                            payload_hex: bytes_to_hex(&bytes),
                        })
                        .collect::<Vec<_>>();
                    items.sort_by(|left, right| left.uuid.cmp(&right.uuid));
                    record.service_data = items;
                }
                _ => {}
            }
        }

        record.note_advertisement(now, current_rssi);

        let is_public = record.is_public_address();

        // Collect values we need before releasing the borrow on self.devices
        let log_msg = if is_first {
            let addr_kind = if is_public { "public" } else { "random" };
            Some(format!(
                "Discovered {} [{}] ({})",
                record.display_name(),
                record.address,
                addr_kind,
            ))
        } else {
            None
        };

        // Only persist public-address devices to DB — random MACs are ephemeral
        // and would just bloat the sightings table.
        let should_upsert = is_public
            && (is_first
                || self
                    .upsert_throttle
                    .get(&address_key)
                    .is_none_or(|last| {
                        now.signed_duration_since(*last) >= chrono::Duration::seconds(5)
                    }));
        if should_upsert {
            if let Some(store) = &self.store {
                let _ = store.upsert_device(record);
            }
            self.upsert_throttle.insert(address_key, now);
        }
        let result = (record.clone(), is_first && is_public);

        if let Some(msg) = log_msg {
            self.log(LogLevel::Info, msg);
        }

        Some(result)
    }

    async fn read_adapter_status(&self, adapter: &Adapter) -> AdapterStatus {
        AdapterStatus {
            name: adapter.name().to_string(),
            alias: adapter
                .alias()
                .await
                .unwrap_or_else(|_| adapter.name().to_string()),
            address: adapter.address().await.ok().map(|value| value.to_string()),
            address_type: adapter
                .address_type()
                .await
                .ok()
                .map(|value| value.to_string()),
            powered: adapter.is_powered().await.unwrap_or(false),
            discovering: adapter.is_discovering().await.unwrap_or(false),
            discoverable: adapter.is_discoverable().await.unwrap_or(false),
            pairable: adapter.is_pairable().await.unwrap_or(false),
        }
    }

    fn mark_stale_devices(&mut self) {
        let stale_after = chrono::Duration::seconds(self.settings.stale_after_seconds as i64);
        let purge_after = chrono::Duration::seconds(self.settings.stale_after_seconds as i64 * 2);
        apply_stale_policy(&mut self.devices, stale_after, purge_after, Utc::now());
    }

    fn emit_snapshot(&mut self) {
        let live_devices = self.devices.values().filter(|d| !d.stale).count();
        let stale_devices = self.devices.len().saturating_sub(live_devices);
        let public_devices = self.devices.values().filter(|d| d.is_public_address()).count();
        let random_devices = self.devices.len().saturating_sub(public_devices);
        self.metrics.total_devices = self.devices.len();
        self.metrics.live_devices = live_devices;
        self.metrics.stale_devices = stale_devices;
        self.metrics.public_devices = public_devices;
        self.metrics.random_devices = random_devices;

        let mut devices = self.devices.values().cloned().collect::<Vec<_>>();
        devices.sort_by(|left, right| {
            right
                .stability_score()
                .cmp(&left.stability_score())
                .then_with(|| right.last_seen.cmp(&left.last_seen))
                .then_with(|| compare_option_i16(right.rssi, left.rssi))
                .then_with(|| left.address.cmp(&right.address))
        });

        let snapshot = AppSnapshot {
            generated_at: Utc::now(),
            active_adapter: self.active_adapter.clone(),
            scan_active: self.scan_active,
            status_line: self.status_line.clone(),
            last_error: self.last_error.clone(),
            adapters: self.adapters.clone(),
            devices,
            metrics: self.metrics.clone(),
            event_log: self.event_log.iter().cloned().collect(),
        };

        self.snapshot_tx.send_replace(snapshot);
        self.repaint_ctx.request_repaint();
    }

    fn select_adapter_name(&self) -> Option<String> {
        if let Some(selected) = &self.settings.selected_adapter
            && self
                .adapters
                .iter()
                .any(|adapter| &adapter.name == selected)
        {
            return Some(selected.clone());
        }
        self.adapters.first().map(|adapter| adapter.name.clone())
    }

    fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        let entry = EventLogEntry {
            at: Utc::now(),
            level,
            message: message.into(),
        };
        self.event_log.push_front(entry);
        while self.event_log.len() > 100 {
            self.event_log.pop_back();
        }
    }

    fn fail(&mut self, message: String) {
        self.last_error = Some(message.clone());
        self.metrics.bluez_errors = self.metrics.bluez_errors.saturating_add(1);
        self.log(LogLevel::Error, message);
    }
}

/// Applies the stale/purge policy to a device map.
///
/// Devices older than `stale_after` are marked stale.
/// Non-public devices older than `purge_after` are removed entirely.
/// Public-address devices are always retained.
pub(crate) fn apply_stale_policy(
    devices: &mut HashMap<String, DeviceRecord>,
    stale_after: chrono::Duration,
    purge_after: chrono::Duration,
    now: chrono::DateTime<Utc>,
) {
    devices.retain(|_, device| {
        let age = now.signed_duration_since(device.last_seen);
        if age > stale_after {
            device.mark_stale();
        }
        device.is_public_address() || age <= purge_after
    });
}

fn should_persist_sighting(
    previous: Option<&chrono::DateTime<Utc>>,
    now: chrono::DateTime<Utc>,
) -> bool {
    match previous {
        Some(previous) => now.signed_duration_since(*previous) >= chrono::Duration::seconds(10),
        None => true,
    }
}

fn transport_label(transport: DiscoveryTransport) -> &'static str {
    match transport {
        DiscoveryTransport::Auto => "auto",
        DiscoveryTransport::BrEdr => "BR/EDR",
        DiscoveryTransport::Le => "LE",
        _ => "auto",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── should_persist_sighting ─────────────────────────────────────

    #[test]
    fn persist_sighting_no_previous() {
        let now = Utc::now();
        assert!(should_persist_sighting(None, now));
    }

    #[test]
    fn persist_sighting_within_throttle() {
        let now = Utc::now();
        let prev = now - chrono::Duration::seconds(5);
        assert!(!should_persist_sighting(Some(&prev), now));
    }

    #[test]
    fn persist_sighting_exactly_at_threshold() {
        let now = Utc::now();
        let prev = now - chrono::Duration::seconds(10);
        assert!(should_persist_sighting(Some(&prev), now));
    }

    #[test]
    fn persist_sighting_past_threshold() {
        let now = Utc::now();
        let prev = now - chrono::Duration::seconds(15);
        assert!(should_persist_sighting(Some(&prev), now));
    }

    // ── transport_label ─────────────────────────────────────────────

    #[test]
    fn transport_label_values() {
        assert_eq!(transport_label(DiscoveryTransport::Auto), "auto");
        assert_eq!(transport_label(DiscoveryTransport::BrEdr), "BR/EDR");
        assert_eq!(transport_label(DiscoveryTransport::Le), "LE");
    }

    // ── apply_stale_policy ──────────────────────────────────────────

    #[test]
    fn stale_policy_marks_old_devices_stale() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();
        let mut d = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", now - chrono::Duration::seconds(35));
        d.address_type = Some("public".into());
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert!(devices.get("AA:BB:CC:DD:EE:FF").unwrap().stale);
    }

    #[test]
    fn stale_policy_keeps_recent_devices_live() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();
        let d = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", now - chrono::Duration::seconds(10));
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert!(!devices.get("AA:BB:CC:DD:EE:FF").unwrap().stale);
    }

    #[test]
    fn stale_policy_purges_old_random_devices() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();
        let mut d = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", now - chrono::Duration::seconds(65));
        d.address_type = Some("random".into());
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert!(devices.is_empty(), "random device past purge_after should be removed");
    }

    #[test]
    fn stale_policy_keeps_old_public_devices() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();
        let mut d = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", now - chrono::Duration::seconds(65));
        d.address_type = Some("public".into());
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert_eq!(devices.len(), 1, "public device should be kept");
        assert!(devices.get("AA:BB:CC:DD:EE:FF").unwrap().stale);
    }

    #[test]
    fn stale_policy_mixed_devices() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();

        // Recent device (no address_type) — kept, not stale
        let d1 = DeviceRecord::new("AA:00:00:00:00:01", "hci0", now - chrono::Duration::seconds(10));
        devices.insert(d1.address.clone(), d1);

        // Old random device — purged
        let mut d2 = DeviceRecord::new("AA:00:00:00:00:02", "hci0", now - chrono::Duration::seconds(65));
        d2.address_type = Some("random".into());
        devices.insert(d2.address.clone(), d2);

        // Old public device — kept but stale
        let mut d3 = DeviceRecord::new("AA:00:00:00:00:03", "hci0", now - chrono::Duration::seconds(65));
        d3.address_type = Some("public".into());
        devices.insert(d3.address.clone(), d3);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert_eq!(devices.len(), 2);
        assert!(devices.contains_key("AA:00:00:00:00:01"));
        assert!(!devices.contains_key("AA:00:00:00:00:02"));
        assert!(devices.contains_key("AA:00:00:00:00:03"));
        assert!(!devices.get("AA:00:00:00:00:01").unwrap().stale);
        assert!(devices.get("AA:00:00:00:00:03").unwrap().stale);
    }

    #[test]
    fn stale_policy_at_exact_boundary_not_stale() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();
        let d = DeviceRecord::new("AA:BB:CC:DD:EE:FF", "hci0", now - chrono::Duration::seconds(30));
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert!(!devices.get("AA:BB:CC:DD:EE:FF").unwrap().stale);
    }
}
