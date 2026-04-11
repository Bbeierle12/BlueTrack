use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use bluer::{
    Adapter, AdapterEvent, Address, DeviceProperty, DiscoveryFilter, DiscoveryTransport, Session,
};
use chrono::Utc;
use eframe::egui;
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
    intelligence,
    model::{
        AdapterStatus, AppSnapshot, DeviceRecord, EventLogEntry, LogLevel, ManufacturerEntry,
        RuntimeMetrics, ServiceDataEntry, bytes_to_hex,
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

    pub fn connect_device(&self, address: String) {
        let _ = self.commands.send(WorkerCommand::ConnectDevice(address));
    }

    pub fn disconnect_device(&self, address: String) {
        let _ = self.commands.send(WorkerCommand::DisconnectDevice(address));
    }

    pub fn set_trusted(&self, address: String, trusted: bool) {
        let _ = self
            .commands
            .send(WorkerCommand::SetTrusted(address, trusted));
    }

    pub fn set_blocked(&self, address: String, blocked: bool) {
        let _ = self
            .commands
            .send(WorkerCommand::SetBlocked(address, blocked));
    }

    pub fn forget_device(&self, address: String) {
        let _ = self.commands.send(WorkerCommand::ForgetDevice(address));
    }

    pub fn set_alias(&self, address: String, alias: String) {
        let _ = self.commands.send(WorkerCommand::SetAlias(address, alias));
    }

    pub fn set_adapter_powered(&self, adapter: String, powered: bool) {
        let _ = self
            .commands
            .send(WorkerCommand::SetAdapterPowered(adapter, powered));
    }

    pub fn set_adapter_discoverable(&self, adapter: String, discoverable: bool) {
        let _ = self
            .commands
            .send(WorkerCommand::SetAdapterDiscoverable(adapter, discoverable));
    }

    pub fn set_adapter_pairable(&self, adapter: String, pairable: bool) {
        let _ = self
            .commands
            .send(WorkerCommand::SetAdapterPairable(adapter, pairable));
    }
}

