//! Unified distance estimation for BLE devices.
//!
//! Implements the cascaded median→EMA RSSI filter, log-distance path loss model,
//! logarithmic radar mapping, proximity band classification, and confidence metric.
//! See `bluetrack_distance_spec.md` for derivations and references.

use std::time::Instant;

// ── Constants ─────────────────────────────────────────────────────────────

/// Free-space path loss at 1 m, 2.44 GHz (dB).
/// ITU-R P.525: FSPL = 20·log10(4π·1/λ), λ = c/f = 0.12287 m → 40.196 dB.
pub const FSPL_1M: f32 = 40.2;

/// Default assumed RSSI at 1 m when no TX power is available (dBm).
pub const DEFAULT_TX_REF: f32 = -59.0;

/// Default indoor path loss exponent. Range [1.8, 5.0].
pub const DEFAULT_N: f32 = 2.5;

/// EMA time constant in seconds.
const TAU_TARGET: f32 = 3.0;

/// Receiver sensitivity floor (dBm).
const RSSI_FLOOR: f32 = -100.0;

/// Minimum reportable distance (m).
const DIST_MIN: f32 = 0.1;

/// Maximum reportable distance (m).
const DIST_MAX: f32 = 100.0;

/// Radar inner edge (m).
const RADAR_D_MIN: f32 = 0.1;

/// Radar outer edge (m).
const RADAR_D_MAX: f32 = 30.0;

/// log10(RADAR_D_MAX / RADAR_D_MIN) = log10(300).
const RADAR_LOG_RANGE: f32 = 2.4771;

/// Near/Mid boundary in meters.
pub const NEAR_THRESHOLD_M: f32 = 1.0;

/// Mid/Far boundary in meters.
pub const MID_THRESHOLD_M: f32 = 4.0;

/// Number of RSSI samples for full sample confidence.
const CONFIDENCE_SAMPLE_TARGET: f32 = 10.0;

/// Standard deviation at which variance confidence drops to 0.1.
const CONFIDENCE_SIGMA_MAX: f32 = 15.0;

/// Seconds until staleness confidence reaches zero.
const CONFIDENCE_STALE_TIMEOUT: f32 = 60.0;

// ── Types ─────────────────────────────────────────────────────────────────

/// Source of the TX reference power used in distance estimation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TxSource {
    /// Beacon calibrated power (iBeacon, AltBeacon, Eddystone).
    BeaconCal,
    /// AD type 0x0A via BlueZ, converted with FSPL.
    Advertised,
    /// Fallback to user-configured default.
    Default,
}

/// Result of a distance estimation pipeline run.
#[derive(Debug, Clone, Copy)]
pub struct DistanceEstimate {
    pub distance_m: f32,
    pub radar_fraction: f32,
    pub confidence: f32,
    pub tx_source: TxSource,
}

/// Per-device RSSI smoothing state: cascaded median-of-3 → time-weighted EMA.
#[derive(Debug, Clone)]
pub struct RssiFilter {
    median_buf: [f32; 3],
    median_idx: u8,
    median_count: u8,
    ema_rssi: Option<f32>,
    last_adv_time: Option<Instant>,
}

impl Default for RssiFilter {
    fn default() -> Self {
        Self {
            median_buf: [0.0; 3],
            median_idx: 0,
            median_count: 0,
            ema_rssi: None,
            last_adv_time: None,
        }
    }
}

impl RssiFilter {
    /// Returns the current smoothed RSSI, if any samples have been processed.
    pub fn smoothed_rssi(&self) -> Option<f32> {
        self.ema_rssi
    }
}

// ── TX Power Resolution ──────────────────────────────────────────────────

/// Beacon type hint for TX power semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeaconType {
    IBeacon,
    AltBeacon,
    EddystoneUid,
    None,
}

/// Resolve the best available TX reference power (RSSI at 1 m) in dBm.
pub fn resolve_tx_ref(
    beacon_cal_power: Option<i8>,
    beacon_type: BeaconType,
    bluez_tx_power: Option<i16>,
    default_tx_ref: f32,
) -> (f32, TxSource) {
    // Priority 1: Beacon calibrated power
    if let Some(cal) = beacon_cal_power {
        return match beacon_type {
            BeaconType::EddystoneUid => {
                // Eddystone: value is "power at 0 m" = RF output power
                (cal as f32 - FSPL_1M, TxSource::BeaconCal)
            }
            BeaconType::IBeacon | BeaconType::AltBeacon => {
                // iBeacon/AltBeacon: value IS RSSI at 1 m
                (cal as f32, TxSource::BeaconCal)
            }
            BeaconType::None => (cal as f32, TxSource::BeaconCal),
        };
    }

    // Priority 2: AD type 0x0A via BlueZ (RF output power → convert)
    if let Some(tx) = bluez_tx_power {
        if tx != 127 {
            let rssi_1m = tx as f32 - FSPL_1M;
            return (rssi_1m, TxSource::Advertised);
        }
    }

    // Priority 3: Default
    (default_tx_ref, TxSource::Default)
}

// ── RSSI Smoothing ───────────────────────────────────────────────────────

/// Branchless median of three values using a comparison network.
fn median3(a: f32, b: f32, c: f32) -> f32 {
    if a > b {
        if b > c { b } else if a > c { c } else { a }
    } else if a > c {
        a
    } else if b > c {
        c
    } else {
        b
    }
}

/// Feed a new raw RSSI sample into the cascaded median→EMA filter.
/// Returns the smoothed RSSI value in dBm.
pub fn smooth_rssi(filter: &mut RssiFilter, new_rssi: i16, now: Instant) -> f32 {
    let raw = (new_rssi as f32).clamp(RSSI_FLOOR, 0.0);

    // Median prefilter
    filter.median_buf[filter.median_idx as usize] = raw;
    filter.median_idx = (filter.median_idx + 1) % 3;
    filter.median_count = filter.median_count.saturating_add(1).min(3);

    let filtered = match filter.median_count {
        1 => raw,
        2 => (filter.median_buf[0] + filter.median_buf[1]) / 2.0,
        _ => median3(filter.median_buf[0], filter.median_buf[1], filter.median_buf[2]),
    };

    // Time-weighted EMA
    match filter.ema_rssi {
        None => {
            filter.ema_rssi = Some(filtered);
            filter.last_adv_time = Some(now);
            filtered
        }
        Some(prev) => {
            let dt = filter
                .last_adv_time
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(1.0)
                .clamp(0.01, 30.0);

            let alpha = 1.0 - (-dt / TAU_TARGET).exp();
            let ema = alpha * filtered + (1.0 - alpha) * prev;

            filter.ema_rssi = Some(ema);
            filter.last_adv_time = Some(now);
            ema
        }
    }
}

// ── Distance Estimation ──────────────────────────────────────────────────

/// Estimate distance in meters using the log-distance path loss model.
///
/// `d = 10^((p_tx - smoothed_rssi) / (10·n))`
pub fn estimate_distance(smoothed_rssi: f32, p_tx: f32, n: f32) -> f32 {
    let exponent = (p_tx - smoothed_rssi) / (10.0 * n);
    10.0_f32.powf(exponent).clamp(DIST_MIN, DIST_MAX)
}

// ── Radar Mapping ────────────────────────────────────────────────────────

/// Map an estimated distance to a [0, 1] radar fraction using logarithmic scaling.
pub fn distance_to_radar_fraction(distance_m: f32) -> f32 {
    let d = distance_m.clamp(RADAR_D_MIN, RADAR_D_MAX);
    let frac = (d.log10() - RADAR_D_MIN.log10()) / RADAR_LOG_RANGE;
    frac.clamp(0.0, 1.0)
}

// ── Proximity Bands ──────────────────────────────────────────────────────

/// Classify distance into a proximity band.
pub fn proximity_band_from_distance(distance_m: f32) -> &'static str {
    if distance_m < NEAR_THRESHOLD_M {
        "Near"
    } else if distance_m < MID_THRESHOLD_M {
        "Mid"
    } else {
        "Far"
    }
}

// ── Confidence ───────────────────────────────────────────────────────────

/// Compute a [0, 1] confidence score for a distance estimate.
pub fn compute_confidence(
    rssi_samples: u64,
    rssi_std_dev: Option<f32>,
    tx_source: TxSource,
    secs_since_last_adv: f32,
) -> f32 {
    let c_samples = (rssi_samples as f32 / CONFIDENCE_SAMPLE_TARGET).min(1.0);

    let sigma = rssi_std_dev.unwrap_or(0.0);
    let c_variance = (1.0 - sigma / CONFIDENCE_SIGMA_MAX).max(0.1);

    let c_tx = match tx_source {
        TxSource::BeaconCal => 1.0,
        TxSource::Advertised => 0.7,
        TxSource::Default => 0.5,
    };

    let c_staleness = (1.0 - secs_since_last_adv / CONFIDENCE_STALE_TIMEOUT).max(0.0);

    c_samples * c_variance * c_tx * c_staleness
}

// ── Formatting ───────────────────────────────────────────────────────────

/// Format a distance estimate for display, respecting confidence level.
pub fn format_distance(distance_m: Option<f32>, confidence: f32) -> String {
    match distance_m {
        Some(d) if confidence >= 0.6 => {
            if d < 1.0 {
                format!("{:.2} m", d)
            } else if d < 10.0 {
                format!("{:.1} m", d)
            } else {
                format!("{:.0} m", d)
            }
        }
        Some(d) if confidence >= 0.3 => format!("~{:.0} m", d),
        _ => "—".to_string(),
    }
}

/// Compute the dot alpha for radar rendering based on confidence.
pub fn dot_alpha(confidence: f32) -> f32 {
    if confidence >= 0.6 {
        1.0
    } else if confidence >= 0.3 {
        0.5
    } else {
        confidence
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── TX Power Resolution ──────────────────────────────────────────

    #[test]
    fn resolve_ibeacon_cal_power() {
        let (p_tx, src) = resolve_tx_ref(Some(-59), BeaconType::IBeacon, None, DEFAULT_TX_REF);
        assert_eq!(p_tx, -59.0);
        assert_eq!(src, TxSource::BeaconCal);
    }

    #[test]
    fn resolve_eddystone_subtracts_fspl() {
        let (p_tx, src) = resolve_tx_ref(Some(-18), BeaconType::EddystoneUid, None, DEFAULT_TX_REF);
        assert!((p_tx - (-58.2)).abs() < 0.01);
        assert_eq!(src, TxSource::BeaconCal);
    }

    #[test]
    fn resolve_bluez_tx_power() {
        let (p_tx, src) = resolve_tx_ref(None, BeaconType::None, Some(4), DEFAULT_TX_REF);
        assert!((p_tx - (-36.2)).abs() < 0.01);
        assert_eq!(src, TxSource::Advertised);
    }

    #[test]
    fn resolve_bluez_sentinel_127_falls_through() {
        let (p_tx, src) = resolve_tx_ref(None, BeaconType::None, Some(127), DEFAULT_TX_REF);
        assert_eq!(p_tx, DEFAULT_TX_REF);
        assert_eq!(src, TxSource::Default);
    }

    #[test]
    fn resolve_beacon_takes_priority_over_bluez() {
        let (p_tx, src) = resolve_tx_ref(Some(-65), BeaconType::AltBeacon, Some(4), DEFAULT_TX_REF);
        assert_eq!(p_tx, -65.0);
        assert_eq!(src, TxSource::BeaconCal);
    }

    #[test]
    fn resolve_default_when_nothing() {
        let (p_tx, src) = resolve_tx_ref(None, BeaconType::None, None, -59.0);
        assert_eq!(p_tx, -59.0);
        assert_eq!(src, TxSource::Default);
    }

    // ── Median-of-3 ─────────────────────────────────────────────────

    #[test]
    fn median3_all_orderings() {
        assert_eq!(median3(1.0, 2.0, 3.0), 2.0);
        assert_eq!(median3(1.0, 3.0, 2.0), 2.0);
        assert_eq!(median3(2.0, 1.0, 3.0), 2.0);
        assert_eq!(median3(2.0, 3.0, 1.0), 2.0);
        assert_eq!(median3(3.0, 1.0, 2.0), 2.0);
        assert_eq!(median3(3.0, 2.0, 1.0), 2.0);
    }

    #[test]
    fn median3_equal_values() {
        assert_eq!(median3(5.0, 5.0, 5.0), 5.0);
    }

    // ── RSSI Smoothing ──────────────────────────────────────────────

    #[test]
    fn smooth_first_sample_passthrough() {
        let mut filter = RssiFilter::default();
        let now = Instant::now();
        let result = smooth_rssi(&mut filter, -60, now);
        assert_eq!(result, -60.0);
        assert_eq!(filter.smoothed_rssi(), Some(-60.0));
    }

    #[test]
    fn smooth_clamps_to_floor() {
        let mut filter = RssiFilter::default();
        let now = Instant::now();
        let result = smooth_rssi(&mut filter, -120, now);
        assert_eq!(result, RSSI_FLOOR);
    }

    #[test]
    fn smooth_clamps_positive_rssi() {
        let mut filter = RssiFilter::default();
        let now = Instant::now();
        let result = smooth_rssi(&mut filter, 5, now);
        assert_eq!(result, 0.0);
    }

    // ── Distance Estimation ─────────────────────────────────────────

    #[test]
    fn distance_reference_point() {
        // Test 1: RSSI = P_tx → 1.0 m
        let d = estimate_distance(-59.0, -59.0, 2.5);
        assert!((d - 1.0).abs() < 0.001);
    }

    #[test]
    fn distance_strong_signal() {
        // Test 2: RSSI = -40, P_tx = -59, n = 2.5 → 0.174 m
        let d = estimate_distance(-40.0, -59.0, 2.5);
        assert!((d - 0.174).abs() < 0.01, "got {d}");
    }

    #[test]
    fn distance_moderate_signal() {
        // Test 3: RSSI = -70, P_tx = -59, n = 2.5 → 2.754 m
        let d = estimate_distance(-70.0, -59.0, 2.5);
        assert!((d - 2.754).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_mid_boundary() {
        // Test 4: RSSI = -74, P_tx = -59, n = 2.5 → 3.981 m
        let d = estimate_distance(-74.0, -59.0, 2.5);
        assert!((d - 3.981).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_weak_signal() {
        // Test 5: RSSI = -90, P_tx = -59, n = 2.5 → 17.38 m
        let d = estimate_distance(-90.0, -59.0, 2.5);
        assert!((d - 17.38).abs() < 1.0, "got {d}");
    }

    #[test]
    fn distance_at_floor() {
        // Test 6: RSSI = -100, P_tx = -59, n = 2.5 → 43.65 m
        let d = estimate_distance(-100.0, -59.0, 2.5);
        assert!((d - 43.65).abs() < 2.0, "got {d}");
    }

    #[test]
    fn distance_rssi_stronger_than_p_tx() {
        // Test 7: RSSI = -50, P_tx = -59, n = 2.5
        // d = 10^((-59 - -50) / 25) = 10^(-9/25) = 10^(-0.36) = 0.437 m
        let d = estimate_distance(-50.0, -59.0, 2.5);
        assert!((d - 0.437).abs() < 0.02, "got {d}");
    }

    #[test]
    fn distance_no_tx_default() {
        // Test 9: Same as test 3 when default is used
        let d = estimate_distance(-70.0, DEFAULT_TX_REF, DEFAULT_N);
        assert!((d - 2.754).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_beacon_cal_power() {
        // Test 10: RSSI = -75, P_tx = -65 (beacon), n = 2.5 → 2.512 m
        let d = estimate_distance(-75.0, -65.0, 2.5);
        assert!((d - 2.512).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_ad_type_high_power() {
        // Test 11: AD 0x0A = +4 dBm → P_tx = 4 - 40.2 = -36.2
        // d = 10^((-36.2 - -70) / 25) = 10^(33.8/25) = 10^1.352 = 22.49 m
        let d = estimate_distance(-70.0, -36.2, 2.5);
        assert!((d - 22.49).abs() < 3.0, "got {d}");
    }

    #[test]
    fn distance_ad_type_zero_power() {
        // Test 12: AD 0x0A = 0 dBm → P_tx = 0 - 40.2 = -40.2
        // d = 10^((-40.2 - -70) / 25) = 10^(29.8/25) = 10^1.192 = 15.56 m
        let d = estimate_distance(-70.0, -40.2, 2.5);
        assert!((d - 15.56).abs() < 2.0, "got {d}");
    }

    #[test]
    fn distance_eddystone_tx() {
        // Test 13: Eddystone TX = -18 → P_tx = -18 - 40.2 = -58.2
        // d = 10^((-58.2 - -75) / 25) = 10^(16.8/25) = 10^0.672 = 4.699 m
        let d = estimate_distance(-75.0, -58.2, 2.5);
        assert!((d - 4.699).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_high_n() {
        // Test 14: n = 3.5 → d = 10^((-59 - -70) / 35) = 10^(11/35) = 10^0.3143 = 2.062 m
        let d = estimate_distance(-70.0, -59.0, 3.5);
        assert!((d - 2.062).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_low_n() {
        // Test 15: n = 2.0 → d = 10^((-59 - -70) / 20) = 10^(11/20) = 10^0.55 = 3.548 m
        let d = estimate_distance(-70.0, -59.0, 2.0);
        assert!((d - 3.548).abs() < 0.1, "got {d}");
    }

    #[test]
    fn distance_clamp_min() {
        // Very strong signal, distance should clamp to DIST_MIN
        let d = estimate_distance(-10.0, -59.0, 2.5);
        assert_eq!(d, DIST_MIN);
    }

    // ── Radar Fractions ─────────────────────────────────────────────

    #[test]
    fn radar_inner_edge() {
        let f = distance_to_radar_fraction(0.1);
        assert!(f.abs() < 0.01, "got {f}");
    }

    #[test]
    fn radar_near_ring() {
        let f = distance_to_radar_fraction(1.0);
        assert!((f - 0.404).abs() < 0.01, "got {f}");
    }

    #[test]
    fn radar_mid_ring() {
        let f = distance_to_radar_fraction(4.0);
        assert!((f - 0.647).abs() < 0.01, "got {f}");
    }

    #[test]
    fn radar_10m() {
        let f = distance_to_radar_fraction(10.0);
        assert!((f - 0.808).abs() < 0.01, "got {f}");
    }

    #[test]
    fn radar_outer_edge() {
        let f = distance_to_radar_fraction(30.0);
        assert!((f - 1.0).abs() < 0.01, "got {f}");
    }

    #[test]
    fn radar_clamp_below_min() {
        let f = distance_to_radar_fraction(0.05);
        assert_eq!(f, 0.0);
    }

    #[test]
    fn radar_clamp_above_max() {
        let f = distance_to_radar_fraction(50.0);
        assert_eq!(f, 1.0);
    }

    // ── Proximity Bands ─────────────────────────────────────────────

    #[test]
    fn band_near() {
        assert_eq!(proximity_band_from_distance(0.5), "Near");
    }

    #[test]
    fn band_near_boundary() {
        // Exactly at 1.0 m → Mid (not < 1.0)
        assert_eq!(proximity_band_from_distance(1.0), "Mid");
    }

    #[test]
    fn band_mid() {
        assert_eq!(proximity_band_from_distance(2.5), "Mid");
    }

    #[test]
    fn band_far() {
        assert_eq!(proximity_band_from_distance(5.0), "Far");
    }

    // ── Confidence ──────────────────────────────────────────────────

    #[test]
    fn confidence_full() {
        // Test 1: 10 samples, σ=3, BeaconCal, 0s stale → 0.80
        let c = compute_confidence(10, Some(3.0), TxSource::BeaconCal, 0.0);
        assert!((c - 0.80).abs() < 0.01, "got {c}");
    }

    #[test]
    fn confidence_single_sample_default() {
        // Test 2: 1 sample, σ=0, Default, 0s → 0.05
        let c = compute_confidence(1, Some(0.0), TxSource::Default, 0.0);
        assert!((c - 0.05).abs() < 0.01, "got {c}");
    }

    #[test]
    fn confidence_half_stale() {
        // Test 3: 10 samples, σ=3, BeaconCal, 30s → 0.40
        let c = compute_confidence(10, Some(3.0), TxSource::BeaconCal, 30.0);
        assert!((c - 0.40).abs() < 0.01, "got {c}");
    }

    #[test]
    fn confidence_high_variance() {
        // Test 4: 10 samples, σ=12, Default, 0s → 0.10
        let c = compute_confidence(10, Some(12.0), TxSource::Default, 0.0);
        assert!((c - 0.10).abs() < 0.01, "got {c}");
    }

    #[test]
    fn confidence_mid_range() {
        // Test 5: 5 samples, σ=3, Advertised, 10s → 0.233
        let c = compute_confidence(5, Some(3.0), TxSource::Advertised, 10.0);
        assert!((c - 0.233).abs() < 0.02, "got {c}");
    }

    #[test]
    fn confidence_fully_stale() {
        // Test 6: 10 samples, σ=3, Default, 60s → 0.00
        let c = compute_confidence(10, Some(3.0), TxSource::Default, 60.0);
        assert_eq!(c, 0.0);
    }

    // ── Format Distance ─────────────────────────────────────────────

    #[test]
    fn format_high_confidence_close() {
        assert_eq!(format_distance(Some(0.5), 0.8), "0.50 m");
    }

    #[test]
    fn format_high_confidence_mid() {
        assert_eq!(format_distance(Some(3.2), 0.8), "3.2 m");
    }

    #[test]
    fn format_high_confidence_far() {
        assert_eq!(format_distance(Some(15.0), 0.8), "15 m");
    }

    #[test]
    fn format_medium_confidence() {
        assert_eq!(format_distance(Some(3.2), 0.4), "~3 m");
    }

    #[test]
    fn format_low_confidence() {
        assert_eq!(format_distance(Some(3.2), 0.1), "—");
    }

    #[test]
    fn format_no_distance() {
        assert_eq!(format_distance(None, 0.0), "—");
    }

    // ── Dot Alpha ───────────────────────────────────────────────────

    #[test]
    fn dot_alpha_high() {
        assert_eq!(dot_alpha(0.8), 1.0);
    }

    #[test]
    fn dot_alpha_medium() {
        assert_eq!(dot_alpha(0.4), 0.5);
    }

    #[test]
    fn dot_alpha_low() {
        assert!((dot_alpha(0.2) - 0.2).abs() < 0.001);
    }

    // ── Invariants ──────────────────────────────────────────────────

    #[test]
    fn invariant_distance_monotonicity() {
        // INV-1: Higher RSSI → shorter distance (fixed P_tx, n)
        let d_strong = estimate_distance(-40.0, -59.0, 2.5);
        let d_mid = estimate_distance(-60.0, -59.0, 2.5);
        let d_weak = estimate_distance(-80.0, -59.0, 2.5);
        assert!(d_strong < d_mid);
        assert!(d_mid < d_weak);
    }

    #[test]
    fn invariant_radar_monotonicity() {
        // INV-2: Shorter distance → smaller fraction
        let f_close = distance_to_radar_fraction(0.5);
        let f_mid = distance_to_radar_fraction(5.0);
        let f_far = distance_to_radar_fraction(20.0);
        assert!(f_close < f_mid);
        assert!(f_mid < f_far);
    }

    #[test]
    fn invariant_ring_alignment() {
        // INV-3: Near threshold → near ring fraction, Mid threshold → mid ring fraction
        let f_near = distance_to_radar_fraction(NEAR_THRESHOLD_M);
        let f_mid = distance_to_radar_fraction(MID_THRESHOLD_M);
        assert!((f_near - 0.404).abs() < 0.01);
        assert!((f_mid - 0.647).abs() < 0.01);
    }

    #[test]
    fn invariant_confidence_bounds() {
        // INV-5: Confidence always in [0, 1]
        for samples in [0, 1, 5, 10, 100] {
            for sigma in [0.0, 3.0, 10.0, 20.0] {
                for src in [TxSource::BeaconCal, TxSource::Advertised, TxSource::Default] {
                    for stale in [0.0, 10.0, 30.0, 60.0, 120.0] {
                        let c = compute_confidence(samples, Some(sigma), src, stale);
                        assert!(
                            (0.0..=1.0).contains(&c),
                            "out of bounds: c={c} for samples={samples}, σ={sigma}, src={src:?}, stale={stale}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn invariant_no_nan() {
        // INV-6: No NaN/Inf from any valid inputs
        let test_rssis = [-100.0, -80.0, -59.0, -40.0, -10.0, 0.0];
        let test_ptx = [-80.0, -59.0, -36.2, 0.0];
        let test_n = [1.8, 2.0, 2.5, 3.5, 5.0];

        for &rssi in &test_rssis {
            for &ptx in &test_ptx {
                for &n in &test_n {
                    let d = estimate_distance(rssi, ptx, n);
                    assert!(d.is_finite(), "NaN/Inf: rssi={rssi}, ptx={ptx}, n={n} → d={d}");

                    let f = distance_to_radar_fraction(d);
                    assert!(f.is_finite(), "NaN/Inf fraction: d={d} → f={f}");
                }
            }
        }
    }
}
