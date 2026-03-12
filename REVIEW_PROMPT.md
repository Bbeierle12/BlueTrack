# BlueTrack Code Review Prompt

You are reviewing **BlueTrack**, a native Linux Bluetooth proximity radar and device mapper. The codebase is ~2,400 lines of Rust across 9 source files.

## Repository

https://github.com/Bbeierle12/BlueTrack

## Architecture

```
src/
  main.rs              — Entry point, eframe window setup
  app.rs               — BluetoothApp struct, egui App impl, theme config
  model.rs             — DeviceRecord, AdapterStatus, RuntimeMetrics, AppSnapshot
  settings.rs          — Settings struct, XDG config paths, validation
  store.rs             — SQLite persistence (WAL mode, sightings + device state)
  backend/
    bluetooth.rs       — Async BlueZ worker (tokio runtime on dedicated thread),
                         discovery stream, device property ingestion, snapshot emission
  ui/
    mod.rs             — Top bar, filter tabs, device grid, proximity radar (egui painter),
                         device detail panel, event log
    settings_modal.rs  — Settings editor window with live validation
```

## Tech Stack

- **Rust 2024 edition** with `bluer` (BlueZ D-Bus bindings), `eframe`/`egui` (immediate-mode GUI), `rusqlite` (SQLite with bundled build), `tokio` (async runtime), `chrono`, `serde`
- Worker thread with its own multi-thread tokio runtime communicates to the egui main loop via `tokio::sync::watch` channel
- SQLite with WAL journal mode for concurrent read/write

## Key Design Decisions

1. **MAC randomization handling** — BLE devices with random (rotating) addresses are treated as ephemeral: not persisted to SQLite, purged from memory at 2x stale timeout. Only public-address devices are stored long-term. This prevents unbounded growth from MAC rotation.

2. **Watch channel (not mpsc)** — The worker emits snapshots via `watch::channel` so the UI always reads the latest state without draining a queue. Snapshot emission from advertisement events is throttled to 1/sec; new/removed devices trigger immediate emission.

3. **Proximity radar** — Custom painter-based radar visualization with concentric rings mapped to RSSI bands (-58/-74/-100 dBm). Devices are bucketed by band and distributed using golden-angle spacing to avoid overlap. Dot size scales with stability score; stale devices are dimmed and smaller.

4. **Filter tabs** — Live/Stale/Public/Random/Errors tabs in the scan view, each filtering the device grid. Counts are shown in the tab labels. Errors tab shows the event log.

5. **Throttled persistence** — DB upserts throttled to 5s per device. Sighting rows only for genuinely new public-address devices. Stale pruning on configurable retention window.

## What to Review

Please evaluate the following areas:

### Correctness
- Is the BlueZ event handling correct? Are there edge cases in `discover_devices_with_changes()` that could cause missed or duplicate devices?
- Is the stale/live lifecycle sound? Devices go stale after a configurable timeout, random ones are purged at 2x that. Could this lose important data?
- Is the `watch::channel` usage correct for cross-thread snapshot sharing? Any race conditions?

### Architecture
- Is the worker thread + dedicated tokio runtime the right pattern, or would a single shared runtime be better?
- Is the separation between model/backend/ui/store clean, or are there coupling concerns?
- Should the `DeviceRecord` struct be split (e.g., separate identity vs. signal vs. advertising data)?

### Performance
- The worker clones the full device HashMap into a Vec on every snapshot emission. With hundreds of devices, is this a concern?
- `all_properties()` is called on every `DeviceAdded` event. Should it use incremental property updates instead?
- The radar painter iterates all devices and does text layout on every frame. Is this fine for egui, or should it cache?

### Security & Robustness
- SQLite is opened from a user-controlled path. Is the migration safe?
- Settings are deserialized with `unwrap_or_default()` — is silent fallback the right choice vs. warning the user?
- Are there any panics hiding behind `.expect()` or `.unwrap()` in non-obvious paths?

### UI/UX
- Is the proximity radar readable with 50+ devices? Does the golden-angle spacing hold up?
- Are the filter tabs discoverable? Should there be an "All" tab?
- Is the settings modal layout stable across different window sizes?
- Should the detail panel scroll for devices with many UUIDs/service data entries?

### Missing Features / Next Steps
- What would you prioritize adding next?
- Are there any Linux-specific assumptions that would block portability?
- Should there be tests, and if so, what would you test first?
