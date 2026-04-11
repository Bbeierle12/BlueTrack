# BlueTrack — Deep Feature Gap Analysis

You are performing a **definitive feature gap analysis** of BlueTrack, a native Linux Bluetooth proximity radar and device mapper. Do not hedge. Do not say "probably" or "might want." For every gap you identify, state clearly: **this is missing, here is what it would take to add it, and here is why it matters.**

## What BlueTrack Is

A real-time Bluetooth scanner and device intelligence tool for Linux. It discovers BLE and BR/EDR devices, classifies them by type, detects known trackers (AirTag, Tile, SmartTag), parses beacon payloads, maps proximity via RSSI, and persists device history to SQLite. The UI is built with egui/eframe (immediate-mode native GUI). The backend is a dedicated Tokio worker thread talking to BlueZ over D-Bus via the `bluer` crate.

## Codebase Scope

- **~9,400 lines of Rust** across 17 source files, plus ~310 lines of tests/benchmarks
- **Dependencies**: bluer 0.17.4, eframe 0.33.3, rusqlite 0.38.0, tokio 1.50, chrono, serde, uuid

### File Breakdown

| File | Lines | Purpose |
|------|-------|---------|
| `src/ui/mod.rs` | 2,039 | All 5 tab views, device detail panel, radar painter |
| `src/backend/bluetooth.rs` | 1,703 | Async BlueZ worker, discovery, device commands, ping |
| `src/model.rs` | 1,183 | DeviceRecord (40+ fields), AppSnapshot, metrics, enums |
| `src/intelligence/oui.rs` | 833 | MAC OUI → manufacturer lookup (300+ entries) |
| `src/intelligence/profile.rs` | 709 | Beacon parsing, address type classification, Apple Continuity |
| `src/intelligence/classify.rs` | 498 | 16-category device classifier, tracker signature detection |
| `src/store.rs` | 455 | SQLite schema, device persistence, sighting history |
| `src/settings.rs` | 363 | User preferences, validation, XDG config paths |
| `src/ui/components.rs` | 331 | Design system: cards, chips, buttons, meters |
| `src/ui/theme.rs` | 298 | Color/spacing/typography tokens |
| `src/ui/settings_modal.rs` | 290 | Settings editor with live validation |
| `src/app.rs` | 289 | eframe App impl, action routing, scan cycles |
| `src/intelligence/uuids.rs` | 242 | 80+ standard Bluetooth UUID → name mappings |
| `src/intelligence/mod.rs` | 170 | Enrichment pipeline orchestrator |

### Currently Implemented Features (verified, not assumed)

**Scanning**: Passive background scan, timed discovery cycles, transport selection (Auto/LE/BR-EDR), RSSI floor filtering, duplicate ad filtering, auto-start on launch.

**Device Intelligence**: 16-category classification from 6 data sources (icon, class, appearance, UUIDs, manufacturer data, OUI). Tracker detection for AirTag, Tile, SmartTag, Find My. iBeacon/AltBeacon/Eddystone parsing. Apple Continuity device type decoding (20+ subtypes). OUI manufacturer lookup (300+ MACs). 80+ standard UUID resolution. Company ID → company name. Address type classification (public, random-static, random-resolvable, random-non-resolvable).

**Device Management**: Connect/disconnect, trust/untrust, block/unblock, forget, rename (alias). Ping with Immediate Alert Service (IAS) write for audible alert on device. GATT Device Information Service reading (manufacturer, model, firmware, hardware).

**Adapter Management**: Power on/off, discoverable toggle, pairable toggle. Multi-adapter support with selection.

**UI**: 5-tab layout (Scan grid, Radar polar plot, Profiles list, Adapter config, Event log). Device detail sidebar with 7 sections. Proximity band filtering (All/Near/Mid/Far/Stale). Profile sorting (Stability/Name/Last Seen/RSSI) with search and tracker filter. Full dark theme design system.

**Persistence**: SQLite (WAL mode) with device state table and sighting history. Throttled writes (5s device upsert, 10s sighting insert). Configurable sighting retention with auto-prune. Settings in JSON at XDG config path.

**Metrics**: Runtime counters (total/live/stale/public/random devices, scan restarts, errors). In-memory event log (100 entries). Stability score (0–100). Proximity bands with estimated distance.

---

## Your Task

Read **every source file** listed above, in full. Then produce the following:

### 1. Feature Gaps — Definitive List

For each missing feature, provide:

- **What is missing** — One sentence, no hedging.
- **Impact** — Who is affected and how. Is this a usability gap, a safety gap, a data gap, or a workflow gap?
- **Implementation scope** — Small (< 100 lines), Medium (100–500 lines), or Large (500+ lines). Name the files that would change.
- **Dependencies** — Any new crates or system APIs required.
- **Priority** — Critical (blocks core use cases), High (significantly limits utility), Medium (nice to have for power users), Low (polish).

Organize gaps into these categories:
1. **Data Export & Interoperability**
2. **Notifications & Alerting**
3. **Visualization & Analytics**
4. **Security & Privacy Intelligence**
5. **Device Interaction & GATT**
6. **Search, Filtering & Navigation**
7. **System Integration**
8. **UI/UX Completeness**
9. **Testing & Reliability**

### 2. Bluetooth Protocol Coverage Gaps

Compare BlueTrack's current protocol handling against the Bluetooth 5.x specification:

- Which GATT services does it read? Which standard services does it ignore that would provide useful data?
- Which advertising data types (AD types from the Bluetooth Supplement) does it parse? Which does it skip?
- Does it handle Extended Advertising (Bluetooth 5.0+)? Periodic Advertising? Coded PHY?
- What about Bluetooth Mesh beacons?
- Is the BR/EDR scanning path fully utilized or is it only capturing addresses?

### 3. Intelligence & Classification Gaps

- Which device categories are misclassifiable with the current heuristic chain? Give concrete examples.
- Which major device manufacturers are missing from the OUI table?
- Which tracker brands exist in the wild that are not detected? (Chipolo, Galaxy Find, Pebblebee, Eufy, etc.)
- Are there behavioral tracker detection techniques (repeated sightings of a rotating random address that follows you) that could be implemented?

### 4. Architecture & Scalability Gaps

- What happens with 500+ simultaneous devices? 1,000+? Where are the bottlenecks?
- Is the single-snapshot-clone-per-emit pattern sustainable at scale?
- Are there any unbounded data structures that grow without limit?
- What breaks if the BlueZ D-Bus connection drops mid-scan?

### 5. Competitive Comparison

Compare BlueTrack's feature set against these tools:
- **nRF Connect** (Nordic Semiconductor) — mobile BLE scanner
- **Wireshark BLE** — packet-level analysis
- **Bluelog** — Linux CLI Bluetooth scanner
- **AirGuard** — Android AirTag tracker detection
- **Bluetooth Scanner** (Android apps in general)

For each, state what they do that BlueTrack does not.

### 6. Prioritized Roadmap

Based on all of the above, produce a ranked list of the **top 15 features to implement next**, ordered by (impact × feasibility). For each, give a one-line description and estimated scope.

---

## Rules

- **Read every file.** Do not summarize from function signatures alone. Read implementations.
- **Be definitive.** "This is missing" not "this could be added." "This will break" not "this might have issues."
- **Be specific.** Name the Bluetooth UUIDs, the GATT services, the AD type codes, the crate names. Don't hand-wave.
- **No filler.** Don't restate what already works unless comparing against a gap. Don't pad with disclaimers or caveats.
- **Prove your claims.** If you say a feature is missing, cite the file and line range where it would live and confirm it's not there. If you say something would break at scale, explain the mechanism.
