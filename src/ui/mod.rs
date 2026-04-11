pub mod components;
pub mod settings_modal;
pub mod theme;

use std::f32::consts::TAU;

use eframe::egui::{self, RichText};

use crate::model::{
    AppSnapshot, DeviceRecord, TrackerConfidence, format_duration_since, format_relative_time,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Scan,
    Radar,
    Profiles,
    Adapter,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFilter {
    All,
    Near,
    Mid,
    Far,
    Stale,
}

pub enum AppAction {
    // Scanner controls
    StartScan,
    StopScan,
    Refresh,
    /// Begin a timed discovery cycle. The app starts the scanner if it's
    /// not running, shows a progress bar in the scan view, and on completion
    /// freezes the live device list into the captured list.
    StartScanCycle,
    /// Abort an in-progress discovery cycle without capturing results.
    CancelScanCycle,
    OpenSettings,
    // Device management
    ConnectDevice(String),
    DisconnectDevice(String),
    SetTrusted {
        address: String,
        trusted: bool,
    },
    SetBlocked {
        address: String,
        blocked: bool,
    },
    ForgetDevice(String),
    SetAlias {
        address: String,
        alias: String,
    },
    // Adapter management
    SetAdapterPowered {
        adapter: String,
        powered: bool,
    },
    SetAdapterDiscoverable {
        adapter: String,
        discoverable: bool,
    },
    SetAdapterPairable {
        adapter: String,
        pairable: bool,
    },
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    ctx: &egui::Context,
    snapshot: &AppSnapshot,
    captured_devices: &[DeviceRecord],
    captured_at: Option<chrono::DateTime<chrono::Utc>>,
    cycle_status: Option<(f32, f32)>,
    selected_tab: &mut ViewTab,
    scan_filter: &mut ScanFilter,
    selected_device: &mut Option<String>,
    rename_draft: &mut Option<String>,
) -> Option<AppAction> {
    let mut action = None;

    egui::TopBottomPanel::top("top_bar")
        .frame(
            egui::Frame::new()
                .fill(theme::color::BG_PANEL)
                .inner_margin(egui::Margin::symmetric(
                    theme::space::XL_I,
                    theme::space::LG_I,
                ))
                .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Left cluster: logo + status dot + status text
                ui.label(
                    RichText::new("BlueTrack")
                        .size(theme::text::HEADING)
                        .color(theme::color::ACCENT)
                        .strong(),
                );
                ui.add_space(theme::space::MD);
                components::status_dot(ui, theme::state_color(!snapshot.scan_active));
                ui.label(
                    RichText::new(snapshot.status_line.as_str())
                        .size(theme::text::SMALL)
                        .color(theme::color::TEXT_MUTED),
                );
                ui.add_space(theme::space::MD);
                ui.label(
                    RichText::new(format!(
                        "· {}",
                        format_duration_since(snapshot.metrics.started_at)
                    ))
                    .size(theme::text::SMALL)
                    .color(theme::color::TEXT_FAINT),
                );

                // Right cluster: tabs + action buttons (widgets added from the
                // right edge inward, so the first `tab_button` appears rightmost).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.style_mut().spacing.item_spacing.x = theme::space::SM;
                    if components::tab_button(ui, "Log", *selected_tab == ViewTab::Log).clicked() {
                        *selected_tab = ViewTab::Log;
                    }
                    if components::tab_button(ui, "Adapter", *selected_tab == ViewTab::Adapter)
                        .clicked()
                    {
                        *selected_tab = ViewTab::Adapter;
                    }
                    if components::tab_button(ui, "Profiles", *selected_tab == ViewTab::Profiles)
                        .clicked()
                    {
                        *selected_tab = ViewTab::Profiles;
                    }
                    if components::tab_button(ui, "Radar", *selected_tab == ViewTab::Radar)
                        .clicked()
                    {
                        *selected_tab = ViewTab::Radar;
                    }
                    if components::tab_button(ui, "Scan", *selected_tab == ViewTab::Scan).clicked()
                    {
                        *selected_tab = ViewTab::Scan;
                    }

                    ui.add_space(theme::space::MD);

                    // Settings
                    if components::ghost_button(ui, "Settings", theme::color::TEXT_DIM).clicked() {
                        action = Some(AppAction::OpenSettings);
                    }

                    // Refresh
                    if components::ghost_button(ui, "Refresh", theme::color::TEXT_DIM).clicked() {
                        action = Some(AppAction::Refresh);
                    }

                    // Start / Stop passive scan — coloured by current state so the
                    // button also acts as a secondary status indicator. Disabled
                    // during a discovery cycle to avoid surprising scanner toggles
                    // that would be reverted on finalize.
                    ui.add_enabled_ui(cycle_status.is_none(), |ui| {
                        if snapshot.scan_active {
                            if components::ghost_button(ui, "Stop scan", theme::color::STALE)
                                .clicked()
                            {
                                action = Some(AppAction::StopScan);
                            }
                        } else if components::ghost_button(ui, "Start scan", theme::color::NEAR)
                            .clicked()
                        {
                            action = Some(AppAction::StartScan);
                        }
                    });
                });
            });
        });

    // Device detail panel only shown on Scan and Radar views
    if matches!(*selected_tab, ViewTab::Scan | ViewTab::Radar)
        && let Some(device_action) = egui::SidePanel::right("device_detail")
            .min_width(320.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::color::BG_PANEL)
                    .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
                    .inner_margin(egui::Margin::same(theme::space::XL_I)),
            )
            .show(ctx, |ui| {
                render_device_detail(
                    ui,
                    snapshot,
                    captured_devices,
                    selected_device,
                    rename_draft,
                )
            })
            .inner
    {
        action = Some(device_action);
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::color::BG_WINDOW)
                .inner_margin(egui::Margin::same(0)),
        )
        .show(ctx, |ui| match selected_tab {
            ViewTab::Scan => {
                if let Some(a) = render_scan_view(
                    ui,
                    captured_devices,
                    captured_at,
                    cycle_status,
                    snapshot,
                    scan_filter,
                    selected_device,
                ) {
                    action = Some(a);
                }
            }
            ViewTab::Radar => render_map_view(ui, snapshot, selected_device),
            ViewTab::Profiles => render_profiles_view(ui, snapshot, selected_tab, selected_device),
            ViewTab::Adapter => {
                if let Some(a) = render_adapter_view(ui, snapshot) {
                    action = Some(a);
                }
            }
            ViewTab::Log => render_log_view(ui, snapshot),
        });

    action
}

