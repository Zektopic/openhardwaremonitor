# SensorView

A **native**, cross-platform, HWiNFO-style hardware monitor written in pure Rust
(eframe/egui — real native windows, no webview, no Electron). This app is a
ground-up rewrite of the C# OpenHardwareMonitor found in the repository root;
the original C# remains as the authoritative reference for low-level sensor
access.

> HWiNFO is a proprietary product. SensorView reproduces a HWiNFO-*style* dense
> sensor UI and feature set; it is not affiliated with or endorsed by HWiNFO.

## Architecture (hybrid, migrate-to-Rust)

The core exposes a single `SensorSource` trait so sensor backends can migrate
from .NET to Rust one device group at a time without any UI or data-model
changes:

| Backend | Status | Coverage |
| --- | --- | --- |
| `demo` | now | Synthetic data — exercises the whole pipeline w/o drivers |
| `lhm_bridge` | next (Windows) | Full — bundles [LibreHardwareMonitor] as a sidecar |
| `native` | growing | Pure-Rust engine (WinRing0 FFI, NVML/ADL, Super-I/O, Linux `/sys`, macOS IOKit) |

```
app/
├── src/
│   ├── main.rs      # eframe entry + background poll thread
│   ├── ui.rs        # HWiNFO-style native UI (dense sensors table)
│   ├── model.rs     # SensorType / HardwareType / Sensor / Hardware
│   ├── poll.rs      # Monitor: min/max/avg + history ring buffers (+ tests)
│   └── source/      # SensorSource trait + backends
├── assets/          # window + exe icons
└── Cargo.toml       # incl. cargo-packager config (NSIS / deb / dmg)
```

The data model mirrors OpenHardwareMonitor's `Hardware/ISensor.cs` and
`Hardware/IHardware.cs` enums exactly.

## Build & run

Prereqs: [Rust](https://rustup.rs) (MSVC toolchain on Windows; VS Build Tools +
Windows SDK for the linker).

```bash
cd app
cargo run --release
```

## Installers

CI (`.github/workflows/`) builds on every push and publishes on `v*` tags via
[cargo-packager]: NSIS setup `.exe` (Windows), `.deb`/`.AppImage` (Linux),
`.dmg` (macOS). Locally: `cargo packager --release --formats nsis`.

## Roadmap (feature branches)

1. `feature/scaffold` — project skeleton ✅
2. `feature/ci-packaging` — CI for exe/deb/dmg ✅
3. `feature/sensor-core` — SensorSource + poll engine + native UI ✅ (this branch)
4. `feature/lhm-bridge` — LibreHardwareMonitor sidecar → full Windows sensors
5. `feature/ui-sensors-table` — full HWiNFO sensors-window fidelity
6. `feature/ui-system-summary` — System Summary window
7. `feature/ui-graphs-logging` — graphs, CSV logging, report export
8. `feature/tray-settings` — tray icon, settings/units, autostart
9–11. `feature/native-*` — pure-Rust sensor engine (CPU, GPU, Linux/macOS)

[LibreHardwareMonitor]: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager
