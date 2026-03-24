use bluer::DiscoveryTransport;
use eframe::egui;

use crate::{
    model::AdapterStatus,
    settings::{Settings, ValidationErrors},
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
        ui.heading("Discovery");
        ui.add_space(6.0);

        let before = self.draft.clone();

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label("Preferred adapter");
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

                ui.label("Transport");
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.draft.scan_transport,
                        DiscoveryTransport::Auto,
                        "Auto",
                    );
                    ui.selectable_value(
                        &mut self.draft.scan_transport,
                        DiscoveryTransport::Le,
                        "LE",
                    );
                    ui.selectable_value(
                        &mut self.draft.scan_transport,
                        DiscoveryTransport::BrEdr,
                        "BR/EDR",
                    );
                });
                ui.end_row();

                ui.label("Auto-start scan");
                ui.checkbox(&mut self.draft.auto_start_scan, "");
                ui.end_row();

                ui.label("Duplicate advertisement data");
                ui.checkbox(&mut self.draft.allow_duplicate_data, "");
                ui.end_row();

                ui.label("Stale timeout");
                ui.add(
                    egui::DragValue::new(&mut self.draft.stale_after_seconds)
                        .range(1..=600)
                        .suffix(" s"),
                );
                ui.end_row();

                ui.label("UI refresh / prune cadence");
                ui.add(
                    egui::DragValue::new(&mut self.draft.refresh_interval_seconds)
                        .range(1..=120)
                        .suffix(" s"),
                );
                ui.end_row();

                ui.label("Minimum RSSI");
                ui.add(egui::Slider::new(&mut self.draft.min_rssi, -110..=0).suffix(" dBm"));
                ui.end_row();

                ui.label("Retention");
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

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(12.0);

        ui.strong("Paths");
        ui.label(format!("Settings: {}", Settings::path().display()));
        ui.label(format!(
            "Database: {}",
            self.draft.database_path().display()
        ));

        if !self.errors.is_empty() {
            ui.add_space(12.0);
            for error in &self.errors.errors {
                ui.colored_label(egui::Color32::from_rgb(220, 82, 70), error);
            }
        }
        if let Some(error) = &self.save_error {
            ui.add_space(8.0);
            ui.colored_label(
                egui::Color32::from_rgb(220, 82, 70),
                format!("Save failed: {error}"),
            );
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    return SettingsModalResult::Cancelled;
                }
                let save_enabled = self.errors.is_empty();
                if ui
                    .add_enabled(save_enabled, egui::Button::new("Save"))
                    .clicked()
                {
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
