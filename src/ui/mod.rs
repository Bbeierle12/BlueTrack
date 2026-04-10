pub mod settings_modal;

use std::f32::consts::TAU;

use eframe::egui::{self, RichText};

use crate::model::{AppSnapshot, AddressTypeDetail, DeviceRecord, LogLevel, TrackerConfidence, format_duration_since, format_relative_time};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Scan,
    Map,
    Profiles,
    Adapters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFilter {
    Live,
    Stale,
    Public,
    Random,
    Trackers,
    Errors,
}

pub enum AppAction {
    // Scanner controls
    StartScan,
    StopScan,
    Refresh,
    CaptureScan,
    OpenSettings,
    // Device management
    ConnectDevice(String),
    DisconnectDevice(String),
    SetTrusted { address: String, trusted: bool },
    SetBlocked { address: String, blocked: bool },
    ForgetDevice(String),
    SetAlias { address: String, alias: String },
    // Adapter management
    SetAdapterPowered { adapter: String, powered: bool },
    SetAdapterDiscoverable { adapter: String, discoverable: bool },
    SetAdapterPairable { adapter: String, pairable: bool },
}

pub fn render(
    ctx: &egui::Context,
    snapshot: &AppSnapshot,
    captured_devices: &[DeviceRecord],
    captured_at: Option<chrono::DateTime<chrono::Utc>>,
    selected_tab: &mut ViewTab,
    scan_filter: &mut ScanFilter,
    selected_device: &mut Option<String>,
    rename_draft: &mut Option<String>,
) -> Option<AppAction> {
    let mut action = None;

    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("BlueTrack").size(24.0).strong());
            ui.label(
                RichText::new(snapshot.status_line.as_str()).color(if snapshot.scan_active {
                    egui::Color32::from_rgb(81, 196, 136)
                } else {
                    egui::Color32::from_rgb(220, 180, 92)
                }),
            );

            if let Some(active_adapter) = &snapshot.active_adapter {
                ui.label(RichText::new(format!("Adapter {active_adapter}")).italics());
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    action = Some(AppAction::OpenSettings);
                }
                if ui.button("Refresh").clicked() {
                    action = Some(AppAction::Refresh);
                }
                if snapshot.scan_active {
                    if ui.button("Stop Scan").clicked() {
                        action = Some(AppAction::StopScan);
                    }
                } else if ui.button("Start Scan").clicked() {
                    action = Some(AppAction::StartScan);
                }
            });
        });

        ui.add_space(8.0);

        // Top-level view tabs
        ui.horizontal(|ui| {
            ui.selectable_value(selected_tab, ViewTab::Scan, "Live Scan");
            ui.selectable_value(selected_tab, ViewTab::Map, "Proximity Radar");
            ui.selectable_value(selected_tab, ViewTab::Profiles, "Profiles");
            ui.selectable_value(selected_tab, ViewTab::Adapters, "Adapters");
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("Runtime: {}", format_duration_since(snapshot.metrics.started_at)))
                    .small()
                    .color(egui::Color32::from_rgb(140, 150, 160)),
            );
        });

        // Filter sub-tabs (only in Scan view). Counts reflect the captured
        // static list so they match what's rendered in the grid below.
        if *selected_tab == ViewTab::Scan {
            let live_n = captured_devices.iter().filter(|d| !d.stale).count();
            let stale_n = captured_devices.len().saturating_sub(live_n);
            let public_n = captured_devices.iter().filter(|d| d.is_public_address()).count();
            let random_n = captured_devices.len().saturating_sub(public_n);
            let tracker_n = captured_devices.iter().filter(|d| d.tracker_alert.is_some()).count();

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                filter_tab(ui, scan_filter, ScanFilter::Live, &format!("Live {live_n}"), COLOR_NEAR);
                filter_tab(ui, scan_filter, ScanFilter::Stale, &format!("Stale {stale_n}"), COLOR_STALE);
                filter_tab(ui, scan_filter, ScanFilter::Public, &format!("Public {public_n}"), COLOR_MID);
                filter_tab(ui, scan_filter, ScanFilter::Random, &format!("Random {random_n}"), COLOR_FAR);
                filter_tab(ui, scan_filter, ScanFilter::Trackers, &format!("Trackers {tracker_n}"), COLOR_TRACKER);
                filter_tab(
                    ui, scan_filter, ScanFilter::Errors,
                    &format!("Errors {}", snapshot.metrics.bluez_errors),
                    COLOR_ERROR,
                );
            });
        }
    });

    // Device detail panel only shown on Scan and Map views
    if *selected_tab != ViewTab::Adapters && *selected_tab != ViewTab::Profiles {
        if let Some(device_action) = egui::SidePanel::right("device_detail")
            .min_width(340.0)
            .show(ctx, |ui| {
                render_device_detail(ui, snapshot, selected_device, rename_draft)
            })
            .inner
        {
            action = Some(device_action);
        }
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        match selected_tab {
            ViewTab::Scan => {
                if let Some(a) = render_scan_view(
                    ui,
                    captured_devices,
                    captured_at,
                    snapshot,
                    scan_filter,
                    selected_device,
                ) {
                    action = Some(a);
                }
            }
            ViewTab::Map => render_map_view(ui, snapshot, selected_device),
            ViewTab::Profiles => render_profiles_view(ui, snapshot, selected_tab, selected_device),
            ViewTab::Adapters => {
                if let Some(a) = render_adapter_view(ui, snapshot) {
                    action = Some(a);
                }
            }
        }
    });

    action
}

fn filter_tab(
    ui: &mut egui::Ui,
    current: &mut ScanFilter,
    value: ScanFilter,
    label: &str,
    tint: egui::Color32,
) {
    let active = *current == value;
    let (fill, stroke) = if active {
        (tint.gamma_multiply(0.25), egui::Stroke::new(1.5, tint))
    } else {
        (
            tint.gamma_multiply(0.08),
            egui::Stroke::new(1.0, tint.gamma_multiply(0.3)),
        )
    };
    let text_color = if active {
        tint
    } else {
        tint.gamma_multiply(0.65)
    };

    let button = egui::Button::new(RichText::new(label).color(text_color).strong())
        .fill(fill)
        .stroke(stroke)
        .corner_radius(8.0);

    if ui.add(button).clicked() {
        *current = value;
    }
}

fn render_scan_view(
    ui: &mut egui::Ui,
    captured_devices: &[DeviceRecord],
    captured_at: Option<chrono::DateTime<chrono::Utc>>,
    snapshot: &AppSnapshot,
    scan_filter: &ScanFilter,
    selected_device: &mut Option<String>,
) -> Option<AppAction> {
    let mut action: Option<AppAction> = None;

    // Errors tab shows the event log instead of the device grid
    if *scan_filter == ScanFilter::Errors {
        render_event_log(ui, snapshot);
        return action;
    }

    // Toolbar: Scan button + capture status.
    ui.horizontal(|ui| {
        let scan_btn = egui::Button::new(RichText::new("Scan for Devices").strong())
            .fill(COLOR_NEAR.gamma_multiply(0.18))
            .stroke(egui::Stroke::new(1.5, COLOR_NEAR))
            .corner_radius(8.0);
        if ui.add(scan_btn).clicked() {
            action = Some(AppAction::CaptureScan);
        }

        let status = match captured_at {
            Some(ts) => format!(
                "Captured {} — {} devices",
                format_relative_time(ts),
                captured_devices.len()
            ),
            None => "No scan captured yet".to_string(),
        };
        ui.label(
            RichText::new(status)
                .small()
                .color(egui::Color32::from_rgb(140, 150, 160)),
        );

        if !snapshot.scan_active {
            ui.add_space(8.0);
            ui.label(
                RichText::new("(passive scan is stopped — use Start Scan above)")
                    .small()
                    .color(egui::Color32::from_rgb(209, 135, 61)),
            );
        }
    });
    ui.add_space(6.0);

    // Captured devices are frozen in whatever order they were in at capture
    // time. Only filtering is applied — no re-sorting, so rows don't move.
    let devices: Vec<&DeviceRecord> = captured_devices
        .iter()
        .filter(|d| match scan_filter {
            ScanFilter::Live => !d.stale,
            ScanFilter::Stale => d.stale,
            ScanFilter::Public => d.is_public_address(),
            ScanFilter::Random => !d.is_public_address(),
            ScanFilter::Trackers => d.tracker_alert.is_some(),
            ScanFilter::Errors => false,
        })
        .collect();

    if devices.is_empty() {
        ui.add_space(24.0);
        ui.label(
            RichText::new(if captured_devices.is_empty() {
                "No scan captured yet. Click \"Scan for Devices\" to capture nearby Bluetooth devices."
            } else {
                match scan_filter {
                    ScanFilter::Live => "No live devices in the captured list.",
                    ScanFilter::Stale => "No stale devices in the captured list.",
                    ScanFilter::Public => "No public-address devices in the captured list.",
                    ScanFilter::Random => "No random-address devices in the captured list.",
                    ScanFilter::Trackers => "No trackers in the captured list.",
                    ScanFilter::Errors => "",
                }
            })
            .color(egui::Color32::from_rgb(140, 150, 160)),
        );
        return action;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("device_grid")
            .num_columns(10)
            .spacing([14.0, 10.0])
            .striped(true)
            .show(ui, |ui| {
                for label in [
                    "State",
                    "Category",
                    "Name",
                    "Address",
                    "RSSI",
                    "Band",
                    "Stability",
                    "Recurrence",
                    "Seen",
                    "Traits",
                ] {
                    ui.strong(label);
                }
                ui.end_row();

                for device in &devices {
                    let selected = selected_device.as_deref() == Some(device.address.as_str());

                    // State column
                    let is_tracker = device.tracker_alert.is_some();
                    let (state_label, state_color) = if is_tracker {
                        ("TRACKER", COLOR_TRACKER)
                    } else if device.stale {
                        ("Stale", COLOR_STALE)
                    } else {
                        ("Live", COLOR_NEAR)
                    };
                    ui.colored_label(state_color, state_label);

                    // Category column
                    let cat_color = category_color(&device.category);
                    ui.colored_label(cat_color, device.category.label());

                    if ui
                        .selectable_label(selected, device.display_name())
                        .clicked()
                    {
                        *selected_device = Some(device.address.clone());
                    }
                    ui.monospace(&device.address);
                    ui.label(
                        device
                            .rssi
                            .map(|value| format!("{value} dBm"))
                            .unwrap_or_else(|| "n/a".to_string()),
                    );
                    ui.label(device.proximity_band());
                    ui.label(format!("{}%", device.stability_score()));
                    ui.label(device.recurrence_label());
                    ui.label(format_relative_time(device.last_seen));

                    let mut traits = Vec::new();
                    if device.connected {
                        traits.push("connected");
                    }
                    if device.paired {
                        traits.push("paired");
                    }
                    if device.services_resolved {
                        traits.push("services");
                    }
                    if device.battery_percentage.is_some() {
                        traits.push("battery");
                    }
                    ui.label(if traits.is_empty() {
                        "advertising".to_string()
                    } else {
                        traits.join(", ")
                    });
                    ui.end_row();
                }
            });
    });

    action
}

fn render_map_view(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    selected_device: &mut Option<String>,
) {
    ui.heading("Proximity Radar");
    ui.add_space(4.0);

    // Band counts for the header
    let mut near_count = 0usize;
    let mut mid_count = 0usize;
    let mut far_count = 0usize;
    let mut stale_count = 0usize;
    for device in &snapshot.devices {
        match device.proximity_band() {
            "Near" => near_count += 1,
            "Mid" => mid_count += 1,
            "Far" => far_count += 1,
            "Stale" => stale_count += 1,
            _ => far_count += 1,
        }
    }

    // Legend with counts
    ui.horizontal_wrapped(|ui| {
        for (label, count, color) in [
            ("Near", near_count, COLOR_NEAR),
            ("Mid", mid_count, COLOR_MID),
            ("Far", far_count, COLOR_FAR),
            ("Stale", stale_count, COLOR_STALE),
        ] {
            legend_chip(ui, &format!("{label} {count}"), color);
        }
    });
    ui.add_space(4.0);

    // Radar canvas
    let available = ui.available_size();
    let side = available.x.min(available.y - 8.0).max(200.0);
    let (response, painter) =
        ui.allocate_painter(egui::vec2(side, side), egui::Sense::click_and_drag());
    let center = response.rect.center();
    let radius = side * 0.45;

    // Subtle grid crosshairs
    let grid_color = egui::Color32::from_rgba_premultiplied(60, 75, 85, 40);
    painter.line_segment(
        [
            egui::pos2(center.x - radius, center.y),
            egui::pos2(center.x + radius, center.y),
        ],
        egui::Stroke::new(0.5, grid_color),
    );
    painter.line_segment(
        [
            egui::pos2(center.x, center.y - radius),
            egui::pos2(center.x, center.y + radius),
        ],
        egui::Stroke::new(0.5, grid_color),
    );

    // Band rings with dBm thresholds
    let rings: &[(f32, &str, egui::Color32)] = &[
        (RING_NEAR, "-58 dBm", COLOR_NEAR),
        (RING_MID, "-74 dBm", COLOR_MID),
        (RING_FAR, "-100 dBm", COLOR_FAR),
    ];
    let dim_label = egui::Color32::from_rgb(90, 105, 115);
    for &(fraction, db_label, tint) in rings.iter().rev() {
        let r = radius * fraction;
        painter.circle(
            center,
            r,
            tint.gamma_multiply(0.04),
            egui::Stroke::new(1.0, tint.gamma_multiply(0.18)),
        );
        // dBm label at top of ring
        painter.text(
            egui::pos2(center.x + 4.0, center.y - r + 2.0),
            egui::Align2::LEFT_TOP,
            db_label,
            egui::FontId::new(10.0, egui::FontFamily::Proportional),
            dim_label,
        );
    }

    // Center dot (adapter)
    painter.circle_filled(center, 4.0, egui::Color32::from_rgb(200, 210, 220));
    painter.circle_stroke(
        center,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(140, 150, 160)),
    );

    // Bucket devices by band, assign angles within each band's arc sector
    // This prevents near and far devices from overlapping at the same angle
    let mut near_devices: Vec<&DeviceRecord> = Vec::new();
    let mut mid_devices: Vec<&DeviceRecord> = Vec::new();
    let mut far_devices: Vec<&DeviceRecord> = Vec::new();
    let mut stale_devices: Vec<&DeviceRecord> = Vec::new();

    for device in &snapshot.devices {
        match device.proximity_band() {
            "Near" => near_devices.push(device),
            "Mid" => mid_devices.push(device),
            "Stale" => stale_devices.push(device),
            _ => far_devices.push(device),
        }
    }

    // Each band gets a full 360 degrees but with a different starting offset
    // so the bands don't stack on the same angles
    let golden_angle: f32 = TAU * 0.381966; // golden ratio distribution
    let mut hovered_device: Option<&DeviceRecord> = None;
    let pointer_pos = response.hover_pos();

    for (band_devices, band_offset) in [
        (&near_devices, 0.0f32),
        (&mid_devices, golden_angle * 0.5),
        (&far_devices, golden_angle),
        (&stale_devices, golden_angle * 1.5),
    ] {
        let count = band_devices.len();
        for (index, device) in band_devices.iter().enumerate() {
            let distance_fraction = rssi_to_fraction(device);
            let angle = band_offset
                + if count <= 1 {
                    0.0
                } else {
                    // Golden angle spacing within each band for even distribution
                    index as f32 * golden_angle
                };

            let r = radius * distance_fraction;
            let dot_pos = egui::pos2(
                center.x + r * angle.cos(),
                center.y + r * angle.sin(),
            );

            let tint = band_color(device);
            let is_selected = selected_device.as_deref() == Some(device.address.as_str());
            let is_stale = device.stale;

            // Scale dot by stability: 3..8px for normal, bigger for selected
            let base_dot = if is_stale {
                3.0
            } else {
                3.0 + (device.stability_score() as f32 / 100.0) * 5.0
            };
            let dot_radius = if is_selected { base_dot + 3.0 } else { base_dot };
            let alpha = if is_stale { 0.45 } else { 0.85 };

            // Filled dot
            painter.circle_filled(dot_pos, dot_radius, tint.gamma_multiply(alpha));

            // Selection ring
            if is_selected {
                painter.circle_stroke(
                    dot_pos,
                    dot_radius + 2.5,
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                );
            }

            // Hover detection
            let hit_radius = dot_radius.max(8.0);
            let is_hovered = pointer_pos
                .is_some_and(|pos| pos.distance(dot_pos) <= hit_radius);

            // Show label only for: selected, named (non-unknown), or hovered
            let has_name = device.display_name() != "Unknown device";
            if is_selected || is_hovered {
                draw_device_label(&painter, device, dot_pos, dot_radius, angle, tint, alpha);
            } else if has_name && !is_stale {
                // Compact: name only, dimmer
                let label_text = truncate_name(device.display_name(), 16);
                let anchor = if angle.cos() >= 0.0 {
                    egui::Align2::LEFT_CENTER
                } else {
                    egui::Align2::RIGHT_CENTER
                };
                let offset_x = if angle.cos() >= 0.0 {
                    dot_radius + 4.0
                } else {
                    -(dot_radius + 4.0)
                };
                painter.text(
                    dot_pos + egui::vec2(offset_x, 0.0),
                    anchor,
                    label_text,
                    egui::FontId::new(10.0, egui::FontFamily::Proportional),
                    tint.gamma_multiply(alpha * 0.7),
                );
            }

            if is_hovered {
                hovered_device = Some(device);
            }

            // Click detection
            if response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|pos| pos.distance(dot_pos) <= hit_radius)
            {
                *selected_device = Some(device.address.clone());
            }
        }
    }

    // Tooltip on hover
    if let Some(device) = hovered_device {
        response.clone().on_hover_ui(|ui| {
            ui.strong(device.display_name());
            ui.monospace(&device.address);
            ui.label(format!(
                "RSSI: {}",
                device
                    .rssi
                    .map(|v| format!("{v} dBm"))
                    .unwrap_or_else(|| "n/a".to_string())
            ));
            ui.label(format!(
                "{} / {} / {}%",
                device.proximity_band(),
                device.recurrence_label(),
                device.stability_score()
            ));
            ui.label(format!("Last seen: {}", format_relative_time(device.last_seen)));
            if let Some(battery) = device.battery_percentage {
                ui.label(format!("Battery: {battery}%"));
            }
        });
    }
}

/// Full label with name + RSSI drawn next to a dot.
fn draw_device_label(
    painter: &egui::Painter,
    device: &DeviceRecord,
    dot_pos: egui::Pos2,
    dot_radius: f32,
    angle: f32,
    tint: egui::Color32,
    alpha: f32,
) {
    let name = truncate_name(device.display_name(), 20);
    let rssi_text = device
        .rssi
        .map(|v| format!("{v} dBm"))
        .unwrap_or_else(|| device.proximity_band().to_string());

    let anchor = if angle.cos() >= 0.0 {
        egui::Align2::LEFT_CENTER
    } else {
        egui::Align2::RIGHT_CENTER
    };
    let offset_x = if angle.cos() >= 0.0 {
        dot_radius + 5.0
    } else {
        -(dot_radius + 5.0)
    };

    painter.text(
        dot_pos + egui::vec2(offset_x, -7.0),
        anchor,
        name,
        egui::FontId::new(11.0, egui::FontFamily::Proportional),
        tint.gamma_multiply(alpha),
    );
    painter.text(
        dot_pos + egui::vec2(offset_x, 6.0),
        anchor,
        rssi_text,
        egui::FontId::new(10.0, egui::FontFamily::Proportional),
        tint.gamma_multiply(alpha * 0.65),
    );
}

pub(crate) fn truncate_name(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max - 1).collect();
        format!("{truncated}\u{2026}")
    }
}

// Ring fractions matching proximity_band thresholds
const RING_NEAR: f32 = 0.30;
const RING_MID: f32 = 0.58;
const RING_FAR: f32 = 1.00;

const COLOR_NEAR: egui::Color32 = egui::Color32::from_rgb(81, 196, 136);
const COLOR_MID: egui::Color32 = egui::Color32::from_rgb(97, 173, 255);
const COLOR_FAR: egui::Color32 = egui::Color32::from_rgb(185, 122, 255);
const COLOR_STALE: egui::Color32 = egui::Color32::from_rgb(209, 135, 61);
const COLOR_ERROR: egui::Color32 = egui::Color32::from_rgb(220, 82, 70);
const COLOR_TRACKER: egui::Color32 = egui::Color32::from_rgb(230, 60, 60);

/// Map a device's RSSI to a 0.0..1.0 fraction where 0 = center, 1 = edge.
pub(crate) fn rssi_to_fraction(device: &DeviceRecord) -> f32 {
    let rssi = device
        .rssi
        .or_else(|| device.avg_rssi().map(|v| v.round() as i16));

    match rssi {
        Some(value) => {
            // Clamp to -100..0 range, map linearly: 0 dBm = center, -100 = edge
            let clamped = (value as f32).clamp(-100.0, 0.0);
            (-clamped) / 100.0
        }
        None if device.stale => 0.92,
        None => 0.80,
    }
}

pub(crate) fn band_color(device: &DeviceRecord) -> egui::Color32 {
    match device.proximity_band() {
        "Near" => COLOR_NEAR,
        "Mid" => COLOR_MID,
        "Far" => COLOR_FAR,
        "Stale" => COLOR_STALE,
        _ => egui::Color32::from_rgb(180, 180, 180),
    }
}

pub(crate) fn category_color(category: &crate::model::DeviceCategory) -> egui::Color32 {
    use crate::model::DeviceCategory;
    match category {
        DeviceCategory::Phone | DeviceCategory::Tablet => egui::Color32::from_rgb(97, 173, 255),
        DeviceCategory::Computer => egui::Color32::from_rgb(140, 180, 255),
        DeviceCategory::Headphone | DeviceCategory::Headset | DeviceCategory::Speaker => {
            egui::Color32::from_rgb(185, 122, 255)
        }
        DeviceCategory::Wearable => egui::Color32::from_rgb(120, 220, 200),
        DeviceCategory::FitnessTracker => egui::Color32::from_rgb(81, 196, 136),
        DeviceCategory::HealthSensor => egui::Color32::from_rgb(100, 220, 120),
        DeviceCategory::InputDevice => egui::Color32::from_rgb(220, 200, 100),
        DeviceCategory::Beacon => egui::Color32::from_rgb(220, 160, 80),
        DeviceCategory::Tracker => COLOR_TRACKER,
        DeviceCategory::NetworkDevice => egui::Color32::from_rgb(100, 180, 220),
        DeviceCategory::Printer => egui::Color32::from_rgb(160, 160, 160),
        DeviceCategory::Vehicle => egui::Color32::from_rgb(200, 160, 100),
        DeviceCategory::SmartHome => egui::Color32::from_rgb(180, 220, 140),
        DeviceCategory::Unknown => egui::Color32::from_rgb(120, 130, 140),
    }
}

fn legend_chip(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(6.0)
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.4)))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.colored_label(color, label);
        });
}

// ── Profiles view ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ProfileSort {
    #[default]
    Stability,
    Name,
    LastSeen,
    Rssi,
}

fn render_profiles_view(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    selected_tab: &mut ViewTab,
    selected_device: &mut Option<String>,
) {
    // Toolbar: search + sort
    let search_id = ui.id().with("prof_search");
    let sort_id = ui.id().with("prof_sort");
    let filter_id = ui.id().with("prof_filter");

    let mut search_text: String = ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());
    let mut sort_mode: ProfileSort = ui.data_mut(|d| d.get_temp(sort_id).unwrap_or_default());
    let mut only_trackers: bool = ui.data_mut(|d| d.get_temp(filter_id).unwrap_or(false));

    ui.horizontal(|ui| {
        ui.heading("Device Profiles");
        ui.add_space(16.0);
        ui.label(RichText::new(format!("{} logged", snapshot.devices.len())).small()
            .color(egui::Color32::from_rgb(120, 140, 160)));
    });
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        let search_resp = ui.add(
            egui::TextEdit::singleline(&mut search_text)
                .hint_text("Search name / address…")
                .desired_width(220.0),
        );
        if search_resp.changed() {
            ui.data_mut(|d| d.insert_temp(search_id, search_text.clone()));
        }

        ui.add_space(8.0);

        let tracker_count = snapshot.devices.iter().filter(|d| d.tracker_alert.is_some()).count();
        let t_label = format!("Trackers {tracker_count}");
        let (t_fill, t_stroke) = if only_trackers {
            (COLOR_TRACKER.gamma_multiply(0.25), egui::Stroke::new(1.5, COLOR_TRACKER))
        } else {
            (COLOR_TRACKER.gamma_multiply(0.08), egui::Stroke::new(1.0, COLOR_TRACKER.gamma_multiply(0.3)))
        };
        let t_btn = egui::Button::new(
            RichText::new(&t_label)
                .color(if only_trackers { COLOR_TRACKER } else { COLOR_TRACKER.gamma_multiply(0.65) })
                .strong()
        )
        .fill(t_fill)
        .stroke(t_stroke)
        .corner_radius(8.0);
        if ui.add(t_btn).clicked() {
            only_trackers = !only_trackers;
            ui.data_mut(|d| d.insert_temp(filter_id, only_trackers));
        }

        ui.add_space(16.0);
        ui.label("Sort:");
        for (label, mode) in [
            ("Stability", ProfileSort::Stability),
            ("Name", ProfileSort::Name),
            ("Last Seen", ProfileSort::LastSeen),
            ("RSSI", ProfileSort::Rssi),
        ] {
            let active = sort_mode == mode;
            let btn = egui::Button::new(RichText::new(label).small())
                .fill(if active {
                    egui::Color32::from_rgb(40, 65, 90)
                } else {
                    egui::Color32::TRANSPARENT
                })
                .stroke(if active {
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(97, 173, 255))
                } else {
                    egui::Stroke::NONE
                })
                .corner_radius(5.0);
            if ui.add(btn).clicked() {
                sort_mode = mode;
                ui.data_mut(|d| d.insert_temp(sort_id, sort_mode));
            }
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Build filtered + sorted list
    let query = search_text.to_lowercase();
    let mut devices: Vec<&DeviceRecord> = snapshot
        .devices
        .iter()
        .filter(|d| {
            if only_trackers && d.tracker_alert.is_none() {
                return false;
            }
            if !query.is_empty() {
                let name_match = d.display_name().to_lowercase().contains(&query);
                let addr_match = d.address.to_lowercase().contains(&query);
                if !name_match && !addr_match {
                    return false;
                }
            }
            true
        })
        .collect();

    match sort_mode {
        ProfileSort::Stability => {
            devices.sort_by(|a, b| {
                b.stability_score().cmp(&a.stability_score())
                    .then_with(|| b.first_seen.cmp(&a.first_seen))
                    .then_with(|| a.address.cmp(&b.address))
            });
        }
        ProfileSort::Name => devices.sort_by(|a, b| a.display_name().cmp(&b.display_name())),
        ProfileSort::LastSeen => devices.sort_by(|a, b| b.last_seen.cmp(&a.last_seen)),
        ProfileSort::Rssi => devices.sort_by(|a, b| {
            let ra = a.rssi.unwrap_or(i16::MIN);
            let rb = b.rssi.unwrap_or(i16::MIN);
            rb.cmp(&ra)
        }),
    }

    if devices.is_empty() {
        ui.add_space(24.0);
        ui.label(
            RichText::new(if snapshot.devices.is_empty() {
                "No devices logged yet. Start a scan to discover nearby Bluetooth devices."
            } else {
                "No devices match the current filter."
            })
            .color(egui::Color32::from_rgb(140, 150, 160)),
        );
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            for device in devices {
                if let Some(goto) = render_profile_card(ui, device) {
                    *selected_device = Some(goto);
                    *selected_tab = ViewTab::Scan;
                }
            }
        });
    });
}

