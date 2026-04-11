//! Design tokens and color helpers for BlueTrack.
//!
//! Single source of truth for every color, spacing value, corner radius, stroke
//! width, and typographic size used in the UI. All rendering code should pull
//! constants and helpers from this module instead of hardcoding literals.
//!
//! The palette matches the wireframe reference — a dark navy theme with
//! ultra-thin borders, compact typography, and a small semantic accent set
//! (near/mid/far/stale/error/tracker).

use eframe::egui::Color32;

use crate::intelligence::classify::{DeviceCategory, TrackerConfidence};
use crate::intelligence::profile::AddressTypeDetail;
use crate::model::LogLevel;

// ─────────────────────────────────────────────────────────────────────────────
// Color palette
// ─────────────────────────────────────────────────────────────────────────────

pub mod color {
    use super::Color32;

    // ── Surfaces ────────────────────────────────────────────────────────────
    pub const BG_WINDOW: Color32 = Color32::from_rgb(0x12, 0x1a, 0x21);
    pub const BG_PANEL: Color32 = Color32::from_rgb(0x0a, 0x11, 0x16);
    pub const BG_EXTREME: Color32 = Color32::from_rgb(0x07, 0x0b, 0x0f);
    pub const BG_SURFACE_1: Color32 = Color32::from_rgb(0x0e, 0x18, 0x22);
    pub const BG_SURFACE_2: Color32 = Color32::from_rgb(0x12, 0x1c, 0x24);
    pub const BG_SURFACE_3: Color32 = Color32::from_rgb(0x15, 0x20, 0x28);

    // ── Borders ─────────────────────────────────────────────────────────────
    pub const BORDER: Color32 = Color32::from_rgb(0x28, 0x3c, 0x4b);
    pub const BORDER_MUTED: Color32 = Color32::from_rgb(0x32, 0x41, 0x50);

    // ── Text ────────────────────────────────────────────────────────────────
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xdc, 0xe1, 0xe6);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xa0, 0xaf, 0xbe);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8c, 0x96, 0xa0);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x78, 0x8c, 0xa0);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x64, 0x73, 0x7d);

    // ── Accents / semantic ──────────────────────────────────────────────────
    pub const ACCENT: Color32 = Color32::from_rgb(0xa0, 0xc8, 0xff);
    pub const NEAR: Color32 = Color32::from_rgb(0x51, 0xc4, 0x88);
    pub const MID: Color32 = Color32::from_rgb(0x61, 0xad, 0xff);
    pub const FAR: Color32 = Color32::from_rgb(0xb9, 0x7a, 0xff);
    pub const STALE: Color32 = Color32::from_rgb(0xd1, 0x87, 0x3d);
    pub const ERROR: Color32 = Color32::from_rgb(0xdc, 0x52, 0x46);
    pub const TRACKER: Color32 = Color32::from_rgb(0xe6, 0x3c, 0x3c);

    pub const STATUS_ACTIVE: Color32 = NEAR;
    pub const STATUS_WARN: Color32 = STALE;

    // ── Category palette ────────────────────────────────────────────────────
    pub const CAT_PHONE: Color32 = MID;
    pub const CAT_COMPUTER: Color32 = NEAR;
    pub const CAT_AUDIO: Color32 = FAR;
    pub const CAT_WEARABLE: Color32 = Color32::from_rgb(0x78, 0xdc, 0xc8);
    pub const CAT_FITNESS: Color32 = NEAR;
    pub const CAT_HEALTH: Color32 = Color32::from_rgb(0x64, 0xdc, 0x78);
    pub const CAT_INPUT: Color32 = Color32::from_rgb(0xdc, 0xc8, 0x64);
    pub const CAT_BEACON: Color32 = STALE;
    pub const CAT_NETWORK: Color32 = Color32::from_rgb(0x64, 0xb4, 0xdc);
    pub const CAT_PRINTER: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);
    pub const CAT_VEHICLE: Color32 = Color32::from_rgb(0xc8, 0xa0, 0x64);
    pub const CAT_SMARTHOME: Color32 = Color32::from_rgb(0xb4, 0xdc, 0x8c);
    pub const CAT_UNKNOWN: Color32 = TEXT_FAINT;
}

