//! Reusable visual primitives for BlueTrack.
//!
//! Every widget style — cards, chips, buttons, key-value rows, toggles — lives
//! here and consumes tokens from [`super::theme`]. Rendering code should never
//! hand-roll `Frame::new().fill(...).stroke(...)` with hex literals.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, Pos2, Rect, Response, RichText, Sense, Stroke,
    Ui, Vec2, WidgetText,
};

use super::theme::{alpha, color, radius, space, stroke, text};

// ─────────────────────────────────────────────────────────────────────────────
// Containers
// ─────────────────────────────────────────────────────────────────────────────

/// A plain surface card: dark fill, subtle border, comfortable inner padding.
pub fn card<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(color::BG_SURFACE_2)
        .corner_radius(CornerRadius::same(radius::LG as u8))
        .stroke(Stroke::new(stroke::THIN, color::BORDER))
        .inner_margin(Margin::same(space::LG_I))
        .show(ui, body)
        .inner
}

/// Card with an accent border (used for tracker-highlighted profile cards).
pub fn card_accent<R>(ui: &mut Ui, accent: Color32, body: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(color::BG_SURFACE_1)
        .corner_radius(CornerRadius::same(radius::XL as u8))
        .stroke(Stroke::new(
            stroke::REGULAR,
            accent.gamma_multiply(alpha::BORDER_DIM),
        ))
        .inner_margin(Margin::same(space::LG_I))
        .show(ui, body)
        .inner
}

/// A titled sub-section used inside the device detail panel.
///
/// Renders a dark uppercase title strip on top of a bordered body, matching
/// the wireframe's `.section` / `.section-title` / `.section-body` idiom.
pub fn section<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(color::BG_SURFACE_2)
        .corner_radius(CornerRadius::same(radius::LG as u8))
        .stroke(Stroke::new(stroke::THIN, color::BORDER))
        .show(ui, |ui| {
            ui.style_mut().spacing.item_spacing = Vec2::new(space::MD, 0.0);

            // Title strip
            Frame::new()
                .fill(color::BG_EXTREME)
                .inner_margin(Margin::symmetric(space::LG_I, space::MD_I))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(title.to_uppercase())
                                .size(text::SMALL)
                                .color(color::TEXT_FAINT)
                                .strong(),
                        );
                    });
                });

            // Body
            Frame::new()
                .inner_margin(Margin::symmetric(space::LG_I, space::MD_I))
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing = Vec2::new(space::MD, space::SM);
                    body(ui)
                })
                .inner
        })
        .inner
}

/// A tinted banner used for high-urgency alerts (tracker notifications).
pub fn alert_banner(ui: &mut Ui, accent: Color32, body: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(accent.gamma_multiply(alpha::FAINT))
        .corner_radius(CornerRadius::same(radius::MD as u8))
        .stroke(Stroke::new(
            stroke::THIN,
            accent.gamma_multiply(alpha::BORDER_DIM),
        ))
        .inner_margin(Margin::symmetric(space::LG_I, space::MD_I))
        .show(ui, body);
}

/// Thin horizontal separator with consistent vertical breathing room.
pub fn hairline(ui: &mut Ui) {
    ui.add_space(space::SM);
    ui.separator();
    ui.add_space(space::SM);
}

// ─────────────────────────────────────────────────────────────────────────────
// Atoms
// ─────────────────────────────────────────────────────────────────────────────

/// A small colored badge with rounded corners, used for category / state tags.
pub fn chip(ui: &mut Ui, label: &str, accent: Color32) {
    Frame::new()
        .fill(accent.gamma_multiply(alpha::BADGE))
        .corner_radius(CornerRadius::same(radius::PILL as u8))
        .stroke(Stroke::NONE)
        .inner_margin(Margin::symmetric(space::MD_I, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(text::TINY)
                    .color(accent)
                    .strong(),
            );
        });
}

/// Solid colored dot — used for status indicators on cards and adapters.
pub fn status_dot(ui: &mut Ui, accent: Color32) {
    let size = Vec2::splat(10.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 4.0, accent);
}

/// Render a key / value row with the key on the left (dim) and value on the
/// right (aligned to the right edge of the parent).
pub fn kv_row(ui: &mut Ui, key: &str, value: impl Into<WidgetText>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(key)
                .size(text::BODY)
                .color(color::TEXT_DIM),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(value.into());
        });
    });
}

/// `kv_row` with the value rendered in a specific color (e.g. `● Active` in green).
pub fn kv_row_colored(ui: &mut Ui, key: &str, value: &str, value_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(key)
                .size(text::BODY)
                .color(color::TEXT_DIM),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .size(text::BODY)
                    .color(value_color),
            );
        });
    });
}

/// `kv_row` with the value rendered in monospace — used for addresses, UUIDs, hex.
pub fn kv_row_mono(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(key)
                .size(text::BODY)
                .color(color::TEXT_DIM),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .monospace()
                    .size(text::MONO)
                    .color(color::TEXT_SECONDARY),
            );
        });
    });
}

/// `kv_row` that collapses to nothing when the value is `None`.
pub fn kv_row_opt(ui: &mut Ui, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        kv_row(ui, key, v);
    }
}