/// Renders a compact device profile card. Returns the device address if clicked.
fn render_profile_card(ui: &mut egui::Ui, device: &DeviceRecord) -> Option<String> {
    let card_width = 220.0_f32;
    let is_tracker = device.tracker_alert.is_some();
    let band_col = band_color(device);
    let cat_col = category_color(&device.category);

    let border_color = if is_tracker {
        COLOR_TRACKER
    } else if device.stale {
        egui::Color32::from_rgb(50, 65, 80)
    } else {
        band_col.gamma_multiply(0.5)
    };
    let fill_color = if is_tracker {
        COLOR_TRACKER.gamma_multiply(0.07)
    } else {
        egui::Color32::from_rgb(14, 24, 34)
    };

    let mut goto: Option<String> = None;

    egui::Frame::new()
        .fill(fill_color)
        .corner_radius(10.0)
        .stroke(egui::Stroke::new(1.5, border_color))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(card_width);

            // Row 1: status dot + name + tracker badge
            ui.horizontal(|ui| {
                let (dot_char, dot_color) = if is_tracker {
                    ("▲", COLOR_TRACKER)
                } else if device.stale {
                    ("●", COLOR_STALE)
                } else {
                    ("●", band_col)
                };
                ui.colored_label(dot_color, dot_char);
                let name = truncate_name(&device.display_name(), 18);
                ui.label(RichText::new(name).strong());
                if is_tracker {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(
                            COLOR_TRACKER,
                            RichText::new("TRACKER").small().strong(),
                        );
                    });
                }
            });

            // Row 2: MAC + address type
            ui.horizontal(|ui| {
                ui.monospace(
                    RichText::new(&device.address)
                        .small()
                        .color(egui::Color32::from_rgb(120, 140, 160)),
                );
            });
            let addr_type_color = match &device.address_type_detail {
                AddressTypeDetail::Public => egui::Color32::from_rgb(81, 196, 136),
                AddressTypeDetail::RandomStatic => egui::Color32::from_rgb(130, 175, 245),
                AddressTypeDetail::RandomResolvable => egui::Color32::from_rgb(220, 180, 92),
                AddressTypeDetail::RandomNonResolvable |
                AddressTypeDetail::Unknown => egui::Color32::from_rgb(100, 110, 125),
            };
            ui.label(
                RichText::new(device.address_type_detail.label())
                    .small()
                    .color(addr_type_color),
            );

            // Row 3: category badge + manufacturer
            ui.horizontal(|ui| {
                legend_chip(ui, device.category.label(), cat_col);
                let mfr = device.gatt_manufacturer.as_deref()
                    .or(device.oui_manufacturer.as_deref())
                    .or(device.apple_device_type.as_deref());
                if let Some(m) = mfr {
                    ui.label(
                        RichText::new(truncate_name(m, 14))
                            .small()
                            .color(egui::Color32::from_rgb(160, 200, 255)),
                    );
                }
            });

            // Row 4: RSSI bar + proximity + adv interval
            ui.horizontal(|ui| {
                let rssi_val = device.rssi.unwrap_or(-120);
                let filled = ((rssi_val + 100).max(0).min(100) as usize * 5 / 100).min(5);
                let bar: String = "█".repeat(filled) + &"░".repeat(5 - filled);
                ui.colored_label(
                    band_col,
                    RichText::new(bar).small().monospace(),
                );
                let rssi_str = device.rssi
                    .map(|v| format!("{v} dBm"))
                    .unwrap_or_else(|| "n/a".to_string());
                ui.label(
                    RichText::new(format!("{} {}", rssi_str, device.proximity_band()))
                        .small()
                        .color(egui::Color32::from_rgb(140, 160, 175)),
                );
            });

            // Row 5: sighting count + last seen
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} sightings  ·  {}",
                        device.advertisement_count,
                        crate::model::format_relative_time(device.last_seen)
                    ))
                    .small()
                    .color(egui::Color32::from_rgb(100, 120, 135)),
                );
            });

            // Interaction — full card click
            let response = ui.interact(
                ui.min_rect(),
                ui.id().with(&device.address),
                egui::Sense::click(),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                goto = Some(device.address.clone());
            }
        });

    goto
}