// ─────────────────────────────────────────────────────────────────────────────
// Spacing scale
// ─────────────────────────────────────────────────────────────────────────────

pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
    pub const XL: f32 = 16.0;
    pub const XXL: f32 = 24.0;

    // `i8` variants for egui's `Margin::same` / `Margin::symmetric`.
    pub const XS_I: i8 = 4;
    pub const SM_I: i8 = 6;
    pub const MD_I: i8 = 8;
    pub const LG_I: i8 = 12;
    pub const XL_I: i8 = 16;
    pub const XXL_I: i8 = 24;
}

// ─────────────────────────────────────────────────────────────────────────────
// Corner radii
// ─────────────────────────────────────────────────────────────────────────────

pub mod radius {
    pub const SM: f32 = 4.0;
    pub const MD: f32 = 6.0;
    pub const LG: f32 = 8.0;
    pub const XL: f32 = 10.0;
    pub const PILL: f32 = 12.0;
}

// ─────────────────────────────────────────────────────────────────────────────
// Stroke widths
// ─────────────────────────────────────────────────────────────────────────────

pub mod stroke {
    pub const THIN: f32 = 1.0;
    pub const REGULAR: f32 = 1.5;
    pub const THICK: f32 = 2.0;
}

// ─────────────────────────────────────────────────────────────────────────────
// Alpha (gamma_multiply) scale
// ─────────────────────────────────────────────────────────────────────────────

pub mod alpha {
    pub const TRACE: f32 = 0.05;
    pub const FAINT: f32 = 0.10;
    pub const SOFT: f32 = 0.15;
    pub const BADGE: f32 = 0.18;
    pub const STRONG: f32 = 0.25;
    pub const BORDER_DIM: f32 = 0.30;
    pub const BORDER_MED: f32 = 0.50;
}

// ─────────────────────────────────────────────────────────────────────────────
// Typography sizes
// ─────────────────────────────────────────────────────────────────────────────

pub mod text {
    pub const HEADING: f32 = 20.0;
    pub const SUBHEADING: f32 = 14.0;
    pub const BODY: f32 = 13.0;
    pub const BUTTON: f32 = 12.0;
    pub const MONO: f32 = 11.0;
    pub const SMALL: f32 = 11.0;
    pub const TINY: f32 = 10.0;
}

// ─────────────────────────────────────────────────────────────────────────────
// Color mapping helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn category_color(category: &DeviceCategory) -> Color32 {
    match category {
        DeviceCategory::Phone | DeviceCategory::Tablet => color::CAT_PHONE,
        DeviceCategory::Computer => color::CAT_COMPUTER,
        DeviceCategory::Headphone | DeviceCategory::Headset | DeviceCategory::Speaker => {
            color::CAT_AUDIO
        }
        DeviceCategory::Wearable => color::CAT_WEARABLE,
        DeviceCategory::FitnessTracker => color::CAT_FITNESS,
        DeviceCategory::HealthSensor => color::CAT_HEALTH,
        DeviceCategory::InputDevice => color::CAT_INPUT,
        DeviceCategory::Beacon => color::CAT_BEACON,
        DeviceCategory::Tracker => color::TRACKER,
        DeviceCategory::NetworkDevice => color::CAT_NETWORK,
        DeviceCategory::Printer => color::CAT_PRINTER,
        DeviceCategory::Vehicle => color::CAT_VEHICLE,
        DeviceCategory::SmartHome => color::CAT_SMARTHOME,
        DeviceCategory::Unknown => color::CAT_UNKNOWN,
    }
}

pub fn address_type_color(detail: &AddressTypeDetail) -> Color32 {
    match detail {
        AddressTypeDetail::Public => color::NEAR,
        AddressTypeDetail::RandomStatic => color::MID,
        AddressTypeDetail::RandomResolvable => color::STALE,
        AddressTypeDetail::RandomNonResolvable => color::FAR,
        AddressTypeDetail::Unknown => color::TEXT_MUTED,
    }
}

pub fn proximity_band_color(band: &str) -> Color32 {
    match band {
        "Near" => color::NEAR,
        "Mid" => color::MID,
        "Far" => color::FAR,
        "Stale" => color::STALE,
        _ => color::TEXT_MUTED,
    }
}

pub fn log_level_color(level: &LogLevel) -> Color32 {
    match level {
        LogLevel::Info => color::MID,
        LogLevel::Warn => color::STALE,
        LogLevel::Error => color::ERROR,
    }
}

pub fn state_color(stale: bool) -> Color32 {
    if stale { color::STALE } else { color::NEAR }
}

pub fn tracker_confidence_color(_c: &TrackerConfidence) -> Color32 {
    color::TRACKER
}

// ─────────────────────────────────────────────────────────────────────────────
// RSSI → distance
// ─────────────────────────────────────────────────────────────────────────────

/// Approximate distance in meters from an RSSI sample using the free-space
/// path loss model:
///
///     d = 10 ^ ((tx_ref - rssi) / (10 * n))
///
/// with a 1 m BLE reference power of `-59 dBm` and path-loss exponent `n = 2.0`.
///
/// This is a rough estimate — real-world accuracy is ±1–3 m outdoors and can
/// be much worse indoors or through obstacles. The result is clamped to a
/// sensible `[0.1, 100.0]` range. Intended only for display labels and not
/// for any safety-critical logic.
pub fn rssi_to_distance_meters(rssi: i16) -> f32 {
    const TX_REF_DBM: f32 = -59.0;
    const PATH_LOSS_N: f32 = 2.0;

    let ratio = (TX_REF_DBM - rssi as f32) / (10.0 * PATH_LOSS_N);
    let meters = 10f32.powf(ratio);
    meters.clamp(0.1, 100.0)
}

/// Short human-readable distance string: `"1.2m"`, `"12m"`, `"—"` for None.
pub fn format_distance(rssi: Option<i16>) -> String {
    match rssi {
        Some(value) => {
            let m = rssi_to_distance_meters(value);
            if m < 10.0 {
                format!("{m:.1}m")
            } else {
                format!("{:.0}m", m.round())
            }
        }
        None => "—".to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_color_stable() {
        assert_eq!(category_color(&DeviceCategory::Phone), color::MID);
        assert_eq!(category_color(&DeviceCategory::Computer), color::NEAR);
        assert_eq!(category_color(&DeviceCategory::Headphone), color::FAR);
        assert_eq!(category_color(&DeviceCategory::Unknown), color::TEXT_FAINT);
    }

    #[test]
    fn address_type_color_is_single_source() {
        // Regression: the pre-refactor code had two conflicting copies of this
        // match, with `RandomStatic` mapped to two different shades of blue.
        // The canonical helper resolves that to `MID`.
        assert_eq!(
            address_type_color(&AddressTypeDetail::RandomStatic),
            color::MID
        );
        assert_eq!(address_type_color(&AddressTypeDetail::Public), color::NEAR);
    }

    #[test]
    fn proximity_band_color_covers_named_bands() {
        assert_eq!(proximity_band_color("Near"), color::NEAR);
        assert_eq!(proximity_band_color("Mid"), color::MID);
        assert_eq!(proximity_band_color("Far"), color::FAR);
        assert_eq!(proximity_band_color("Stale"), color::STALE);
        assert_eq!(proximity_band_color("Unknown"), color::TEXT_MUTED);
    }

    #[test]
    fn rssi_to_distance_monotonic_and_clamped() {
        // Stronger signal → shorter distance.
        assert!(rssi_to_distance_meters(-40) < rssi_to_distance_meters(-70));
        // Reference signal → 1 m.
        let near_ref = rssi_to_distance_meters(-59);
        assert!((near_ref - 1.0).abs() < 0.05);
        // Extreme values are clamped.
        assert!(rssi_to_distance_meters(-150) <= 100.0);
        assert!(rssi_to_distance_meters(0) >= 0.1);
    }

    #[test]
    fn format_distance_handles_none() {
        assert_eq!(format_distance(None), "—");
        assert!(format_distance(Some(-59)).ends_with('m'));
    }
}