/// Render an RSSI value with a small inline bar tinted to match the accent
/// color. The bar length scales linearly with signal strength.
pub fn rssi_meter(ui: &mut Ui, rssi: Option<i16>, accent: Color32) {
    match rssi {
        Some(value) => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{value} dBm"))
                        .size(text::BODY)
                        .color(color::TEXT_PRIMARY),
                );
                // Map RSSI -> [0.0, 1.0]. -30 dBm (very strong) = 1.0, -100 dBm = 0.0.
                let strength = ((value as f32 + 100.0) / 70.0).clamp(0.0, 1.0);
                let bar_w = 60.0 * strength;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 4.0), Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(bar_w, 4.0)),
                    CornerRadius::same(2),
                    accent,
                );
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(2),
                    Stroke::new(
                        stroke::THIN,
                        color::BORDER.gamma_multiply(alpha::BORDER_DIM),
                    ),
                    egui::StrokeKind::Inside,
                );
            });
        }
        None => {
            ui.label(
                RichText::new("—")
                    .size(text::BODY)
                    .color(color::TEXT_FAINT),
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Buttons
// ─────────────────────────────────────────────────────────────────────────────

/// Primary filled button, used for the top-right Scan action.
pub fn primary_button(ui: &mut Ui, label: &str, accent: Color32) -> Response {
    let button = egui::Button::new(
        RichText::new(label)
            .size(text::BUTTON)
            .color(color::BG_PANEL)
            .strong(),
    )
    .fill(accent)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(radius::MD as u8));
    ui.add(button)
}

/// Ghost (outline) button: transparent fill, colored border, colored label.
/// Used for the Manage buttons (Connect / Trust / Block).
pub fn ghost_button(ui: &mut Ui, label: &str, accent: Color32) -> Response {
    let button = egui::Button::new(
        RichText::new(label)
            .size(text::BUTTON)
            .color(accent)
            .strong(),
    )
    .fill(Color32::TRANSPARENT)
    .stroke(Stroke::new(stroke::THIN, accent))
    .corner_radius(CornerRadius::same(radius::MD as u8));
    ui.add(button)
}

/// Destructive button: red tint background + red border, full-width friendly.
pub fn danger_button(ui: &mut Ui, label: &str) -> Response {
    let accent = color::ERROR;
    let button = egui::Button::new(
        RichText::new(label)
            .size(text::BUTTON)
            .color(accent)
            .strong(),
    )
    .fill(accent.gamma_multiply(alpha::FAINT))
    .stroke(Stroke::new(stroke::THIN, accent))
    .corner_radius(CornerRadius::same(radius::MD as u8));
    ui.add(button)
}

/// Toggleable pill button — the unified style for filter, sort, and trait pills.
/// Active: filled with `accent`, label in panel background. Inactive: border only.
pub fn toggle_pill(ui: &mut Ui, label: &str, active: bool, accent: Color32) -> Response {
    let (fill, stroke_width, label_color) = if active {
        (accent, stroke::THIN, color::BG_PANEL)
    } else {
        (
            Color32::TRANSPARENT,
            stroke::THIN,
            color::TEXT_DIM,
        )
    };
    let border = if active {
        accent
    } else {
        color::BORDER
    };
    let button = egui::Button::new(
        RichText::new(label)
            .size(text::TINY)
            .color(label_color)
            .strong(),
    )
    .fill(fill)
    .stroke(Stroke::new(stroke_width, border))
    .corner_radius(CornerRadius::same(radius::PILL as u8));
    ui.add(button)
}

/// Top-bar tab button. Active: filled MID background; inactive: border-only.
pub fn tab_button(ui: &mut Ui, label: &str, active: bool) -> Response {
    let (fill, border, label_color) = if active {
        (color::MID, color::MID, color::BG_PANEL)
    } else {
        (Color32::TRANSPARENT, color::BORDER, color::TEXT_DIM)
    };
    let button = egui::Button::new(
        RichText::new(label)
            .size(text::BUTTON)
            .color(label_color)
            .strong(),
    )
    .fill(fill)
    .stroke(Stroke::new(stroke::THIN, border))
    .corner_radius(CornerRadius::same(radius::MD as u8));
    ui.add(button)
}

/// Animated slide toggle switch. Returns a Response whose `.clicked()` indicates
/// the user wants to flip the value. Renders a 28×16 pill with a moving knob.
pub fn toggle_switch(ui: &mut Ui, on: bool) -> Response {
    let size = Vec2::new(28.0, 16.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();

    let track_color = if on { color::NEAR } else { color::BORDER };
    painter.rect_filled(rect, CornerRadius::same(8), track_color);

    let knob_x = if on {
        rect.max.x - 8.0
    } else {
        rect.min.x + 8.0
    };
    let knob_center = Pos2::new(knob_x, rect.center().y);
    painter.circle_filled(knob_center, 6.0, color::TEXT_PRIMARY);

    response
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // Components rely on `egui::Ui`, which is hard to construct in unit tests
    // without a full context. The render-level integration is verified by the
    // top-level UI tests in `src/ui/mod.rs`.

    #[test]
    fn module_compiles() {
        // Marker test so `cargo test` executes the module.
    }
}