fn render_device_detail(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    selected_device: &mut Option<String>,
    rename_draft: &mut Option<String>,
) -> Option<AppAction> {
    let mut action: Option<AppAction> = None;
    ui.heading("Device Detail");
    ui.add_space(8.0);

    let current = selected_device
        .as_deref()
        .and_then(|address| {
            snapshot
                .devices
                .iter()
                .find(|device| device.address == address)
        })
        .or_else(|| snapshot.devices.first());

    if current.is_none() {
        ui.label("No device selected.");
        return action;
    }
    let device = current.unwrap();
    *selected_device = Some(device.address.clone());

    // ── Tracker alert banner ──────────────────────────────────────────────────
    if let Some(alert) = &device.tracker_alert {
        let confidence_str = match alert.confidence {
            TrackerConfidence::Definite => "CONFIRMED",
            TrackerConfidence::Likely => "LIKELY",
        };
        egui::Frame::new()
            .fill(COLOR_TRACKER.gamma_multiply(0.18))
            .corner_radius(6.0)
            .stroke(egui::Stroke::new(1.5, COLOR_TRACKER))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.colored_label(
                    COLOR_TRACKER,
                    RichText::new(format!("TRACKER DETECTED [{confidence_str}]")).strong(),
                );
                ui.colored_label(COLOR_TRACKER, &alert.name);
                ui.small(RichText::new(&alert.detail).color(COLOR_TRACKER.gamma_multiply(0.75)));
            });
        ui.add_space(6.0);
    }

    ui.label(RichText::new(device.display_name()).heading());
    ui.monospace(device.address.as_str());

    // Address type detail
    let addr_type_color = match &device.address_type_detail {
        AddressTypeDetail::Public => egui::Color32::from_rgb(81, 196, 136),
        AddressTypeDetail::RandomStatic => egui::Color32::from_rgb(160, 200, 255),
        AddressTypeDetail::RandomResolvable => egui::Color32::from_rgb(220, 180, 92),
        AddressTypeDetail::RandomNonResolvable => egui::Color32::from_rgb(140, 150, 160),
        AddressTypeDetail::Unknown => egui::Color32::from_rgb(100, 110, 120),
    };
    ui.colored_label(
        addr_type_color,
        RichText::new(device.address_type_detail.label()).small(),
    );

    // OUI manufacturer / GATT manufacturer
    let mfr_display = device.gatt_manufacturer.as_deref()
        .or(device.oui_manufacturer.as_deref());
    if let Some(mfr) = mfr_display {
        let source = if device.gatt_manufacturer.is_some() { "" } else { " (OUI)" };
        ui.label(
            RichText::new(format!("Manufacturer: {mfr}{source}"))
                .color(egui::Color32::from_rgb(160, 200, 255)),
        );
    }

    // GATT model / hardware
    if let Some(model) = &device.gatt_model {
        ui.label(
            RichText::new(format!("Model: {model}"))
                .color(egui::Color32::from_rgb(160, 200, 255)),
        );
    }

    // Category badge + Apple device type
    let cat_color = category_color(&device.category);
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(cat_color.gamma_multiply(0.15))
            .corner_radius(4.0)
            .stroke(egui::Stroke::new(1.0, cat_color.gamma_multiply(0.5)))
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.colored_label(cat_color, device.category.label());
            });
        if let Some(apple_type) = &device.apple_device_type {
            ui.colored_label(
                egui::Color32::from_rgb(200, 200, 220),
                RichText::new(apple_type).small(),
            );
        }
    });

    ui.add_space(8.0);

    let status_color = if device.stale {
        egui::Color32::from_rgb(209, 135, 61)
    } else {
        egui::Color32::from_rgb(81, 196, 136)
    };
    ui.colored_label(
        status_color,
        if device.stale {
            "Currently stale"
        } else {
            "Currently live"
        },
    );
    ui.label(format!("Adapter: {}", device.adapter_name));
    ui.label(format!(
        "First seen: {}",
        device
            .first_seen
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
    ));
    ui.label(format!(
        "Last seen: {}",
        device
            .last_seen
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
    ));
    ui.label(format!("Seen count: {}", device.seen_count));
    ui.label(format!("Advertisements: {}", device.advertisement_count));
    ui.label(format!("Proximity band: {}", device.proximity_band()));
    ui.label(format!("Stability score: {}%", device.stability_score()));
    ui.label(format!(
        "Recurrence: {} ({} active days)",
        device.recurrence_label(),
        device.active_days
    ));

    // ── Manage ────────────────────────────────────────────────────────────────
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
    ui.strong("Manage");
    ui.add_space(4.0);

    let addr = device.address.clone();

    egui::ScrollArea::vertical()
        .id_salt("device_detail_scroll")
        .max_height(f32::INFINITY)
        .show(ui, |ui| {
        // Connection row
        ui.horizontal_wrapped(|ui| {
            let connect_btn = egui::Button::new("Connect")
                .fill(egui::Color32::from_rgb(30, 60, 40))
                .stroke(egui::Stroke::new(1.0, COLOR_NEAR.gamma_multiply(0.6)));
            if ui.add_enabled(!device.connected, connect_btn).clicked() {
                action = Some(AppAction::ConnectDevice(addr.clone()));
            }

            let disconnect_btn = egui::Button::new("Disconnect")
                .fill(egui::Color32::from_rgb(50, 30, 20))
                .stroke(egui::Stroke::new(1.0, COLOR_STALE.gamma_multiply(0.6)));
            if ui.add_enabled(device.connected, disconnect_btn).clicked() {
                action = Some(AppAction::DisconnectDevice(addr.clone()));
            }
        });

        // Trust / Block row
        ui.horizontal_wrapped(|ui| {
            let trust_label = if device.trusted { "Untrust" } else { "Trust" };
            let trust_color = if device.trusted { COLOR_STALE } else { COLOR_MID };
            let trust_btn = egui::Button::new(trust_label)
                .fill(trust_color.gamma_multiply(0.12))
                .stroke(egui::Stroke::new(1.0, trust_color.gamma_multiply(0.5)));
            if ui.add(trust_btn).clicked() {
                action = Some(AppAction::SetTrusted {
                    address: addr.clone(),
                    trusted: !device.trusted,
                });
            }

            let block_label = if device.blocked { "Unblock" } else { "Block" };
            let block_color = if device.blocked { COLOR_NEAR } else { COLOR_ERROR };
            let block_btn = egui::Button::new(block_label)
                .fill(block_color.gamma_multiply(0.12))
                .stroke(egui::Stroke::new(1.0, block_color.gamma_multiply(0.5)));
            if ui.add(block_btn).clicked() {
                action = Some(AppAction::SetBlocked {
                    address: addr.clone(),
                    blocked: !device.blocked,
                });
            }

            let forget_btn = egui::Button::new("Forget")
                .fill(egui::Color32::from_rgb(50, 15, 15))
                .stroke(egui::Stroke::new(1.0, COLOR_ERROR.gamma_multiply(0.6)));
            if ui.add(forget_btn).clicked() {
                action = Some(AppAction::ForgetDevice(addr.clone()));
            }
        });

        // Rename row
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let is_renaming = rename_draft.is_some();
            if !is_renaming {
                let current_name = device.alias
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(device.name.as_deref().unwrap_or(""));
                if ui.button("Rename").clicked() {
                    *rename_draft = Some(current_name.to_string());
                }
            } else {
                let draft = rename_draft.as_mut().unwrap();
                let text_edit = egui::TextEdit::singleline(draft)
                    .desired_width(180.0)
                    .hint_text("Alias…");
                let response = ui.add(text_edit);
                let submitted = response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Apply").clicked() || submitted {
                    let alias = draft.trim().to_string();
                    action = Some(AppAction::SetAlias {
                        address: addr.clone(),
                        alias,
                    });
                    *rename_draft = None;
                }
                if ui.button("Cancel").clicked() {
                    *rename_draft = None;
                }
            }
        });
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.strong("Signal");
    ui.label(
        device
            .rssi
            .map(|value| format!("Current RSSI: {value} dBm"))
            .unwrap_or_else(|| "Current RSSI: n/a".to_string()),
    );
    ui.label(
        device
            .avg_rssi()
            .map(|value| format!("Average RSSI: {value:.1} dBm"))
            .unwrap_or_else(|| "Average RSSI: n/a".to_string()),
    );
    ui.label(
        device
            .rssi_std_dev()
            .map(|value| format!("RSSI deviation: {value:.1}"))
            .unwrap_or_else(|| "RSSI deviation: n/a".to_string()),
    );
    ui.label(
        device
            .tx_power
            .map(|value| format!("TX Power: {value} dBm"))
            .unwrap_or_else(|| "TX Power: n/a".to_string()),
    );
    if let Some(battery) = device.battery_percentage {
        ui.label(format!("Battery: {battery}%"));
    }
    if let Some(interval) = crate::intelligence::profile::estimate_adv_interval_ms(device) {
        let interval_label = if interval < 1000 {
            format!("Adv interval: ~{interval} ms")
        } else {
            format!("Adv interval: ~{:.1} s", interval as f32 / 1000.0)
        };
        ui.label(interval_label);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.strong("Traits");
    for trait_line in [
        format!("Paired: {}", yes_no(device.paired)),
        format!("Trusted: {}", yes_no(device.trusted)),
        format!("Connected: {}", yes_no(device.connected)),
        format!("Blocked: {}", yes_no(device.blocked)),
        format!("Services resolved: {}", yes_no(device.services_resolved)),
        format!("Legacy pairing: {}", yes_no(device.legacy_pairing)),
        format!("Wake allowed: {}", yes_no(device.wake_allowed)),
    ] {
        ui.label(trait_line);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.strong("Advertising");
    if !device.manufacturer_data.is_empty() {
        ui.label("Manufacturer data:");
        for entry in &device.manufacturer_data {
            let company = entry.company_name.as_deref().unwrap_or("Unknown");
            ui.monospace(format!(
                "0x{:04X}  {}  {}",
                entry.id, company, entry.payload_hex
            ));
        }
    } else {
        ui.label("Manufacturer data: None");
    }
    if !device.service_data.is_empty() {
        ui.add_space(6.0);
        ui.label("Service data:");
        for entry in &device.service_data {
            ui.monospace(format!("{}: {}", entry.uuid, entry.payload_hex));
        }
    }
    if !device.uuids.is_empty() {
        ui.add_space(6.0);
        ui.label("Service UUIDs:");
        for uuid in &device.uuids {
            ui.monospace(uuid.as_str());
        }
        if !device.decoded_services.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("Decoded services:")
                    .color(egui::Color32::from_rgb(140, 150, 160))
                    .small(),
            );
            for name in &device.decoded_services {
                ui.colored_label(egui::Color32::from_rgb(160, 200, 255), name);
            }
        }
    }

    // ── Beacon ────────────────────────────────────────────────────────────────
    if let Some(beacon) = &device.beacon {
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.strong("Beacon");
        ui.label(format!("Format: {}", beacon.format.label()));
        if let Some(uuid) = &beacon.uuid {
            ui.monospace(format!("UUID: {uuid}"));
        }
        if let Some(major) = beacon.major {
            ui.label(format!("Major: {major}"));
        }
        if let Some(minor) = beacon.minor {
            ui.label(format!("Minor: {minor}"));
        }
        if let Some(ns) = &beacon.namespace {
            ui.monospace(format!("Namespace: {ns}"));
        }
        if let Some(inst) = &beacon.instance {
            ui.monospace(format!("Instance: {inst}"));
        }
        if let Some(url) = &beacon.url {
            ui.monospace(format!("URL: {url}"));
        }
        if let Some(dbm) = beacon.calibrated_power_dbm {
            ui.label(format!("Calibrated TX: {dbm} dBm @ 1 m"));
        }
        if let Some(dist) = beacon.estimated_distance_m {
            let dist_label = if dist < 1.0 {
                format!("Est. distance: {:.2} m", dist)
            } else {
                format!("Est. distance: {:.1} m", dist)
            };
            ui.colored_label(egui::Color32::from_rgb(160, 200, 255), dist_label);
        }
        if let Some(mv) = beacon.battery_mv {
            ui.label(format!("Battery: {} mV", mv));
        }
        if let Some(temp) = beacon.temperature_c {
            ui.label(format!("Temperature: {:.1} °C", temp));
        }
    }

    // ── GATT Device Info ──────────────────────────────────────────────────────
    let has_gatt = device.gatt_manufacturer.is_some()
        || device.gatt_model.is_some()
        || device.gatt_firmware.is_some()
        || device.gatt_hardware.is_some();
    if has_gatt {
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.strong("GATT Device Info");
        if let Some(v) = &device.gatt_manufacturer {
            ui.label(format!("Manufacturer: {v}"));
        }
        if let Some(v) = &device.gatt_model {
            ui.label(format!("Model: {v}"));
        }
        if let Some(v) = &device.gatt_firmware {
            ui.label(format!("Firmware: {v}"));
        }
        if let Some(v) = &device.gatt_hardware {
            ui.label(format!("Hardware: {v}"));
        }
    }

    if let Some(error) = &snapshot.last_error {
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.colored_label(egui::Color32::from_rgb(220, 82, 70), error);
    }

    action
}

fn render_adapter_view(ui: &mut egui::Ui, snapshot: &AppSnapshot) -> Option<AppAction> {
    let mut action: Option<AppAction> = None;

    ui.heading("Adapter Management");
    ui.add_space(8.0);

    if snapshot.adapters.is_empty() {
        ui.label(
            RichText::new("No Bluetooth adapters found.")
                .color(egui::Color32::from_rgb(140, 150, 160)),
        );
        return action;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for adapter in &snapshot.adapters {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(18, 28, 36))
                .corner_radius(8.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 60, 75)))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        let power_color = if adapter.powered { COLOR_NEAR } else { COLOR_STALE };
                        ui.colored_label(power_color, if adapter.powered { "●" } else { "○" });
                        ui.strong(&adapter.alias);
                        ui.label(
                            RichText::new(format!("({})", adapter.name))
                                .small()
                                .color(egui::Color32::from_rgb(120, 140, 155)),
                        );
                    });

                    if let Some(addr) = &adapter.address {
                        ui.label(
                            RichText::new(addr.as_str())
                                .monospace()
                                .color(egui::Color32::from_rgb(140, 160, 180)),
                        );
                    }

                    ui.add_space(8.0);

                    // Controls grid
                    egui::Grid::new(format!("adapter_grid_{}", adapter.name))
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            // Powered
                            ui.label("Powered");
                            ui.horizontal(|ui| {
                                let on_btn = egui::Button::new("On")
                                    .fill(if adapter.powered {
                                        COLOR_NEAR.gamma_multiply(0.25)
                                    } else {
                                        egui::Color32::from_rgb(26, 42, 51)
                                    })
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        if adapter.powered { COLOR_NEAR } else { egui::Color32::from_rgb(60, 80, 90) },
                                    ));
                                if ui.add_enabled(!adapter.powered, on_btn).clicked() {
                                    action = Some(AppAction::SetAdapterPowered {
                                        adapter: adapter.name.clone(),
                                        powered: true,
                                    });
                                }
                                let off_btn = egui::Button::new("Off")
                                    .fill(if !adapter.powered {
                                        COLOR_STALE.gamma_multiply(0.25)
                                    } else {
                                        egui::Color32::from_rgb(26, 42, 51)
                                    })
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        if !adapter.powered { COLOR_STALE } else { egui::Color32::from_rgb(60, 80, 90) },
                                    ));
                                if ui.add_enabled(adapter.powered, off_btn).clicked() {
                                    action = Some(AppAction::SetAdapterPowered {
                                        adapter: adapter.name.clone(),
                                        powered: false,
                                    });
                                }
                            });
                            ui.end_row();

                            // Discoverable
                            ui.label("Discoverable");
                            ui.horizontal(|ui| {
                                let disc_color = if adapter.discoverable { COLOR_MID } else { egui::Color32::from_rgb(60, 80, 90) };
                                let disc_btn = egui::Button::new(
                                    if adapter.discoverable { "On  " } else { "Off " },
                                )
                                .fill(disc_color.gamma_multiply(if adapter.discoverable { 0.2 } else { 0.05 }))
                                .stroke(egui::Stroke::new(1.0, disc_color));
                                if ui.add(disc_btn).clicked() {
                                    action = Some(AppAction::SetAdapterDiscoverable {
                                        adapter: adapter.name.clone(),
                                        discoverable: !adapter.discoverable,
                                    });
                                }
                            });
                            ui.end_row();

                            // Pairable
                            ui.label("Pairable");
                            ui.horizontal(|ui| {
                                let pair_color = if adapter.pairable { COLOR_MID } else { egui::Color32::from_rgb(60, 80, 90) };
                                let pair_btn = egui::Button::new(
                                    if adapter.pairable { "On  " } else { "Off " },
                                )
                                .fill(pair_color.gamma_multiply(if adapter.pairable { 0.2 } else { 0.05 }))
                                .stroke(egui::Stroke::new(1.0, pair_color));
                                if ui.add(pair_btn).clicked() {
                                    action = Some(AppAction::SetAdapterPairable {
                                        adapter: adapter.name.clone(),
                                        pairable: !adapter.pairable,
                                    });
                                }
                            });
                            ui.end_row();

                            // Discovering (read-only status)
                            ui.label("Discovering");
                            ui.colored_label(
                                if adapter.discovering { COLOR_NEAR } else { egui::Color32::from_rgb(100, 115, 125) },
                                if adapter.discovering { "Active" } else { "Idle" },
                            );
                            ui.end_row();
                        });
                });
            ui.add_space(12.0);
        }
    });

    action
}

fn render_event_log(ui: &mut egui::Ui, snapshot: &AppSnapshot) {
    if snapshot.event_log.is_empty() {
        ui.add_space(24.0);
        ui.label(
            RichText::new("No events recorded.")
                .color(egui::Color32::from_rgb(140, 150, 160)),
        );
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for entry in &snapshot.event_log {
            let color = match entry.level {
                LogLevel::Info => COLOR_MID,
                LogLevel::Warn => COLOR_STALE,
                LogLevel::Error => COLOR_ERROR,
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, entry.level.label());
                ui.small(
                    entry
                        .at
                        .with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string(),
                );
                ui.label(entry.message.as_str());
            });
        }
    });
}

pub(crate) fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()
    }

    fn make_device(address: &str) -> DeviceRecord {
        DeviceRecord::new(address, "hci0", fixed_time())
    }

    // ── truncate_name ───────────────────────────────────────────────

    #[test]
    fn truncate_name_short_string_unchanged() {
        assert_eq!(truncate_name("Hello", 10), "Hello");
    }

    #[test]
    fn truncate_name_exact_length_unchanged() {
        assert_eq!(truncate_name("12345", 5), "12345");
    }

    #[test]
    fn truncate_name_long_string_truncated() {
        let result = truncate_name("Hello World", 6);
        assert_eq!(result, "Hello\u{2026}");
        assert_eq!(result.chars().count(), 6);
    }

    #[test]
    fn truncate_name_multibyte_no_panic() {
        // Japanese characters are 3 bytes each in UTF-8
        let name = "日本語テストデバイス";
        let result = truncate_name(name, 5);
        assert_eq!(result.chars().count(), 5);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_name_emoji_no_panic() {
        let name = "📱🔵📡🎧🎵Speaker";
        let result = truncate_name(name, 4);
        assert_eq!(result.chars().count(), 4);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_name_mixed_ascii_multibyte() {
        let name = "BT-日本語";
        let result = truncate_name(name, 5);
        assert_eq!(result.chars().count(), 5);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_name_single_char_max() {
        // max=1 means 0 chars + ellipsis
        let result = truncate_name("Hello", 1);
        assert_eq!(result, "\u{2026}");
    }

    // ── rssi_to_fraction ────────────────────────────────────────────

    #[test]
    fn rssi_to_fraction_zero_dbm() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(0);
        assert!((rssi_to_fraction(&d) - 0.0).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_minus_100() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-100);
        assert!((rssi_to_fraction(&d) - 1.0).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_minus_50() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-50);
        assert!((rssi_to_fraction(&d) - 0.5).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_clamped_below_minus_100() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-120);
        // Should clamp to -100, so fraction = 1.0
        assert!((rssi_to_fraction(&d) - 1.0).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_clamped_above_zero() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(10);
        // Should clamp to 0, so fraction = 0.0
        assert!((rssi_to_fraction(&d) - 0.0).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_stale_no_rssi() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.mark_stale();
        assert!((rssi_to_fraction(&d) - 0.92).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_unknown_no_rssi() {
        let d = make_device("AA:00:00:00:00:01");
        assert!((rssi_to_fraction(&d) - 0.80).abs() < 0.01);
    }

    #[test]
    fn rssi_to_fraction_falls_back_to_avg() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = None;
        d.rssi_sum = -50;
        d.rssi_sum_squares = 2500.0;
        d.rssi_samples = 1;
        // avg_rssi = -50, fraction = 0.5
        assert!((rssi_to_fraction(&d) - 0.5).abs() < 0.01);
    }

    // ── band_color ──────────────────────────────────────────────────

    #[test]
    fn band_color_near() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-50);
        assert_eq!(band_color(&d), COLOR_NEAR);
    }

    #[test]
    fn band_color_mid() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-65);
        assert_eq!(band_color(&d), COLOR_MID);
    }

    #[test]
    fn band_color_far() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-80);
        assert_eq!(band_color(&d), COLOR_FAR);
    }

    #[test]
    fn band_color_stale() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.mark_stale();
        assert_eq!(band_color(&d), COLOR_STALE);
    }

    // ── yes_no ──────────────────────────────────────────────────────

    #[test]
    fn yes_no_values() {
        assert_eq!(yes_no(true), "Yes");
        assert_eq!(yes_no(false), "No");
    }

    // ── truncate_name edge cases ────────────────────────────────────

    #[test]
    fn truncate_name_empty_string() {
        assert_eq!(truncate_name("", 10), "");
    }

    #[test]
    fn truncate_name_two_char_max() {
        let result = truncate_name("Hello", 2);
        assert_eq!(result, "H\u{2026}");
        assert_eq!(result.chars().count(), 2);
    }

    // ── rssi_to_fraction monotonicity ───────────────────────────────

    #[test]
    fn rssi_to_fraction_increases_with_distance() {
        let mut close = make_device("AA:00:00:00:00:01");
        let mut far = make_device("AA:00:00:00:00:02");
        close.rssi = Some(-30);
        far.rssi = Some(-80);
        assert!(rssi_to_fraction(&close) < rssi_to_fraction(&far));
    }

    // ── band_color unknown ──────────────────────────────────────────

    #[test]
    fn band_color_unknown_device() {
        let d = make_device("AA:00:00:00:00:01");
        assert_eq!(d.proximity_band(), "Unknown");
        assert_eq!(band_color(&d), egui::Color32::from_rgb(180, 180, 180));
    }
}