enum WorkerCommand {
    StartScan,
    StopScan,
    Refresh,
    ApplySettings(Settings),
    // Device management
    ConnectDevice(String),
    DisconnectDevice(String),
    SetTrusted(String, bool),
    SetBlocked(String, bool),
    ForgetDevice(String),
    SetAlias(String, String),
    // Adapter management
    SetAdapterPowered(String, bool),
    SetAdapterDiscoverable(String, bool),
    SetAdapterPairable(String, bool),
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
                    intelligence::enrich(&mut device);
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
                    if self.tick_count.is_multiple_of(60)
                        && let Some(store) = &self.store
                    {
                        let _ = store.prune_old_sightings(self.settings.retention_days);
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
                        WorkerCommand::ConnectDevice(addr) => self.cmd_connect(&addr).await,
                        WorkerCommand::DisconnectDevice(addr) => self.cmd_disconnect(&addr).await,
                        WorkerCommand::SetTrusted(addr, v) => self.cmd_set_trusted(&addr, v).await,
                        WorkerCommand::SetBlocked(addr, v) => self.cmd_set_blocked(&addr, v).await,
                        WorkerCommand::ForgetDevice(addr) => self.cmd_forget(&addr).await,
                        WorkerCommand::SetAlias(addr, alias) => self.cmd_set_alias(&addr, &alias).await,
                        WorkerCommand::SetAdapterPowered(name, v) => self.cmd_adapter_powered(&name, v).await,
                        WorkerCommand::SetAdapterDiscoverable(name, v) => self.cmd_adapter_discoverable(&name, v).await,
                        WorkerCommand::SetAdapterPairable(name, v) => self.cmd_adapter_pairable(&name, v).await,
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
                    if should_persist_sighting(self.sighting_throttle.get(&device.address), now) {
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
                            company_name: None,
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
        intelligence::enrich(record);

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
                || self.upsert_throttle.get(&address_key).is_none_or(|last| {
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
        let public_devices = self
            .devices
            .values()
            .filter(|d| d.is_public_address())
            .count();
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
                .then_with(|| right.first_seen.cmp(&left.first_seen))
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

    // ── Device management handlers ───────────────────────────────────────────

    async fn cmd_connect(&mut self, address: &str) {
        let result = match self.get_device(address).await {
            Ok(device) => device.connect().await.map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        if self.apply_connect_result(address, result) {
            self.sync_device_props(address).await;
        }
        self.emit_snapshot();
    }

    async fn cmd_disconnect(&mut self, address: &str) {
        let result = match self.get_device(address).await {
            Ok(device) => device.disconnect().await.map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        if self.apply_disconnect_result(address, result) {
            self.sync_device_props(address).await;
        }
        self.emit_snapshot();
    }

    async fn cmd_set_trusted(&mut self, address: &str, trusted: bool) {
        let result = match self.get_device(address).await {
            Ok(device) => device.set_trusted(trusted).await.map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        self.apply_set_trusted(address, trusted, result);
        self.emit_snapshot();
    }

    async fn cmd_set_blocked(&mut self, address: &str, blocked: bool) {
        let result = match self.get_device(address).await {
            Ok(device) => device.set_blocked(blocked).await.map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        self.apply_set_blocked(address, blocked, result);
        self.emit_snapshot();
    }

    async fn cmd_forget(&mut self, address: &str) {
        let result = self.do_remove_device(address).await;
        self.apply_forget_result(address, result);
        self.emit_snapshot();
    }

    async fn cmd_set_alias(&mut self, address: &str, alias: &str) {
        let result = match self.get_device(address).await {
            Ok(device) => device
                .set_alias(alias.to_string())
                .await
                .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        self.apply_set_alias(address, alias, result);
        self.emit_snapshot();
    }

    // ── Adapter management handlers ──────────────────────────────────────────

    async fn cmd_adapter_powered(&mut self, adapter_name: &str, powered: bool) {
        let result = self.do_set_adapter_powered(adapter_name, powered).await;
        if self.apply_adapter_powered(adapter_name, powered, result) {
            self.refresh_adapters().await;
        }
        self.emit_snapshot();
    }

    async fn cmd_adapter_discoverable(&mut self, adapter_name: &str, discoverable: bool) {
        let result = self
            .do_set_adapter_discoverable(adapter_name, discoverable)
            .await;
        if self.apply_adapter_discoverable(adapter_name, discoverable, result) {
            self.refresh_adapters().await;
        }
        self.emit_snapshot();
    }

    async fn cmd_adapter_pairable(&mut self, adapter_name: &str, pairable: bool) {
        let result = self.do_set_adapter_pairable(adapter_name, pairable).await;
        if self.apply_adapter_pairable(adapter_name, pairable, result) {
            self.refresh_adapters().await;
        }
        self.emit_snapshot();
    }

    // ── Pure result-application helpers (testable without BlueZ) ────────────

    /// Returns `true` if the caller should follow up with `sync_device_props`.
    fn apply_connect_result(&mut self, address: &str, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                self.log(LogLevel::Info, format!("Connected to {address}"));
                true
            }
            Err(e) => {
                self.fail(format!("Connect {address} failed: {e}"));
                false
            }
        }
    }

    /// Returns `true` if the caller should follow up with `sync_device_props`.
    fn apply_disconnect_result(&mut self, address: &str, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                self.log(LogLevel::Info, format!("Disconnected from {address}"));
                true
            }
            Err(e) => {
                self.fail(format!("Disconnect {address} failed: {e}"));
                false
            }
        }
    }

    fn apply_set_trusted(&mut self, address: &str, trusted: bool, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let verb = if trusted { "Trusted" } else { "Untrusted" };
                self.log(LogLevel::Info, format!("{verb} {address}"));
                if let Some(record) = self.devices.get_mut(address) {
                    record.trusted = trusted;
                }
            }
            Err(e) => self.fail(format!("Set trusted {address} failed: {e}")),
        }
    }

    fn apply_set_blocked(&mut self, address: &str, blocked: bool, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let verb = if blocked { "Blocked" } else { "Unblocked" };
                self.log(LogLevel::Info, format!("{verb} {address}"));
                if let Some(record) = self.devices.get_mut(address) {
                    record.blocked = blocked;
                }
            }
            Err(e) => self.fail(format!("Set blocked {address} failed: {e}")),
        }
    }

    fn apply_set_alias(&mut self, address: &str, alias: &str, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.log(LogLevel::Info, format!("Renamed {address} → \"{alias}\""));
                if let Some(record) = self.devices.get_mut(address) {
                    record.alias = if alias.is_empty() {
                        None
                    } else {
                        Some(alias.to_string())
                    };
                    if let Some(store) = &self.store {
                        let _ = store.upsert_device(record);
                    }
                }
            }
            Err(e) => self.fail(format!("Set alias {address} failed: {e}")),
        }
    }

    fn apply_forget_result(&mut self, address: &str, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.devices.remove(address);
                self.sighting_throttle.remove(address);
                self.upsert_throttle.remove(address);
                self.log(LogLevel::Info, format!("Forgot device {address}"));
            }
            Err(e) => self.fail(e),
        }
    }

    /// Returns `true` if the caller should follow up with `refresh_adapters`.
    fn apply_adapter_powered(
        &mut self,
        adapter_name: &str,
        powered: bool,
        result: Result<(), String>,
    ) -> bool {
        match result {
            Ok(()) => {
                let verb = if powered { "Powered on" } else { "Powered off" };
                self.log(LogLevel::Info, format!("{verb} {adapter_name}"));
                true
            }
            Err(e) => {
                self.fail(e);
                false
            }
        }
    }

    /// Returns `true` if the caller should follow up with `refresh_adapters`.
    fn apply_adapter_discoverable(
        &mut self,
        adapter_name: &str,
        discoverable: bool,
        result: Result<(), String>,
    ) -> bool {
        match result {
            Ok(()) => {
                let verb = if discoverable {
                    "Discoverable on"
                } else {
                    "Discoverable off"
                };
                self.log(LogLevel::Info, format!("{verb} {adapter_name}"));
                true
            }
            Err(e) => {
                self.fail(e);
                false
            }
        }
    }

    /// Returns `true` if the caller should follow up with `refresh_adapters`.
    fn apply_adapter_pairable(
        &mut self,
        adapter_name: &str,
        pairable: bool,
        result: Result<(), String>,
    ) -> bool {
        match result {
            Ok(()) => {
                let verb = if pairable {
                    "Pairable on"
                } else {
                    "Pairable off"
                };
                self.log(LogLevel::Info, format!("{verb} {adapter_name}"));
                true
            }
            Err(e) => {
                self.fail(e);
                false
            }
        }
    }

    // ── BlueZ operation helpers (live-only, not unit-testable) ───────────────

    async fn do_remove_device(&self, address: &str) -> Result<(), String> {
        let session = self.session.as_ref().ok_or("No BlueZ session")?;
        let adapter_name = self
            .active_adapter
            .clone()
            .or_else(|| self.select_adapter_name())
            .unwrap_or_else(|| "hci0".to_string());
        let adapter = session
            .adapter(&adapter_name)
            .map_err(|e| format!("Cannot open adapter for forget: {e}"))?;
        let addr = address
            .parse::<bluer::Address>()
            .map_err(|e| format!("Invalid address '{address}': {e}"))?;
        adapter
            .remove_device(addr)
            .await
            .map_err(|e| format!("Forget {address} failed: {e}"))
    }

    async fn do_set_adapter_powered(
        &self,
        adapter_name: &str,
        powered: bool,
    ) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or("No BlueZ session".to_string())?;
        let adapter = session
            .adapter(adapter_name)
            .map_err(|e| format!("Cannot open adapter {adapter_name}: {e}"))?;
        adapter
            .set_powered(powered)
            .await
            .map_err(|e| format!("Set powered {adapter_name} failed: {e}"))
    }

    async fn do_set_adapter_discoverable(
        &self,
        adapter_name: &str,
        discoverable: bool,
    ) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or("No BlueZ session".to_string())?;
        let adapter = session
            .adapter(adapter_name)
            .map_err(|e| format!("Cannot open adapter {adapter_name}: {e}"))?;
        adapter
            .set_discoverable(discoverable)
            .await
            .map_err(|e| format!("Set discoverable {adapter_name} failed: {e}"))
    }

    async fn do_set_adapter_pairable(
        &self,
        adapter_name: &str,
        pairable: bool,
    ) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or("No BlueZ session".to_string())?;
        let adapter = session
            .adapter(adapter_name)
            .map_err(|e| format!("Cannot open adapter {adapter_name}: {e}"))?;
        adapter
            .set_pairable(pairable)
            .await
            .map_err(|e| format!("Set pairable {adapter_name} failed: {e}"))
    }

    // ── Shared helpers ───────────────────────────────────────────────────────

    /// Get a bluer Device handle for the given address string.
    async fn get_device(&self, address: &str) -> Result<bluer::Device, String> {
        let session = self.session.as_ref().ok_or("No BlueZ session")?;
        let adapter_name = self
            .active_adapter
            .clone()
            .or_else(|| self.select_adapter_name())
            .unwrap_or_else(|| "hci0".to_string());
        let adapter = session
            .adapter(&adapter_name)
            .map_err(|e| format!("Cannot open adapter: {e}"))?;
        let addr = address
            .parse::<bluer::Address>()
            .map_err(|e| format!("Invalid address '{address}': {e}"))?;
        adapter
            .device(addr)
            .map_err(|e| format!("Device {address} not found: {e}"))
    }

    /// Re-read device properties from BlueZ and update our in-memory record.
    async fn sync_device_props(&mut self, address: &str) {
        if let Ok(addr) = address.parse::<bluer::Address>() {
            let _ = self.refresh_device(addr).await;
        }
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
        let mut d = DeviceRecord::new(
            "AA:BB:CC:DD:EE:FF",
            "hci0",
            now - chrono::Duration::seconds(35),
        );
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
        let d = DeviceRecord::new(
            "AA:BB:CC:DD:EE:FF",
            "hci0",
            now - chrono::Duration::seconds(10),
        );
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
        let mut d = DeviceRecord::new(
            "AA:BB:CC:DD:EE:FF",
            "hci0",
            now - chrono::Duration::seconds(65),
        );
        d.address_type = Some("random".into());
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert!(
            devices.is_empty(),
            "random device past purge_after should be removed"
        );
    }

    #[test]
    fn stale_policy_keeps_old_public_devices() {
        let now = Utc::now();
        let stale_after = chrono::Duration::seconds(30);
        let purge_after = chrono::Duration::seconds(60);

        let mut devices = HashMap::new();
        let mut d = DeviceRecord::new(
            "AA:BB:CC:DD:EE:FF",
            "hci0",
            now - chrono::Duration::seconds(65),
        );
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
        let d1 = DeviceRecord::new(
            "AA:00:00:00:00:01",
            "hci0",
            now - chrono::Duration::seconds(10),
        );
        devices.insert(d1.address.clone(), d1);

        // Old random device — purged
        let mut d2 = DeviceRecord::new(
            "AA:00:00:00:00:02",
            "hci0",
            now - chrono::Duration::seconds(65),
        );
        d2.address_type = Some("random".into());
        devices.insert(d2.address.clone(), d2);

        // Old public device — kept but stale
        let mut d3 = DeviceRecord::new(
            "AA:00:00:00:00:03",
            "hci0",
            now - chrono::Duration::seconds(65),
        );
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
        let d = DeviceRecord::new(
            "AA:BB:CC:DD:EE:FF",
            "hci0",
            now - chrono::Duration::seconds(30),
        );
        devices.insert(d.address.clone(), d);

        apply_stale_policy(&mut devices, stale_after, purge_after, now);

        assert!(!devices.get("AA:BB:CC:DD:EE:FF").unwrap().stale);
    }

    // ── Management apply_* helpers ──────────────────────────────────────────

    /// Build a Worker with no BlueZ session, suitable for testing apply_* helpers.
    fn make_test_worker() -> Worker {
        use tokio::sync::{mpsc::unbounded_channel as ub, watch};
        let (_, cmd_rx) = ub::<WorkerCommand>();
        let (snap_tx, _) = watch::channel(crate::model::AppSnapshot::default());
        let (int_tx, int_rx) = ub::<InternalEvent>();
        Worker {
            settings: crate::settings::Settings::default(),
            commands: cmd_rx,
            snapshot_tx: snap_tx,
            repaint_ctx: egui::Context::default(),
            internal_tx: int_tx,
            internal_rx: int_rx,
            store: None,
            session: None,
            devices: HashMap::new(),
            adapters: Vec::new(),
            scan_task: None,
            active_adapter: None,
            metrics: crate::model::RuntimeMetrics::default(),
            event_log: VecDeque::with_capacity(16),
            last_error: None,
            scan_active: false,
            status_line: "test".to_string(),
            sighting_throttle: HashMap::new(),
            upsert_throttle: HashMap::new(),
            last_snapshot_at: chrono::DateTime::<Utc>::MIN_UTC,
            tick_count: 0,
        }
    }

    #[test]
    fn apply_connect_success_logs_connected() {
        let mut w = make_test_worker();
        let should_sync = w.apply_connect_result("AA:BB:CC:DD:EE:FF", Ok(()));
        assert!(should_sync);
        assert!(w.event_log.front().unwrap().message.contains("Connected"));
        assert!(w.last_error.is_none());
    }

    #[test]
    fn apply_connect_error_logs_failure() {
        let mut w = make_test_worker();
        let should_sync = w.apply_connect_result("AA:BB:CC:DD:EE:FF", Err("refused".into()));
        assert!(!should_sync);
        assert!(w.last_error.is_some());
    }

    #[test]
    fn apply_disconnect_success_logs_disconnected() {
        let mut w = make_test_worker();
        let should_sync = w.apply_disconnect_result("AA:BB:CC:DD:EE:FF", Ok(()));
        assert!(should_sync);
        assert!(
            w.event_log
                .front()
                .unwrap()
                .message
                .contains("Disconnected")
        );
    }

    #[test]
    fn apply_disconnect_error_logs_failure() {
        let mut w = make_test_worker();
        let should_sync = w.apply_disconnect_result("AA:BB:CC:DD:EE:FF", Err("timeout".into()));
        assert!(!should_sync);
        assert!(w.last_error.is_some());
    }

    #[test]
    fn apply_set_trusted_true_updates_record() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        let mut d = DeviceRecord::new(addr, "hci0", Utc::now());
        d.trusted = false;
        w.devices.insert(addr.to_string(), d);
        w.apply_set_trusted(addr, true, Ok(()));
        assert!(w.devices[addr].trusted);
        assert!(w.event_log.front().unwrap().message.contains("Trusted"));
        assert!(w.last_error.is_none());
    }

    #[test]
    fn apply_set_trusted_false_updates_record() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        let mut d = DeviceRecord::new(addr, "hci0", Utc::now());
        d.trusted = true;
        w.devices.insert(addr.to_string(), d);
        w.apply_set_trusted(addr, false, Ok(()));
        assert!(!w.devices[addr].trusted);
        assert!(w.event_log.front().unwrap().message.contains("Untrusted"));
    }

    #[test]
    fn apply_set_trusted_error_logs_failure() {
        let mut w = make_test_worker();
        w.apply_set_trusted("AA:BB:CC:DD:EE:FF", true, Err("D-Bus error".into()));
        assert!(w.last_error.is_some());
        assert!(w.last_error.as_ref().unwrap().contains("D-Bus error"));
    }

    #[test]
    fn apply_set_blocked_true_updates_record() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        let mut d = DeviceRecord::new(addr, "hci0", Utc::now());
        d.blocked = false;
        w.devices.insert(addr.to_string(), d);
        w.apply_set_blocked(addr, true, Ok(()));
        assert!(w.devices[addr].blocked);
        assert!(w.event_log.front().unwrap().message.contains("Blocked"));
    }

    #[test]
    fn apply_set_blocked_false_updates_record() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        let mut d = DeviceRecord::new(addr, "hci0", Utc::now());
        d.blocked = true;
        w.devices.insert(addr.to_string(), d);
        w.apply_set_blocked(addr, false, Ok(()));
        assert!(!w.devices[addr].blocked);
        assert!(w.event_log.front().unwrap().message.contains("Unblocked"));
    }

    #[test]
    fn apply_set_alias_sets_alias_on_device() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        w.devices.insert(
            addr.to_string(),
            DeviceRecord::new(addr, "hci0", Utc::now()),
        );
        w.apply_set_alias(addr, "My Headphones", Ok(()));
        assert_eq!(w.devices[addr].alias.as_deref(), Some("My Headphones"));
        assert!(w.event_log.front().unwrap().message.contains("Renamed"));
    }

    #[test]
    fn apply_set_alias_empty_clears_alias() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        let mut d = DeviceRecord::new(addr, "hci0", Utc::now());
        d.alias = Some("Old Name".to_string());
        w.devices.insert(addr.to_string(), d);
        w.apply_set_alias(addr, "", Ok(()));
        assert!(w.devices[addr].alias.is_none());
    }

    #[test]
    fn apply_set_alias_error_logs_failure() {
        let mut w = make_test_worker();
        w.apply_set_alias("AA:BB:CC:DD:EE:FF", "name", Err("not found".into()));
        assert!(w.last_error.is_some());
    }

    #[test]
    fn apply_forget_result_removes_device_and_throttles() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        w.devices.insert(
            addr.to_string(),
            DeviceRecord::new(addr, "hci0", Utc::now()),
        );
        w.sighting_throttle.insert(addr.to_string(), Utc::now());
        w.upsert_throttle.insert(addr.to_string(), Utc::now());
        w.apply_forget_result(addr, Ok(()));
        assert!(!w.devices.contains_key(addr));
        assert!(!w.sighting_throttle.contains_key(addr));
        assert!(!w.upsert_throttle.contains_key(addr));
        assert!(w.event_log.front().unwrap().message.contains("Forgot"));
    }

    #[test]
    fn apply_forget_result_error_preserves_device() {
        let mut w = make_test_worker();
        let addr = "AA:BB:CC:DD:EE:FF";
        w.devices.insert(
            addr.to_string(),
            DeviceRecord::new(addr, "hci0", Utc::now()),
        );
        w.apply_forget_result(addr, Err("adapter gone".into()));
        assert!(
            w.devices.contains_key(addr),
            "device must not be removed on error"
        );
        assert!(w.last_error.is_some());
    }

    #[test]
    fn apply_adapter_powered_on_logs_and_returns_true() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_powered("hci0", true, Ok(()));
        assert!(refresh);
        assert!(w.event_log.front().unwrap().message.contains("Powered on"));
    }

    #[test]
    fn apply_adapter_powered_off_logs_and_returns_true() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_powered("hci0", false, Ok(()));
        assert!(refresh);
        assert!(w.event_log.front().unwrap().message.contains("Powered off"));
    }

    #[test]
    fn apply_adapter_powered_error_returns_false() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_powered("hci0", true, Err("D-Bus failed".into()));
        assert!(!refresh);
        assert!(w.last_error.is_some());
    }

    #[test]
    fn apply_adapter_discoverable_on_logs() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_discoverable("hci0", true, Ok(()));
        assert!(refresh);
        assert!(
            w.event_log
                .front()
                .unwrap()
                .message
                .contains("Discoverable on")
        );
    }

    #[test]
    fn apply_adapter_discoverable_off_logs() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_discoverable("hci0", false, Ok(()));
        assert!(refresh);
        assert!(
            w.event_log
                .front()
                .unwrap()
                .message
                .contains("Discoverable off")
        );
    }

    #[test]
    fn apply_adapter_pairable_on_logs() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_pairable("hci0", true, Ok(()));
        assert!(refresh);
        assert!(w.event_log.front().unwrap().message.contains("Pairable on"));
    }

    #[test]
    fn apply_adapter_pairable_off_logs() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_pairable("hci0", false, Ok(()));
        assert!(refresh);
        assert!(
            w.event_log
                .front()
                .unwrap()
                .message
                .contains("Pairable off")
        );
    }

    #[test]
    fn apply_adapter_pairable_error_returns_false() {
        let mut w = make_test_worker();
        let refresh = w.apply_adapter_pairable("hci0", true, Err("failed".into()));
        assert!(!refresh);
        assert!(w.last_error.is_some());
    }
}
