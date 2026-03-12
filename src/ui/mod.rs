pub mod settings_modal;

use std::f32::consts::TAU;

use eframe::egui::{self, RichText};

use crate::model::{AppSnapshot, DeviceRecord, LogLevel, format_relative_time};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Scan,
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFilter {
    Live,
    Stale,
    Public,
    Random,
    Errors,
}

pub enum TopAction {
    StartScan,
    StopScan,
    Refresh,
    OpenSettings,
}

pub fn render(
    ctx: &egui::Context,
    snapshot: &AppSnapshot,
    selected_tab: &mut ViewTab,
    scan_filter: &mut ScanFilter,
    selected_device: &mut Option<String>,
) -> Option<TopAction> {
    let mut action = None;

    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Bluetooth Mapper").size(24.0).strong());
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
                    action = Some(TopAction::OpenSettings);
                }
                if ui.button("Refresh").clicked() {
                    action = Some(TopAction::Refresh);
                }
                if snapshot.scan_active {
                    if ui.button("Stop Scan").clicked() {
                        action = Some(TopAction::StopScan);
                    }
                } else if ui.button("Start Scan").clicked() {
                    action = Some(TopAction::StartScan);
                }
            });
        });

        ui.add_space(8.0);

        // Top-level view tabs
        ui.horizontal(|ui| {
            ui.selectable_value(selected_tab, ViewTab::Scan, "Live Scan");
            ui.selectable_value(selected_tab, ViewTab::Map, "Proximity Radar");
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("Runtime: {}", format_relative_time(snapshot.metrics.started_at)))
                    .small()
                    .color(egui::Color32::from_rgb(140, 150, 160)),
            );
        });

        // Filter sub-tabs (only in Scan view)
        if *selected_tab == ViewTab::Scan {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                filter_tab(
                    ui, scan_filter, ScanFilter::Live,
                    &format!("Live {}", snapshot.metrics.live_devices),
                    COLOR_NEAR,
                );
                filter_tab(
                    ui, scan_filter, ScanFilter::Stale,
                    &format!("Stale {}", snapshot.metrics.stale_devices),
                    COLOR_STALE,
                );
                filter_tab(
                    ui, scan_filter, ScanFilter::Public,
                    &format!("Public {}", snapshot.metrics.public_devices),
                    COLOR_MID,
                );
                filter_tab(
                    ui, scan_filter, ScanFilter::Random,
                    &format!("Random {}", snapshot.metrics.random_devices),
                    COLOR_FAR,
                );
                filter_tab(
                    ui, scan_filter, ScanFilter::Errors,
                    &format!("Errors {}", snapshot.metrics.bluez_errors),
                    COLOR_ERROR,
                );
            });
        }
    });

    egui::SidePanel::right("device_detail")
        .min_width(340.0)
        .show(ctx, |ui| {
            render_device_detail(ui, snapshot, selected_device)
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        match selected_tab {
            ViewTab::Scan => render_scan_view(ui, snapshot, scan_filter, selected_device),
            ViewTab::Map => render_map_view(ui, snapshot, selected_device),
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
    snapshot: &AppSnapshot,
    scan_filter: &ScanFilter,
    selected_device: &mut Option<String>,
) {
    // Errors tab shows the event log instead of the device grid
    if *scan_filter == ScanFilter::Errors {
        render_event_log(ui, snapshot);
        return;
    }

    let devices: Vec<&DeviceRecord> = snapshot
        .sorted_devices()
        .into_iter()
        .filter(|d| match scan_filter {
            ScanFilter::Live => !d.stale,
            ScanFilter::Stale => d.stale,
            ScanFilter::Public => d.is_public_address(),
            ScanFilter::Random => !d.is_public_address(),
            ScanFilter::Errors => false,
        })
        .collect();

    if devices.is_empty() {
        ui.add_space(24.0);
        ui.label(
            RichText::new(match scan_filter {
                ScanFilter::Live => "No live devices.",
                ScanFilter::Stale => "No stale devices.",
                ScanFilter::Public => "No public-address devices.",
                ScanFilter::Random => "No random-address devices.",
                ScanFilter::Errors => "",
            })
            .color(egui::Color32::from_rgb(140, 150, 160)),
        );
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("device_grid")
            .num_columns(9)
            .spacing([14.0, 10.0])
            .striped(true)
            .show(ui, |ui| {
                for label in [
                    "State",
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
                    let label = if device.stale { "Stale" } else { "Live" };
                    let state_color = if device.stale {
                        COLOR_STALE
                    } else {
                        COLOR_NEAR
                    };
                    ui.colored_label(state_color, label);

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

fn truncate_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        name.to_string()
    } else {
        let mut s = name[..max - 1].to_string();
        s.push('\u{2026}'); // ellipsis
        s
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

/// Map a device's RSSI to a 0.0..1.0 fraction where 0 = center, 1 = edge.
fn rssi_to_fraction(device: &DeviceRecord) -> f32 {
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

fn band_color(device: &DeviceRecord) -> egui::Color32 {
    match device.proximity_band() {
        "Near" => COLOR_NEAR,
        "Mid" => COLOR_MID,
        "Far" => COLOR_FAR,
        "Stale" => COLOR_STALE,
        _ => egui::Color32::from_rgb(180, 180, 180),
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

fn render_device_detail(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    selected_device: &mut Option<String>,
) {
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
        return;
    }
    let device = current.unwrap();
    *selected_device = Some(device.address.clone());

    ui.label(RichText::new(device.display_name()).heading());
    ui.monospace(device.address.as_str());
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
    ui.label(format!("Manufacturers: {}", device.manufacturer_summary()));
    if !device.manufacturer_data.is_empty() {
        for entry in &device.manufacturer_data {
            ui.monospace(format!("0x{:04x}: {}", entry.id, entry.payload_hex));
        }
    }
    if !device.service_data.is_empty() {
        ui.add_space(6.0);
        ui.label("Service data");
        for entry in &device.service_data {
            ui.monospace(format!("{}: {}", entry.uuid, entry.payload_hex));
        }
    }
    if !device.uuids.is_empty() {
        ui.add_space(6.0);
        ui.label("Service UUIDs");
        for uuid in &device.uuids {
            ui.monospace(uuid);
        }
    }

    if let Some(error) = &snapshot.last_error {
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.colored_label(egui::Color32::from_rgb(220, 82, 70), error);
    }
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

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}