#[allow(clippy::too_many_arguments)]
fn render_scan_view(
    ui: &mut egui::Ui,
    captured_devices: &[DeviceRecord],
    captured_at: Option<chrono::DateTime<chrono::Utc>>,
    cycle_status: Option<(f32, f32)>,
    snapshot: &AppSnapshot,
    scan_filter: &mut ScanFilter,
    selected_device: &mut Option<String>,
) -> Option<AppAction> {
    let _ = snapshot; // scan-active hint moved to the top bar; keep for future use.
    let mut action: Option<AppAction> = None;

    // ── Filter bar: pills on the left, Scan button (or progress) on the right
    egui::Frame::new()
        .fill(theme::color::BG_PANEL)
        .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
        .inner_margin(egui::Margin::symmetric(
            theme::space::XL_I,
            theme::space::MD_I,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = theme::space::SM;

                for (variant, label, accent) in [
                    (ScanFilter::All, "All", theme::color::TEXT_DIM),
                    (ScanFilter::Near, "Near", theme::color::NEAR),
                    (ScanFilter::Mid, "Mid", theme::color::MID),
                    (ScanFilter::Far, "Far", theme::color::FAR),
                    (ScanFilter::Stale, "Stale", theme::color::STALE),
                ] {
                    if components::toggle_pill(ui, label, *scan_filter == variant, accent).clicked()
                    {
                        *scan_filter = variant;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((elapsed, total)) = cycle_status {
                        // Cancel sits on the right, progress bar to its left.
                        if components::ghost_button(ui, "Cancel", theme::color::STALE).clicked() {
                            action = Some(AppAction::CancelScanCycle);
                        }
                        ui.add_space(theme::space::SM);
                        // Guard against bad `total` values (per the resolved
                        // PR #2 review) — NaN / zero / negative collapses to
                        // a flat 0.0 fraction instead of producing inf/NaN.
                        let fraction = if total.is_finite() && total > 0.0 && elapsed.is_finite() {
                            (elapsed / total).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        ui.add(egui::ProgressBar::new(fraction).desired_width(220.0).text(
                            format!(
                                "Scanning… {} / {}",
                                format_mmss(elapsed),
                                format_mmss(total),
                            ),
                        ));
                    } else {
                        if components::primary_button(ui, "Scan for devices", theme::color::NEAR)
                            .clicked()
                        {
                            action = Some(AppAction::StartScanCycle);
                        }

                        if let Some(ts) = captured_at {
                            ui.add_space(theme::space::MD);
                            ui.label(
                                RichText::new(format!(
                                    "Captured {} · {} devices",
                                    format_relative_time(ts),
                                    captured_devices.len()
                                ))
                                .size(theme::text::SMALL)
                                .color(theme::color::TEXT_FAINT),
                            );
                        }
                    }
                });
            });
        });

    // ── Device grid ─────────────────────────────────────────────────────────
    let devices: Vec<&DeviceRecord> = captured_devices
        .iter()
        .filter(|d| match scan_filter {
            ScanFilter::All => true,
            ScanFilter::Near => matches!(d.proximity_band(), "Near"),
            ScanFilter::Mid => matches!(d.proximity_band(), "Mid"),
            ScanFilter::Far => matches!(d.proximity_band(), "Far"),
            ScanFilter::Stale => d.stale,
        })
        .collect();

    if devices.is_empty() {
        // Empty-state copy is driven by (cycle_status, captured_at,
        // captured_devices, scan_filter) so the user sees distinct messages
        // for "cycle running", "never scanned", "scan found nothing", and
        // "filter hides everything".
        let msg: &str = if cycle_status.is_some() {
            if captured_devices.is_empty() {
                "Discovery cycle in progress — results will appear here when the scan completes."
            } else {
                "Discovery cycle in progress — showing the previous capture until results are ready."
            }
        } else if captured_at.is_none() {
            "No scan captured yet. Click \"Scan for devices\" to run a discovery cycle."
        } else if captured_devices.is_empty() {
            "Scan finished — no devices found. Click \"Scan for devices\" to try again."
        } else {
            match scan_filter {
                ScanFilter::All => "No devices in the captured list.",
                ScanFilter::Near => "No devices within the Near band.",
                ScanFilter::Mid => "No devices within the Mid band.",
                ScanFilter::Far => "No devices within the Far band.",
                ScanFilter::Stale => "No stale devices.",
            }
        };

        egui::Frame::new()
            .inner_margin(egui::Margin::same(theme::space::XXL_I))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(theme::space::XXL);
                    ui.label(
                        RichText::new(msg)
                            .size(theme::text::BODY)
                            .color(theme::color::TEXT_MUTED),
                    );
                });
            });
        return action;
    }

    // Header strip
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(
            theme::space::XL_I,
            theme::space::SM_I,
        ))
        .stroke(egui::Stroke::new(
            theme::stroke::THIN,
            theme::color::BORDER.gamma_multiply(theme::alpha::BORDER_DIM),
        ))
        .show(ui, |ui| {
            ui.columns(5, |cols| {
                header_label(&mut cols[0], "Device");
                header_label(&mut cols[1], "Category");
                header_label(&mut cols[2], "RSSI");
                header_label(&mut cols[3], "Distance");
                header_label(&mut cols[4], "Band");
            });
        });

    egui::ScrollArea::vertical().show(ui, |ui| {
        for device in &devices {
            let selected = selected_device.as_deref() == Some(device.address.as_str());
            let band = device.proximity_band();
            let band_color = theme::proximity_band_color(band);

            let row_fill = if selected {
                theme::color::MID.gamma_multiply(theme::alpha::FAINT)
            } else {
                egui::Color32::TRANSPARENT
            };

            let mut frame =
                egui::Frame::new()
                    .fill(row_fill)
                    .inner_margin(egui::Margin::symmetric(
                        theme::space::XL_I,
                        theme::space::MD_I,
                    ));
            if selected {
                frame = frame.stroke(egui::Stroke::new(2.0, theme::color::MID));
            }

            let response = frame
                .show(ui, |ui| {
                    ui.columns(5, |cols| {
                        // Device column
                        let name_color = if device.stale {
                            theme::color::STALE
                        } else if device.tracker_alert.is_some() {
                            theme::color::TRACKER
                        } else if device.name.is_some() || device.alias.is_some() {
                            theme::color::TEXT_PRIMARY
                        } else {
                            theme::color::TEXT_MUTED
                        };
                        let mut name_text = RichText::new(device.display_name())
                            .size(theme::text::BODY)
                            .color(name_color);
                        if device.name.is_some() || device.alias.is_some() {
                            name_text = name_text.strong();
                        }
                        if device.stale {
                            name_text = name_text.italics();
                        }
                        cols[0].label(name_text);

                        // Category column
                        cols[1].horizontal(|ui| {
                            components::chip(
                                ui,
                                device.category.label(),
                                theme::category_color(&device.category),
                            );
                        });

                        // RSSI column
                        components::rssi_meter(&mut cols[2], device.rssi, band_color);

                        // Distance column
                        let dist_text = theme::format_distance(device.rssi);
                        let dist_color = if device.rssi.is_some() {
                            theme::color::TEXT_SECONDARY
                        } else {
                            theme::color::TEXT_FAINT
                        };
                        cols[3].label(
                            RichText::new(dist_text)
                                .size(theme::text::BODY)
                                .color(dist_color),
                        );

                        // Band column
                        cols[4].label(
                            RichText::new(band)
                                .size(theme::text::BODY)
                                .color(band_color),
                        );
                    });
                })
                .response
                .interact(egui::Sense::click());

            if response.clicked() {
                *selected_device = Some(device.address.clone());
            }
        }
    });

    action
}

