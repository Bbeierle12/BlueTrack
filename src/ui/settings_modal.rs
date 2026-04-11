use bluer::DiscoveryTransport;
use eframe::egui::{self, RichText};

use crate::{
    model::AdapterStatus,
    settings::{Settings, ValidationErrors},
    ui::{components, theme},
};

pub struct SettingsModal {
    pub draft: Settings,
    pub errors: ValidationErrors,
    pub save_error: Option<String>,
    dirty: bool,
}

pub enum SettingsModalResult {
    Open,
    Saved(Settings),
    Cancelled,
}

impl SettingsModal {
    pub fn new(current: &Settings) -> Self {
        Self {
            draft: current.clone(),
            errors: ValidationErrors::default(),
            save_error: None,
            dirty: false,
        }
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        adapters: &[AdapterStatus],
    ) -> SettingsModalResult {
        let mut result = SettingsModalResult::Open;
        let mut keep_open = true;

        egui::Window::new("Scanner Settings")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .open(&mut keep_open)
            .show(ctx, |ui| {
                result = self.render_contents(ui, adapters);
            });

        if !keep_open {
            return SettingsModalResult::Cancelled;
        }

        result
    }

    fn render_contents(
        &mut self,
        ui: &mut egui::Ui,
        adapters: &[AdapterStatus],
    ) -> SettingsModalResult {
        ui.label(
            RichText::new("Discovery")
                .size(theme::text::SUBHEADING)
                .color(theme::color::TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(theme::space::MD);

        let before = self.draft.clone();

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([theme::space::XL, theme::space::MD])
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Preferred adapter")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                egui::ComboBox::from_id_salt("adapter_select")
                    .selected_text(self.draft.selected_adapter.as_deref().unwrap_or("Default"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.draft.selected_adapter, None, "Default");
                        for adapter in adapters {
                            ui.selectable_value(
                                &mut self.draft.selected_adapter,
                                Some(adapter.name.clone()),
                                format!("{} ({})", adapter.alias, adapter.name),
                            );
                        }
                    });
                ui.end_row();

                ui.label(
                    RichText::new("Transport")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.horizontal(|ui| {
                    ui.style_mut().spacing.item_spacing.x = theme::space::SM;
                    for (label, value) in [
                        ("Auto", DiscoveryTransport::Auto),
                        ("LE", DiscoveryTransport::Le),
                        ("BR/EDR", DiscoveryTransport::BrEdr),
                    ] {
                        if components::toggle_pill(
                            ui,
                            label,
                            self.draft.scan_transport == value,
                            theme::color::MID,
                        )
                        .clicked()
                        {
                            self.draft.scan_transport = value;
                        }
                    }
                });
                ui.end_row();

                ui.label(
                    RichText::new("Auto-start scan")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                if components::toggle_switch(ui, self.draft.auto_start_scan).clicked() {
                    self.draft.auto_start_scan = !self.draft.auto_start_scan;
                }
                ui.end_row();

                ui.label(
                    RichText::new("Duplicate adv data")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                if components::toggle_switch(ui, self.draft.allow_duplicate_data).clicked() {
                    self.draft.allow_duplicate_data = !self.draft.allow_duplicate_data;
                }
                ui.end_row();

                ui.label(
                    RichText::new("Stale timeout")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.add(
                    egui::DragValue::new(&mut self.draft.stale_after_seconds)
                        .range(1..=600)
                        .suffix(" s"),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Refresh cadence")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.add(
                    egui::DragValue::new(&mut self.draft.refresh_interval_seconds)
                        .range(1..=120)
                        .suffix(" s"),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Minimum RSSI")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.add(egui::Slider::new(&mut self.draft.min_rssi, -110..=0).suffix(" dBm"));
                ui.end_row();

                ui.label(
                    RichText::new("Retention")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.add(
                    egui::DragValue::new(&mut self.draft.retention_days)
                        .range(1..=365)
                        .suffix(" days"),
                );
                ui.end_row();
            });

        // Detect changes to re-validate only when needed
        if serde_json::to_string(&before).ok() != serde_json::to_string(&self.draft).ok() {
            self.dirty = true;
            self.errors = self.draft.validate();
        }

        ui.add_space(theme::space::LG);
        components::hairline(ui);

        ui.label(
            RichText::new("Distance Estimation")
                .size(theme::text::SUBHEADING)
                .color(theme::color::TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(theme::space::MD);

        egui::Grid::new("distance_grid")
            .num_columns(2)
            .spacing([theme::space::XL, theme::space::MD])
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Environment factor (n)")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.add(
                    egui::Slider::new(&mut self.draft.path_loss_n, 1.8..=5.0)
                        .step_by(0.1),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Default TX ref")
                        .size(theme::text::BODY)
                        .color(theme::color::TEXT_DIM),
                );
                ui.add(
                    egui::DragValue::new(&mut self.draft.default_tx_ref)
                        .range(-80.0..=-30.0)
                        .suffix(" dBm")
                        .speed(0.5),
                );
                ui.end_row();
            });

        ui.add_space(theme::space::LG);
        components::hairline(ui);

        ui.label(
            RichText::new("Paths")
                .size(theme::text::SUBHEADING)
                .color(theme::color::TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(theme::space::SM);
        components::kv_row_mono(ui, "Settings", &Settings::path().display().to_string());
        components::kv_row_mono(
            ui,
            "Database",
            &self.draft.database_path().display().to_string(),
        );

        if !self.errors.is_empty() {
            ui.add_space(theme::space::LG);
            for error in &self.errors.errors {
                ui.label(
                    RichText::new(error)
                        .size(theme::text::SMALL)
                        .color(theme::color::ERROR),
                );
            }
        }
        if let Some(error) = &self.save_error {
            ui.add_space(theme::space::MD);
            ui.label(
                RichText::new(format!("Save failed: {error}"))
                    .size(theme::text::SMALL)
                    .color(theme::color::ERROR),
            );
        }

        ui.add_space(theme::space::LG);
        components::hairline(ui);

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if components::ghost_button(ui, "Cancel", theme::color::TEXT_DIM).clicked() {
                    return SettingsModalResult::Cancelled;
                }
                let save_enabled = self.errors.is_empty();
                let resp = if save_enabled {
                    components::primary_button(ui, "Save", theme::color::NEAR)
                } else {
                    components::ghost_button(ui, "Save", theme::color::TEXT_FAINT)
                };
                if save_enabled && resp.clicked() {
                    match self.draft.save() {
                        Ok(()) => return SettingsModalResult::Saved(self.draft.clone()),
                        Err(error) => self.save_error = Some(error),
                    }
                }
                SettingsModalResult::Open
            })
            .inner
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn new_copies_settings() {
        let mut settings = Settings::default();
        settings.stale_after_seconds = 42;
        settings.min_rssi = -80;

        let modal = SettingsModal::new(&settings);
        assert_eq!(modal.draft.stale_after_seconds, 42);
        assert_eq!(modal.draft.min_rssi, -80);
    }

    #[test]
    fn new_starts_not_dirty() {
        let modal = SettingsModal::new(&Settings::default());
        assert!(!modal.dirty);
    }

    #[test]
    fn new_starts_with_no_errors() {
        let modal = SettingsModal::new(&Settings::default());
        assert!(modal.errors.is_empty());
    }

    #[test]
    fn new_starts_with_no_save_error() {
        let modal = SettingsModal::new(&Settings::default());
        assert!(modal.save_error.is_none());
    }
}
