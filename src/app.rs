use chrono::{DateTime, Utc};
use eframe::egui::{self, FontFamily, FontId, TextStyle, Visuals};

use crate::{
    backend::bluetooth::ScannerHandle,
    model::{AppSnapshot, DeviceRecord},
    settings::Settings,
    ui::{
        self, AppAction, ScanFilter, ViewTab,
        settings_modal::{SettingsModal, SettingsModalResult},
    },
};

pub struct BluetoothApp {
    snapshot: AppSnapshot,
    // Frozen copy of the device list shown in the Scan tab. Only refreshed
    // when the user clicks the Scan button, so rows don't reorder while the
    // live passive scan keeps feeding the Proximity Radar.
    captured_devices: Vec<DeviceRecord>,
    captured_at: Option<DateTime<Utc>>,
    scanner: ScannerHandle,
    settings: Settings,
    selected_tab: ViewTab,
    scan_filter: ScanFilter,
    selected_device: Option<String>,
    settings_modal: Option<SettingsModal>,
    rename_draft: Option<String>,
}

impl BluetoothApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);
        let settings = Settings::load();
        let scanner = ScannerHandle::spawn(settings.clone(), cc.egui_ctx.clone());

        Self {
            snapshot: AppSnapshot::default(),
            captured_devices: Vec::new(),
            captured_at: None,
            scanner,
            settings,
            selected_tab: ViewTab::Scan,
            scan_filter: ScanFilter::All,
            selected_device: None,
            settings_modal: None,
            rename_draft: None,
        }
    }

    fn poll_snapshot(&mut self) {
        if self.scanner.snapshot.has_changed().unwrap_or(false) {
            let snapshot = self.scanner.snapshot.borrow_and_update().clone();
            if self.selected_device.is_none() {
                self.selected_device = snapshot
                    .devices
                    .first()
                    .map(|device| device.address.clone());
            }
            self.snapshot = snapshot;
        }
    }

    fn capture_scan(&mut self) {
        self.captured_devices = self.snapshot.devices.clone();
        self.captured_at = Some(Utc::now());
        if !self.snapshot.scan_active {
            self.scanner.start_scan();
        }
    }
}

impl eframe::App for BluetoothApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_snapshot();
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        if let Some(action) = ui::render(
            ctx,
            &self.snapshot,
            &self.captured_devices,
            self.captured_at,
            &mut self.selected_tab,
            &mut self.scan_filter,
            &mut self.selected_device,
            &mut self.rename_draft,
        ) {
            match action {
                AppAction::StartScan => self.scanner.start_scan(),
                AppAction::StopScan => self.scanner.stop_scan(),
                AppAction::Refresh => self.scanner.refresh(),
                AppAction::CaptureScan => self.capture_scan(),
                AppAction::OpenSettings => {
                    self.settings_modal = Some(SettingsModal::new(&self.settings));
                }
                AppAction::ConnectDevice(addr) => self.scanner.connect_device(addr),
                AppAction::DisconnectDevice(addr) => self.scanner.disconnect_device(addr),
                AppAction::SetTrusted { address, trusted } => {
                    self.scanner.set_trusted(address, trusted)
                }
                AppAction::SetBlocked { address, blocked } => {
                    // If we're forgetting a blocked device, clear selection
                    self.scanner.set_blocked(address, blocked)
                }
                AppAction::ForgetDevice(addr) => {
                    if self.selected_device.as_deref() == Some(addr.as_str()) {
                        self.selected_device = None;
                    }
                    self.scanner.forget_device(addr);
                }
                AppAction::SetAlias { address, alias } => {
                    self.scanner.set_alias(address, alias);
                }
                AppAction::SetAdapterPowered { adapter, powered } => {
                    self.scanner.set_adapter_powered(adapter, powered);
                }
                AppAction::SetAdapterDiscoverable { adapter, discoverable } => {
                    self.scanner.set_adapter_discoverable(adapter, discoverable);
                }
                AppAction::SetAdapterPairable { adapter, pairable } => {
                    self.scanner.set_adapter_pairable(adapter, pairable);
                }
            }
        }

        if let Some(modal) = self.settings_modal.as_mut() {
            match modal.render(ctx, &self.snapshot.adapters) {
                SettingsModalResult::Open => {}
                SettingsModalResult::Saved(settings) => {
                    self.settings = settings.clone();
                    self.scanner.apply_settings(settings);
                    self.settings_modal = None;
                }
                SettingsModalResult::Cancelled => {
                    self.settings_modal = None;
                }
            }
        }
    }
}

fn configure_theme(ctx: &egui::Context) {
    use crate::ui::theme::{color, space, text};

    let mut style = (*ctx.style()).clone();
    style.visuals = Visuals::dark();

    // Surfaces
    style.visuals.window_fill = color::BG_WINDOW;
    style.visuals.panel_fill = color::BG_PANEL;
    style.visuals.extreme_bg_color = color::BG_EXTREME;
    style.visuals.faint_bg_color = color::BG_SURFACE_3;

    // Widget backgrounds
    style.visuals.widgets.noninteractive.bg_fill = color::BG_SURFACE_3;
    style.visuals.widgets.inactive.bg_fill = color::BG_SURFACE_2;
    style.visuals.widgets.hovered.bg_fill = color::BG_SURFACE_3;
    style.visuals.widgets.active.bg_fill = color::BG_SURFACE_3;

    // Borders
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, color::BORDER);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, color::BORDER);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, color::BORDER_MUTED);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, color::ACCENT);

    // Selection highlight
    style.visuals.selection.bg_fill = color::MID.gamma_multiply(0.18);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, color::MID);

    // Text on widgets
    style.visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, color::TEXT_PRIMARY);
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0, color::TEXT_SECONDARY);
    style.visuals.widgets.hovered.fg_stroke =
        egui::Stroke::new(1.0, color::TEXT_PRIMARY);
    style.visuals.widgets.active.fg_stroke =
        egui::Stroke::new(1.0, color::TEXT_PRIMARY);

    style.spacing.item_spacing = egui::vec2(space::MD, space::MD);
    style.spacing.button_padding = egui::vec2(space::LG, space::SM);

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(text::HEADING, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(text::BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(text::BUTTON, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(text::MONO, FontFamily::Monospace),
        ),
        (
            TextStyle::Small,
            FontId::new(text::SMALL, FontFamily::Proportional),
        ),
    ]
    .into();
    ctx.set_style(style);
}