fn header_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(theme::text::SMALL)
            .color(theme::color::TEXT_FAINT)
            .strong(),
    );
}

fn render_map_view(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    selected_device: &mut Option<String>,
) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(theme::space::XL_I))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Proximity Radar")
                    .size(theme::text::HEADING)
                    .color(theme::color::TEXT_PRIMARY)
                    .strong(),
            );
            ui.add_space(theme::space::MD);

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
                ui.style_mut().spacing.item_spacing.x = theme::space::SM;
                for (label, count, accent) in [
                    ("Near", near_count, theme::color::NEAR),
                    ("Mid", mid_count, theme::color::MID),
                    ("Far", far_count, theme::color::FAR),
                    ("Stale", stale_count, theme::color::STALE),
                ] {
                    components::chip(ui, &format!("{label} {count}"), accent);
                }
            });
            ui.add_space(theme::space::MD);

            render_radar_canvas(ui, snapshot, selected_device);
        });
}

fn render_radar_canvas(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    selected_device: &mut Option<String>,
) {
    // Radar canvas
    let available = ui.available_size();
    let side = available.x.min(available.y - 8.0).max(200.0);
    let (response, painter) =
        ui.allocate_painter(egui::vec2(side, side), egui::Sense::click_and_drag());
    let center = response.rect.center();
    let radius = side * 0.45;

    // Subtle grid crosshairs
    let grid_color = theme::color::ACCENT.gamma_multiply(theme::alpha::FAINT);
    painter.line_segment(
        [
            egui::pos2(center.x - radius, center.y),
            egui::pos2(center.x + radius, center.y),
        ],
        egui::Stroke::new(theme::stroke::THIN, grid_color),
    );
    painter.line_segment(
        [
            egui::pos2(center.x, center.y - radius),
            egui::pos2(center.x, center.y + radius),
        ],
        egui::Stroke::new(theme::stroke::THIN, grid_color),
    );

    // Band rings with dBm thresholds
    let rings: &[(f32, &str, egui::Color32)] = &[
        (RING_NEAR, "-58 dBm", theme::color::NEAR),
        (RING_MID, "-74 dBm", theme::color::MID),
        (RING_FAR, "-100 dBm", theme::color::FAR),
    ];
    for &(fraction, db_label, tint) in rings.iter().rev() {
        let r = radius * fraction;
        painter.circle(
            center,
            r,
            tint.gamma_multiply(theme::alpha::TRACE),
            egui::Stroke::new(
                theme::stroke::THIN,
                tint.gamma_multiply(theme::alpha::BADGE),
            ),
        );
        // dBm label at top of ring
        painter.text(
            egui::pos2(center.x + 4.0, center.y - r + 2.0),
            egui::Align2::LEFT_TOP,
            db_label,
            egui::FontId::new(theme::text::TINY, egui::FontFamily::Proportional),
            theme::color::TEXT_FAINT,
        );
    }

    // Center dot (adapter)
    painter.circle_filled(center, 4.0, theme::color::TEXT_PRIMARY);
    painter.circle_stroke(
        center,
        4.0,
        egui::Stroke::new(theme::stroke::THIN, theme::color::TEXT_MUTED),
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
            let dot_pos = egui::pos2(center.x + r * angle.cos(), center.y + r * angle.sin());

            let tint = band_color(device);
            let is_selected = selected_device.as_deref() == Some(device.address.as_str());
            let is_stale = device.stale;

            // Scale dot by stability: 3..8px for normal, bigger for selected
            let base_dot = if is_stale {
                3.0
            } else {
                3.0 + (device.stability_score() as f32 / 100.0) * 5.0
            };
            let dot_radius = if is_selected {
                base_dot + 3.0
            } else {
                base_dot
            };
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
            let is_hovered = pointer_pos.is_some_and(|pos| pos.distance(dot_pos) <= hit_radius);

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
            ui.label(format!(
                "Last seen: {}",
                format_relative_time(device.last_seen)
            ));
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

/// Format a duration in seconds as `m:ss`. Used by the discovery-cycle
/// progress timer; negative / NaN / infinite inputs collapse to `0:00`.
pub(crate) fn format_mmss(secs: f32) -> String {
    let total = if secs.is_finite() && secs > 0.0 {
        secs as u32
    } else {
        0
    };
    format!("{}:{:02}", total / 60, total % 60)
}

// Ring fractions matching proximity_band thresholds
const RING_NEAR: f32 = 0.30;
const RING_MID: f32 = 0.58;
const RING_FAR: f32 = 1.00;

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
    theme::proximity_band_color(device.proximity_band())
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
    let search_id = ui.id().with("prof_search");
    let sort_id = ui.id().with("prof_sort");
    let filter_id = ui.id().with("prof_filter");

    let mut search_text: String = ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());
    let mut sort_mode: ProfileSort = ui.data_mut(|d| d.get_temp(sort_id).unwrap_or_default());
    let mut only_trackers: bool = ui.data_mut(|d| d.get_temp(filter_id).unwrap_or(false));

    egui::Frame::new()
        .inner_margin(egui::Margin::same(theme::space::XL_I))
        .show(ui, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Profiles")
                        .size(theme::text::HEADING)
                        .color(theme::color::TEXT_PRIMARY)
                        .strong(),
                );
                ui.add_space(theme::space::MD);
                ui.label(
                    RichText::new(format!("· {} logged", snapshot.devices.len()))
                        .size(theme::text::SMALL)
                        .color(theme::color::TEXT_FAINT),
                );
            });

            ui.add_space(theme::space::MD);

            // Toolbar: search + sort pills + tracker toggle
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = theme::space::SM;
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut search_text)
                        .hint_text("Search name / address…")
                        .desired_width(220.0),
                );
                if search_resp.changed() {
                    ui.data_mut(|d| d.insert_temp(search_id, search_text.clone()));
                }

                ui.add_space(theme::space::LG);
                ui.label(
                    RichText::new("Sort:")
                        .size(theme::text::SMALL)
                        .color(theme::color::TEXT_DIM),
                );
                for (label, mode) in [
                    ("Stability", ProfileSort::Stability),
                    ("Name", ProfileSort::Name),
                    ("Last seen", ProfileSort::LastSeen),
                    ("RSSI", ProfileSort::Rssi),
                ] {
                    if components::toggle_pill(ui, label, sort_mode == mode, theme::color::MID)
                        .clicked()
                    {
                        sort_mode = mode;
                        ui.data_mut(|d| d.insert_temp(sort_id, sort_mode));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tracker_count = snapshot
                        .devices
                        .iter()
                        .filter(|d| d.tracker_alert.is_some())
                        .count();
                    if components::toggle_pill(
                        ui,
                        &format!("Trackers {tracker_count}"),
                        only_trackers,
                        theme::color::TRACKER,
                    )
                    .clicked()
                    {
                        only_trackers = !only_trackers;
                        ui.data_mut(|d| d.insert_temp(filter_id, only_trackers));
                    }
                });
            });

            ui.add_space(theme::space::LG);

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
                ProfileSort::Stability => devices.sort_by(|a, b| {
                    b.stability_score()
                        .cmp(&a.stability_score())
                        .then_with(|| b.first_seen.cmp(&a.first_seen))
                        .then_with(|| a.address.cmp(&b.address))
                }),
                ProfileSort::Name => devices.sort_by(|a, b| a.display_name().cmp(b.display_name())),
                ProfileSort::LastSeen => devices.sort_by(|a, b| b.last_seen.cmp(&a.last_seen)),
                ProfileSort::Rssi => devices.sort_by(|a, b| {
                    let ra = a.rssi.unwrap_or(i16::MIN);
                    let rb = b.rssi.unwrap_or(i16::MIN);
                    rb.cmp(&ra)
                }),
            }

            if devices.is_empty() {
                ui.add_space(theme::space::XXL);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(if snapshot.devices.is_empty() {
                            "No devices logged yet."
                        } else {
                            "No devices match the current filter."
                        })
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_MUTED),
                    );
                });
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing.y = theme::space::MD;
                    for device in devices {
                        if let Some(goto) = render_profile_card(ui, device) {
                            *selected_device = Some(goto);
                            *selected_tab = ViewTab::Scan;
                        }
                    }
                });
        });
}

