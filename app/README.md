# SensorView

A cross-platform, **HWiNFO-style hardware monitor** — Rust core (Tauri v2) with a
Svelte/TypeScript frontend. This app is a ground-up rewrite of the C#
OpenHardwareMonitor found in the repository root; the original C# remains as the
authoritative reference for low-level sensor access.

> HWiNFO is a proprietary product. SensorView reproduces a HWiNFO-*style* dense
> sensor UI and feature set; it is not affiliated with or endorsed by HWiNFO and
> is not distributed under a confusingly similar name.

## Architecture (hybrid, migrate-to-Rust)

The Rust core exposes a single `SensorSource` abstraction with two interchangeable
backends, so sensor coverage can migrate from .NET to Rust one device group at a
time without any UI or data-model changes:

| Backend | Status | Coverage |
| --- | --- | --- |
| `LhmBridge` | first release (Windows) | Full — bundles [LibreHardwareMonitor] as a sidecar |
| `NativeRust` | growing | Pure-Rust engine (WinRing0 FFI, NVML/ADL, Super-I/O, Linux `/sys`, macOS IOKit) |

```
app/
├── src/                 # Svelte + Vite + TypeScript frontend (HWiNFO-style UI)
├── src-tauri/
│   ├── src/
│   │   ├── model.rs      # SensorType / HardwareType / Sensor / Hardware
│   │   ├── lib.rs        # Tauri commands + app entry (run)
│   │   └── source/       # SensorSource trait + backends (added on later branches)
│   ├── icons/            # generated app icons
│   └── tauri.conf.json   # bundler config (exe / deb / dmg)
└── package.json
```

The data model (`src-tauri/src/model.rs`) mirrors OpenHardwareMonitor's
`Hardware/ISensor.cs` and `Hardware/IHardware.cs` enums exactly.

## Prerequisites

- **Rust** (stable, MSVC toolchain on Windows) — <https://rustup.rs>
- **Node.js** ≥ 20 and npm
- Platform build deps for Tauri v2 — see <https://tauri.app/start/prerequisites/>
  - Windows: VS Build Tools (VC++), WebView2 (preinstalled on Win 11)
  - Linux: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, etc.
  - macOS: Xcode command-line tools

## Develop

```bash
cd app
npm install
npm run tauri dev      # live-reload desktop app
```

## Build installers

```bash
npm run tauri build    # produces .exe/.msi (Win), .deb/.AppImage (Linux), .dmg (macOS)
```

Artifacts land in `src-tauri/target/release/bundle/`. CI builds all three on tag
push (see `.github/workflows/`, added on `feature/ci-packaging`).

## Roadmap (feature branches)

1. `feature/scaffold` — this skeleton ✅
2. `feature/ci-packaging` — GitHub Actions for exe/deb/dmg
3. `feature/sensor-core` — `SensorSource` trait, polling loop, min/max/avg + history
4. `feature/lhm-bridge` — LibreHardwareMonitor sidecar → full Windows sensors
5. `feature/ui-sensors-table` — HWiNFO-style sensor table + hardware tree
6. `feature/ui-system-summary` — System Summary window
7. `feature/ui-graphs-logging` — graphs, CSV logging, report export
8. `feature/tray-settings` — tray icon, settings/units, autostart
9–11. `feature/native-*` — pure-Rust sensor engine (CPU, GPU, Linux/macOS)

[LibreHardwareMonitor]: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor
