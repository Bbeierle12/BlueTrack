# BlueTrack — Distance Estimation Formula Review

You are performing an **independent mathematical review** of BlueTrack's Bluetooth distance estimation. Your deliverable is a single, unified formula (or formula set) that replaces the three inconsistent models currently in the codebase, along with the constants, smoothing strategy, and mapping functions needed to implement it correctly.

Do not speculate. Justify every constant and every design choice with physics, published BLE research, or the Bluetooth Core Specification. Where empirical data is required, state what must be measured and how.

---

## Context: What BlueTrack Does

BlueTrack is a native Linux BLE/BR-EDR scanner. It receives advertisement events from BlueZ via D-Bus (the `bluer` crate). Each advertisement can include:

- **RSSI** (`i16`): Received signal strength in dBm. Updated on every advertisement (~1–10 Hz per device depending on advertising interval).
- **TX Power** (`Option<i16>`): The device's advertised transmit power at 1 metre, from the BlueZ `TxPower` property. Available for ~30–50% of BLE devices.
- **Beacon calibrated power** (`i8`): For iBeacon, AltBeacon, and Eddystone payloads, the calibrated TX power is embedded in the advertisement data itself.

The app accumulates per-device running statistics:
- `rssi_sum` / `rssi_samples` → `avg_rssi()` (arithmetic mean)
- `rssi_sum_squares` → `rssi_std_dev()` (population standard deviation via online variance)
- `rssi_min`, `rssi_max` (lifetime extremes)

## The Problem: Three Inconsistent Models

BlueTrack currently has three separate distance/proximity computations that contradict each other.

### Model A: General distance text display

**Location**: `src/ui/theme.rs`, function `rssi_to_distance_meters()`

```rust
const TX_REF_DBM: f32 = -59.0;
const PATH_LOSS_N: f32 = 2.0;
let ratio = (TX_REF_DBM - rssi as f32) / (10.0 * PATH_LOSS_N);
let meters = 10f32.powf(ratio);
meters.clamp(0.1, 100.0)
```

This is the log-distance path loss model: $d = 10^{\frac{P_{tx} - \text{RSSI}}{10n}}$

**Problems**:
1. Uses hardcoded $P_{tx} = -59\,\text{dBm}$ (a typical iBeacon reference), ignoring the device's actual `tx_power` field even when available.
2. Uses $n = 2.0$ (free-space path loss exponent), which is only valid in open air with no reflections. Indoor environments typically require $n \in [2.5, 4.0]$.
3. Takes a single instantaneous RSSI sample with no smoothing, despite running statistics being available.

### Model B: Beacon distance

**Location**: `src/intelligence/profile.rs`, function `estimate_distance()`

```rust
let ratio = (calibrated_power_dbm as f32 - rssi as f32) / 20.0;
let distance = 10f32.powf(ratio);
Some(distance.clamp(0.01, 100.0))
```

Same formula ($n = 2.0$ since $10n = 20$), but correctly uses the beacon's embedded calibrated TX power. Only runs for parsed iBeacon, AltBeacon, and Eddystone payloads.

### Model C: Radar dot placement

**Location**: `src/ui/mod.rs`, function `rssi_to_fraction()`

```rust
let clamped = (value as f32).clamp(-100.0, 0.0);
(-clamped) / 100.0
```

Pure linear mapping: $f = \frac{-\text{RSSI}}{100}$. This places dots proportionally on a radar disc. A device at $-50\,\text{dBm}$ appears at 50% of the radius, and one at $-70\,\text{dBm}$ at 70%. But physical distance follows a logarithmic relationship with RSSI — the visual spacing does not correspond to the distance readout or the real-world spacing.

### Band ring vs dot mismatch

The radar draws three rings at fixed fractions:
- **Near** ring at fraction 0.30, labeled "-58 dBm"
- **Mid** ring at fraction 0.58, labeled "-74 dBm"
- **Far** ring at fraction 1.00, labeled "-100 dBm"

But `rssi_to_fraction(-58)` returns $58/100 = 0.58$, which lands on the **Mid** ring, not Near. The ring positions and the dot placement formula are not aligned.

### Available but unused data

| Field | Source | Currently used for distance? |
|-------|--------|------------------------------|
| `record.rssi` (instantaneous) | Every advertisement | Yes — all three models |
| `record.tx_power` | BlueZ `TxPower` property | No — ignored by Model A |
| `record.avg_rssi()` | Computed running mean | No — only used as fallback in `proximity_band()` |
| `record.rssi_std_dev()` | Computed running std dev | No — only used in `stability_score()` |
| `record.rssi_min`, `rssi_max` | Tracked per device | No |
| `record.rssi_samples` | Count of RSSI observations | No |
| `beacon.calibrated_power_dbm` | Beacon payload | Yes — Model B only |

---

## Your Deliverables

### 1. Unified Distance Formula

Design a single distance estimation function that:

- **Uses the best available TX reference power**: beacon calibrated power > device `tx_power` > configurable default.
- **Accounts for indoor propagation**: Justify the path loss exponent $n$ you recommend. If it should be user-configurable, provide the valid range, a sensible default, and the physical reasoning.
- **Incorporates RSSI smoothing**: Define exactly what filter to apply (exponential moving average, sliding window median, Kalman, or other). Specify the filter parameters. Explain why that filter is appropriate for BLE advertisement cadence (typical 100ms–10s intervals, bursty, with occasional dropouts).
- **Handles edge cases**: No RSSI yet (new device), stale device (no recent advertisements), extreme values, very low sample count.

Express the formula in standard mathematical notation using:
- $d$ = estimated distance in metres
- $P_{tx}$ = reference TX power at 1m (dBm)
- $\overline{\text{RSSI}}$ = filtered RSSI value (dBm)
- $n$ = path loss exponent
- Any additional parameters you introduce

### 2. Radar Mapping Function

Design a function $f(\overline{\text{RSSI}}) \to [0, 1]$ that maps filtered RSSI to a radar fraction such that:

- The visual spacing between dots reflects actual distance ratios, not raw dBm ratios.
- The Near/Mid/Far ring positions correspond exactly to the proximity band thresholds (-58, -74 dBm).
- Devices with no RSSI (stale, unknown) map to defined positions outside the main rings.

Show the derivation. If the mapping should be logarithmic, prove that it produces the correct ring alignment. Provide the exact constants.

### 3. Proximity Band Thresholds

Evaluate whether -58 dBm (Near) and -74 dBm (Mid) are defensible thresholds:

- What physical distances do these correspond to under your recommended formula?
- Are these consistent with the Apple iBeacon specification's Immediate/Near/Far regions?
- Should the thresholds be expressed in metres instead of dBm, now that a proper distance formula exists?
- Should they be adjustable per-environment?

### 4. Confidence Metric

Define a confidence value $c \in [0, 1]$ for each distance estimate based on:
- Number of RSSI samples collected
- RSSI variance / standard deviation
- Whether TX power is known vs assumed
- Time since last advertisement

Provide the formula. Explain how the UI should communicate low-confidence estimates (e.g., fade the distance text, show a range instead of a point estimate, add a ± bound).

### 5. Implementation Specification

For each function you define, provide:
- **Inputs**: Exact types and what field(s) they come from on `DeviceRecord` or `BeaconPayload`.
- **Outputs**: Exact type and unit.
- **State**: Any per-device state that must be maintained between advertisements (e.g., EMA accumulator, sample window buffer).
- **Pseudocode**: Language-agnostic, unambiguous, directly translatable to Rust.

### 6. Validation Criteria

Define a set of test cases that the implementation must satisfy:

| Scenario | RSSI | TX Power | Expected Distance | Tolerance |
|----------|------|----------|-------------------|-----------|
| Reference point (TX power match) | -59 dBm | -59 dBm | 1.0 m | ±0.05 m |
| Strong signal, close | -40 dBm | -59 dBm | ? | ? |
| Weak signal, far | -90 dBm | -59 dBm | ? | ? |
| No TX power, default | -70 dBm | None | ? | ? |
| Beacon with calibrated power | -75 dBm | -65 (from payload) | ? | ? |

Fill in the expected values using your formula. Add at least 5 more scenarios covering edge cases (stale device, 1 sample, high variance, TX power of 0 dBm, RSSI equals TX power).

Also define monotonicity and consistency invariants:
- If RSSI_a > RSSI_b (same TX power), then distance_a < distance_b. Always.
- The radar fraction for a device at the Near threshold must land on the Near ring. Exactly.
- The distance value and the radar position must be monotonically consistent: closer distance = smaller fraction.

---

## Rules

- **Show your math.** Every formula must be derived or cited. No "industry standard" hand-waves — state which standard, which section, which equation number.
- **Justify constants.** For every magic number, explain where it comes from and under what conditions it is valid.
- **Be concrete.** Don't say "use a Kalman filter." Specify the state vector, the process noise, the measurement noise, and initialisation. Or pick a simpler filter and explain why it's sufficient.
- **Consider the runtime constraints.** This runs on a Raspberry Pi 4 processing 200+ devices at 1–10 Hz each. The filter must be computationally trivial — no matrix inversions, no FFTs.
- **One model, not three.** The general distance display, the beacon distance, and the radar mapping must all derive from the same underlying formula. The only difference between beacon and non-beacon paths should be the source of the TX power parameter.
- **No hedging.** Don't say "you might consider." State what the formula should be, and why.