/// Renders a vertical device profile card. Returns the device address if clicked.
fn render_profile_card(ui: &mut egui::Ui, device: &DeviceRecord) -> Option<String> {
    let is_tracker = device.tracker_alert.is_some();
    let mut goto: Option<String> = None;

    let card_fn = |ui: &mut egui::Ui| {
        let dot_color = if is_tracker {
            theme::color::TRACKER
        } else if device.stale {
            theme::color::STALE
        } else {
            theme::proximity_band_color(device.proximity_band())
        };

        // Row 1: dot + name + category chip
        ui.horizontal(|ui| {
            components::status_dot(ui, dot_color);
            ui.add_space(theme::space::XS);
            let name_color = if is_tracker {
                theme::color::TRACKER
            } else {
                theme::color::TEXT_PRIMARY
            };
            let name_display = if is_tracker {
                format!("▲ {}", device.display_name())
            } else {
                device.display_name().to_string()
            };
            ui.label(
                RichText::new(truncate_name(&name_display, 30))
                    .size(theme::text::BODY)
                    .color(name_color)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                components::chip(
                    ui,
                    device.category.label(),
                    theme::category_color(&device.category),
                );
            });
        });

        // Row 2: MAC + meta
        ui.add_space(theme::space::XS);
        let rssi_str = device
            .rssi
            .map(|v| format!("{v} dBm"))
            .unwrap_or_else(|| "—".to_string());
        let meta = format!(
            "{} · Last seen {} · {}",
            device.address,
            format_relative_time(device.last_seen),
            rssi_str
        );
        ui.label(
            RichText::new(meta)
                .size(theme::text::SMALL)
                .color(theme::color::TEXT_DIM),
        );

        // Interaction: full-card click
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
    };

    if is_tracker {
        components::card_accent(ui, theme::color::TRACKER, card_fn);
    } else {
        components::card(ui, card_fn);
    }

    goto
}

fn render_device_detail(
    ui: &mut egui::Ui,
    snapshot: &AppSnapshot,
    captured_devices: &[DeviceRecord],
    selected_device: &mut Option<String>,
    rename_draft: &mut Option<String>,
) -> Option<AppAction> {
    let mut action: Option<AppAction> = None;

    // Resolve the selected device. Look in the live snapshot first (so we show
    // fresh RSSI / state), then fall back to the captured list (so the user can
    // still inspect a device they selected from the static Scan grid even after
    // the backend has purged it). Never silently reassign `*selected_device` —
    // selection is user-owned.
    let Some(selected_addr) = selected_device.as_deref().map(str::to_string) else {
        ui.add_space(theme::space::XXL);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("No device selected")
                    .size(theme::text::BODY)
                    .color(theme::color::TEXT_MUTED),
            );
            ui.add_space(theme::space::SM);
            ui.label(
                RichText::new("Click a row in the Scan or Radar view.")
                    .size(theme::text::SMALL)
                    .color(theme::color::TEXT_FAINT),
            );
        });
        return action;
    };

    let live_device = snapshot
        .devices
        .iter()
        .find(|device| device.address == selected_addr);
    let (device, is_stale_capture) = match live_device {
        Some(device) => (device, false),
        None => match captured_devices
            .iter()
            .find(|device| device.address == selected_addr)
        {
            Some(device) => (device, true),
            None => {
                // Vanished from both lists — show an explicit error rather
                // than silently picking another device.
                ui.add_space(theme::space::LG);
                components::alert_banner(ui, theme::color::ERROR, |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Selected device is no longer available")
                                .size(theme::text::BODY)
                                .color(theme::color::ERROR)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(&selected_addr)
                                .monospace()
                                .size(theme::text::MONO)
                                .color(theme::color::TEXT_SECONDARY),
                        );
                        ui.label(
                            RichText::new(
                                "Purged from the live scan and not in the captured list. \
                                 Select another device from the Scan view.",
                            )
                            .size(theme::text::SMALL)
                            .color(theme::color::TEXT_MUTED),
                        );
                    });
                });
                return action;
            }
        },
    };

    egui::ScrollArea::vertical()
        .id_salt("device_detail_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.style_mut().spacing.item_spacing.y = theme::space::LG;

            if is_stale_capture {
                components::alert_banner(ui, theme::color::STALE, |ui| {
                    ui.label(
                        RichText::new("Showing captured data — device is no longer live")
                            .size(theme::text::SMALL)
                            .color(theme::color::STALE),
                    );
                });
            }

            // Tracker banner (conditional)
            if let Some(alert) = &device.tracker_alert {
                let confidence_str = match alert.confidence {
                    TrackerConfidence::Definite => "CONFIRMED",
                    TrackerConfidence::Likely => "LIKELY",
                };
                components::alert_banner(ui, theme::color::TRACKER, |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(format!("▲ TRACKER [{confidence_str}]"))
                                .size(theme::text::SMALL)
                                .color(theme::color::TRACKER)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(&alert.name)
                                .size(theme::text::BODY)
                                .color(theme::color::TRACKER),
                        );
                        ui.label(
                            RichText::new(&alert.detail)
                                .size(theme::text::SMALL)
                                .color(theme::color::TRACKER.gamma_multiply(0.75)),
                        );
                    });
                });
            }

            render_detail_header(ui, device);
            render_detail_section_status(ui, device);
            if let Some(a) = render_detail_section_manage(ui, device, rename_draft) {
                action = Some(a);
            }
            render_detail_section_signal(ui, device);
            render_detail_section_traits(ui, device);
            render_detail_section_advertising(ui, device);

            if device.beacon.is_some() {
                render_detail_section_beacon(ui, device);
            }

            let has_gatt = device.gatt_manufacturer.is_some()
                || device.gatt_model.is_some()
                || device.gatt_firmware.is_some()
                || device.gatt_hardware.is_some();
            if has_gatt {
                render_detail_section_gatt(ui, device);
            }

            if let Some(error) = &snapshot.last_error {
                ui.add_space(theme::space::MD);
                components::alert_banner(ui, theme::color::ERROR, |ui| {
                    ui.label(
                        RichText::new(error)
                            .size(theme::text::SMALL)
                            .color(theme::color::ERROR),
                    );
                });
            }
        });

    action
}

fn render_detail_header(ui: &mut egui::Ui, device: &DeviceRecord) {
    ui.horizontal(|ui| {
        // Device icon: rounded square tinted with category color
        let icon_size = egui::Vec2::splat(36.0);
        let (rect, _) = ui.allocate_exact_size(icon_size, egui::Sense::hover());
        let cat_color = theme::category_color(&device.category);
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme::radius::LG as u8),
            cat_color.gamma_multiply(theme::alpha::BADGE),
        );
        let glyph = device_glyph(&device.category);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(18.0),
            cat_color,
        );

        ui.add_space(theme::space::MD);

        ui.vertical(|ui| {
            ui.label(
                RichText::new(device.display_name())
                    .size(theme::text::SUBHEADING)
                    .color(theme::color::TEXT_PRIMARY)
                    .strong(),
            );
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    device.address,
                    device.address_type_detail.label()
                ))
                .size(theme::text::TINY)
                .color(theme::color::TEXT_MUTED),
            );
        });
    });

    ui.horizontal(|ui| {
        components::chip(
            ui,
            device.category.label(),
            theme::category_color(&device.category),
        );
        if let Some(apple_type) = &device.apple_device_type {
            ui.add_space(theme::space::XS);
            ui.label(
                RichText::new(apple_type)
                    .size(theme::text::TINY)
                    .color(theme::color::TEXT_MUTED),
            );
        }
    });
}

fn render_detail_section_status(ui: &mut egui::Ui, device: &DeviceRecord) {
    components::section(ui, "Status", |ui| {
        let state_label = if device.stale {
            "● Stale"
        } else {
            "● Active"
        };
        let state_color = theme::state_color(device.stale);
        components::kv_row_colored(ui, "State", state_label, state_color);
        components::kv_row_mono(
            ui,
            "First seen",
            &device
                .first_seen
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string(),
        );
        components::kv_row(
            ui,
            "Last seen",
            RichText::new(format_relative_time(device.last_seen))
                .size(theme::text::MONO)
                .color(theme::color::TEXT_SECONDARY),
        );
        components::kv_row(ui, "Adapter", device.adapter_name.as_str());
        components::kv_row(ui, "Seen count", format!("{}", device.seen_count));
    });
}

fn render_detail_section_manage(
    ui: &mut egui::Ui,
    device: &DeviceRecord,
    rename_draft: &mut Option<String>,
) -> Option<AppAction> {
    let mut action = None;
    let addr = device.address.clone();
    components::section(ui, "Manage", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().spacing.item_spacing.x = theme::space::SM;

            // Connect / Disconnect
            if device.connected {
                if components::ghost_button(ui, "Disconnect", theme::color::STALE).clicked() {
                    action = Some(AppAction::DisconnectDevice(addr.clone()));
                }
            } else if components::ghost_button(ui, "Connect", theme::color::NEAR).clicked() {
                action = Some(AppAction::ConnectDevice(addr.clone()));
            }

            // Trust / Untrust
            let (trust_label, trust_action_trusted) = if device.trusted {
                ("Untrust", false)
            } else {
                ("Trust", true)
            };
            if components::ghost_button(ui, trust_label, theme::color::MID).clicked() {
                action = Some(AppAction::SetTrusted {
                    address: addr.clone(),
                    trusted: trust_action_trusted,
                });
            }

            // Block / Unblock
            let (block_label, block_action_blocked) = if device.blocked {
                ("Unblock", false)
            } else {
                ("Block", true)
            };
            if components::ghost_button(ui, block_label, theme::color::ERROR).clicked() {
                action = Some(AppAction::SetBlocked {
                    address: addr.clone(),
                    blocked: block_action_blocked,
                });
            }
        });

        ui.add_space(theme::space::XS);

        // Forget / Rename row
        ui.horizontal(|ui| {
            if let Some(draft) = rename_draft.as_mut() {
                let response = ui.add(
                    egui::TextEdit::singleline(draft)
                        .desired_width(140.0)
                        .hint_text("Alias…"),
                );
                let submitted =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if components::ghost_button(ui, "Apply", theme::color::MID).clicked() || submitted {
                    action = Some(AppAction::SetAlias {
                        address: addr.clone(),
                        alias: draft.trim().to_string(),
                    });
                    *rename_draft = None;
                }
                if components::ghost_button(ui, "Cancel", theme::color::TEXT_DIM).clicked() {
                    *rename_draft = None;
                }
            } else if components::ghost_button(ui, "Rename", theme::color::ACCENT).clicked() {
                let current = device
                    .alias
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(device.name.as_deref().unwrap_or(""));
                *rename_draft = Some(current.to_string());
            }
        });

        ui.add_space(theme::space::XS);
        if components::danger_button(ui, "Forget device").clicked() {
            action = Some(AppAction::ForgetDevice(addr.clone()));
        }
    });
    action
}

fn render_detail_section_signal(ui: &mut egui::Ui, device: &DeviceRecord) {
    components::section(ui, "Signal", |ui| {
        components::kv_row(
            ui,
            "RSSI",
            device
                .rssi
                .map(|v| format!("{v} dBm"))
                .unwrap_or_else(|| "—".to_string()),
        );
        if let Some(avg) = device.avg_rssi() {
            components::kv_row(ui, "Avg RSSI", format!("{avg:.1} dBm"));
        }
        if let Some(tx) = device.tx_power {
            components::kv_row(ui, "TX Power", format!("{tx} dBm"));
        }
        components::kv_row(ui, "Distance", theme::format_distance(device.rssi));
        components::kv_row_colored(
            ui,
            "Band",
            device.proximity_band(),
            theme::proximity_band_color(device.proximity_band()),
        );
        components::kv_row(ui, "Stability", format!("{}%", device.stability_score()));
        if let Some(battery) = device.battery_percentage {
            components::kv_row(ui, "Battery", format!("{battery}%"));
        }
    });
}

fn render_detail_section_traits(ui: &mut egui::Ui, device: &DeviceRecord) {
    components::section(ui, "Traits", |ui| {
        components::kv_row(ui, "Address type", device.address_type_detail.label());
        components::kv_row(ui, "Paired", yes_no(device.paired));
        components::kv_row(ui, "Trusted", yes_no(device.trusted));
        components::kv_row(ui, "Connected", yes_no(device.connected));
        if device.blocked {
            components::kv_row_colored(ui, "Blocked", "Yes", theme::color::ERROR);
        }
        components::kv_row(ui, "Services resolved", yes_no(device.services_resolved));
    });
}

fn render_detail_section_advertising(ui: &mut egui::Ui, device: &DeviceRecord) {
    components::section(ui, "Advertising", |ui| {
        let mfr = device
            .gatt_manufacturer
            .as_deref()
            .or(device.oui_manufacturer.as_deref());
        if let Some(mfr) = mfr {
            let suffix = if device.gatt_manufacturer.is_some() {
                ""
            } else {
                " (OUI)"
            };
            components::kv_row(ui, "Manufacturer", format!("{mfr}{suffix}"));
        }
        if let Some(name) = &device.name {
            components::kv_row_mono(ui, "Local name", name);
        }
        if !device.uuids.is_empty() {
            let joined = device
                .uuids
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            components::kv_row_mono(ui, "Service UUIDs", &joined);
        }
        if !device.manufacturer_data.is_empty() {
            let preview = device
                .manufacturer_data
                .iter()
                .take(2)
                .map(|e| format!("0x{:04X}", e.id))
                .collect::<Vec<_>>()
                .join(", ");
            components::kv_row_mono(ui, "Mfr data", &preview);
        }
        if let Some(interval) = crate::intelligence::profile::estimate_adv_interval_ms(device) {
            let label = if interval < 1000 {
                format!("~{interval} ms")
            } else {
                format!("~{:.1} s", interval as f32 / 1000.0)
            };
            components::kv_row(ui, "Adv interval", label);
        }
    });
}

fn render_detail_section_beacon(ui: &mut egui::Ui, device: &DeviceRecord) {
    let Some(beacon) = &device.beacon else {
        return;
    };
    components::section(ui, "Beacon", |ui| {
        components::kv_row(ui, "Format", beacon.format.label());
        if let Some(uuid) = &beacon.uuid {
            components::kv_row_mono(ui, "UUID", uuid);
        }
        if let Some(major) = beacon.major {
            components::kv_row(ui, "Major", format!("{major}"));
        }
        if let Some(minor) = beacon.minor {
            components::kv_row(ui, "Minor", format!("{minor}"));
        }
        if let Some(ns) = &beacon.namespace {
            components::kv_row_mono(ui, "Namespace", ns);
        }
        if let Some(inst) = &beacon.instance {
            components::kv_row_mono(ui, "Instance", inst);
        }
        if let Some(url) = &beacon.url {
            components::kv_row_mono(ui, "URL", url);
        }
        if let Some(dbm) = beacon.calibrated_power_dbm {
            components::kv_row(ui, "TX @ 1 m", format!("{dbm} dBm"));
        }
        if let Some(dist) = beacon.estimated_distance_m {
            let label = if dist < 1.0 {
                format!("{dist:.2} m")
            } else {
                format!("{dist:.1} m")
            };
            components::kv_row(ui, "Est. distance", label);
        }
        if let Some(mv) = beacon.battery_mv {
            components::kv_row(ui, "Battery", format!("{mv} mV"));
        }
        if let Some(temp) = beacon.temperature_c {
            components::kv_row(ui, "Temperature", format!("{temp:.1} °C"));
        }
    });
}

fn render_detail_section_gatt(ui: &mut egui::Ui, device: &DeviceRecord) {
    components::section(ui, "GATT Device Info", |ui| {
        if let Some(v) = &device.gatt_manufacturer {
            components::kv_row(ui, "Manufacturer", v.as_str());
        }
        if let Some(v) = &device.gatt_model {
            components::kv_row(ui, "Model", v.as_str());
        }
        if let Some(v) = &device.gatt_firmware {
            components::kv_row_mono(ui, "Firmware", v);
        }
        if let Some(v) = &device.gatt_hardware {
            components::kv_row_mono(ui, "Hardware", v);
        }
    });
}

fn device_glyph(category: &crate::model::DeviceCategory) -> &'static str {
    use crate::model::DeviceCategory;
    match category {
        DeviceCategory::Phone | DeviceCategory::Tablet => "📱",
        DeviceCategory::Computer => "💻",
        DeviceCategory::Headphone | DeviceCategory::Headset => "🎧",
        DeviceCategory::Speaker => "🔊",
        DeviceCategory::Wearable => "⌚",
        DeviceCategory::FitnessTracker => "🏃",
        DeviceCategory::HealthSensor => "❤",
        DeviceCategory::InputDevice => "⌨",
        DeviceCategory::Beacon => "📡",
        DeviceCategory::Tracker => "▲",
        DeviceCategory::NetworkDevice => "📶",
        DeviceCategory::Printer => "🖨",
        DeviceCategory::Vehicle => "🚗",
        DeviceCategory::SmartHome => "🏠",
        DeviceCategory::Unknown => "?",
    }
}

fn render_adapter_view(ui: &mut egui::Ui, snapshot: &AppSnapshot) -> Option<AppAction> {
    let mut action: Option<AppAction> = None;

    egui::Frame::new()
        .inner_margin(egui::Margin::same(theme::space::XL_I))
        .show(ui, |ui| {
            ui.heading(
                RichText::new("Adapter")
                    .size(theme::text::HEADING)
                    .color(theme::color::TEXT_PRIMARY),
            );
            ui.add_space(theme::space::MD);

            if snapshot.adapters.is_empty() {
                ui.add_space(theme::space::XXL);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("No Bluetooth adapters found.")
                            .size(theme::text::BODY)
                            .color(theme::color::TEXT_MUTED),
                    );
                });
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing.y = theme::space::LG;
                    for adapter in &snapshot.adapters {
                        components::card(ui, |ui| {
                            // Header row: dot + alias + model
                            ui.horizontal(|ui| {
                                let dot_color = theme::state_color(!adapter.powered);
                                components::status_dot(ui, dot_color);
                                ui.add_space(theme::space::XS);
                                ui.label(
                                    RichText::new(&adapter.alias)
                                        .size(theme::text::SUBHEADING)
                                        .color(theme::color::TEXT_PRIMARY)
                                        .strong(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(&adapter.name)
                                                .size(theme::text::SMALL)
                                                .color(theme::color::TEXT_MUTED),
                                        );
                                    },
                                );
                            });

                            ui.add_space(theme::space::MD);

                            // KV rows
                            if let Some(addr) = &adapter.address {
                                components::kv_row_mono(ui, "Address", addr);
                            }
                            if let Some(addr_type) = &adapter.address_type {
                                components::kv_row(ui, "Address type", addr_type.as_str());
                            }

                            ui.add_space(theme::space::MD);
                            components::hairline(ui);

                            // Toggle switches
                            toggle_switch_row(ui, "Powered", adapter.powered, |on| {
                                action = Some(AppAction::SetAdapterPowered {
                                    adapter: adapter.name.clone(),
                                    powered: on,
                                });
                            });
                            toggle_switch_row(ui, "Discoverable", adapter.discoverable, |on| {
                                action = Some(AppAction::SetAdapterDiscoverable {
                                    adapter: adapter.name.clone(),
                                    discoverable: on,
                                });
                            });
                            toggle_switch_row(ui, "Pairable", adapter.pairable, |on| {
                                action = Some(AppAction::SetAdapterPairable {
                                    adapter: adapter.name.clone(),
                                    pairable: on,
                                });
                            });

                            // Discovering is read-only
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Discovering")
                                        .size(theme::text::BODY)
                                        .color(theme::color::TEXT_DIM),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let (label, color) = if adapter.discovering {
                                            ("Active", theme::color::NEAR)
                                        } else {
                                            ("Idle", theme::color::TEXT_FAINT)
                                        };
                                        ui.label(
                                            RichText::new(label)
                                                .size(theme::text::BODY)
                                                .color(color),
                                        );
                                    },
                                );
                            });
                        });
                    }
                });
        });

    action
}

fn toggle_switch_row(ui: &mut egui::Ui, label: &str, on: bool, mut on_toggle: impl FnMut(bool)) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(theme::text::BODY)
                .color(theme::color::TEXT_SECONDARY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::toggle_switch(ui, on).clicked() {
                on_toggle(!on);
            }
        });
    });
}

fn render_log_view(ui: &mut egui::Ui, snapshot: &AppSnapshot) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(theme::space::XL_I))
        .show(ui, |ui| {
            ui.heading(
                RichText::new("Event Log")
                    .size(theme::text::HEADING)
                    .color(theme::color::TEXT_PRIMARY),
            );
            ui.add_space(theme::space::MD);

            if snapshot.event_log.is_empty() {
                ui.add_space(theme::space::XXL);
                ui.label(
                    RichText::new("No events recorded.")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_MUTED),
                );
                return;
            }

            egui::Frame::new()
                .fill(theme::color::BG_EXTREME)
                .corner_radius(egui::CornerRadius::same(theme::radius::LG as u8))
                .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
                .inner_margin(egui::Margin::same(theme::space::LG_I))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().spacing.item_spacing.y = theme::space::XS;
                            for entry in &snapshot.event_log {
                                let level_color = theme::log_level_color(&entry.level);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(
                                            entry
                                                .at
                                                .with_timezone(&chrono::Local)
                                                .format("%H:%M:%S")
                                                .to_string(),
                                        )
                                        .monospace()
                                        .size(theme::text::MONO)
                                        .color(theme::color::TEXT_FAINT),
                                    );
                                    ui.label(
                                        RichText::new(entry.level.label())
                                            .monospace()
                                            .size(theme::text::MONO)
                                            .color(level_color)
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(entry.message.as_str())
                                            .monospace()
                                            .size(theme::text::MONO)
                                            .color(theme::color::TEXT_SECONDARY),
                                    );
                                });
                            }
                        });
                });
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
        assert_eq!(band_color(&d), theme::color::NEAR);
    }

    #[test]
    fn band_color_mid() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-65);
        assert_eq!(band_color(&d), theme::color::MID);
    }

    #[test]
    fn band_color_far() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.rssi = Some(-80);
        assert_eq!(band_color(&d), theme::color::FAR);
    }

    #[test]
    fn band_color_stale() {
        let mut d = make_device("AA:00:00:00:00:01");
        d.mark_stale();
        assert_eq!(band_color(&d), theme::color::STALE);
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
        assert_eq!(band_color(&d), theme::color::TEXT_MUTED);
    }
}
