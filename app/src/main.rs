//! SensorView — a HWiNFO-style native hardware monitor.
//!
//! Pure-Rust native app (eframe/egui, no webview). A background thread polls
//! the active [`source::SensorSource`]; [`poll::Monitor`] folds readings into
//! running min/max/avg + history; the [`ui`] module renders the HWiNFO-style
//! windows (Main, Sensors Status, System Summary, Settings).

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
mod model;
mod poll;
mod report;
mod settings;
mod source;
mod sysinfo;
mod ui;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use eframe::egui;

use poll::Monitor;
use settings::AppSettings;
use ui::{main_window, Shared, WindowFlags};

fn main() -> eframe::Result {
    let app_settings = AppSettings::load();
    let poll_interval = Duration::from_millis(app_settings.poll_interval_ms.clamp(250, 10_000));

    let shared = Shared {
        monitor: Arc::new(Mutex::new(Monitor::new(source::default_source()))),
        settings: Arc::new(RwLock::new(app_settings.clone())),
        sysinfo: sysinfo::spawn_query(),
        windows: Arc::new(WindowFlags::default()),
        graphs: Arc::new(RwLock::new(std::collections::BTreeSet::new())),
        logger: Arc::new(Mutex::new(None)),
        elevated: sysinfo::is_elevated(),
        started: Instant::now(),
    };

    // Startup windows per settings (HWiNFO's "Show … on Startup").
    shared
        .windows
        .summary
        .store(app_settings.show_summary_on_startup, Ordering::Relaxed);
    shared
        .windows
        .sensors
        .store(app_settings.show_sensors_on_startup, Ordering::Relaxed);
    // Dev/testing affordances (env-gated, harmless in normal use).
    if std::env::var("SENSORVIEW_SHOW_SETTINGS").is_ok() {
        shared.windows.settings.store(true, Ordering::Relaxed);
    }
    // Prime one snapshot so graph/logging dev-hooks have sensors to attach to.
    if std::env::var("SENSORVIEW_OPEN_GRAPH").is_ok() || std::env::var("SENSORVIEW_START_LOGGING").is_ok() {
        let tree = shared.monitor.lock().map(|mut m| m.poll()).unwrap_or_default();
        if let Ok(needle) = std::env::var("SENSORVIEW_OPEN_GRAPH") {
            if let Some(id) = first_sensor_matching(&tree, &needle) {
                shared.windows.sensors.store(true, Ordering::Relaxed);
                if let Ok(mut g) = shared.graphs.write() {
                    g.insert(id);
                }
            }
        }
        if std::env::var("SENSORVIEW_START_LOGGING").is_ok() {
            if let Ok(l) = logging::CsvLogger::start(&tree) {
                *shared.logger.lock().unwrap() = Some(l);
                shared.windows.sensors.store(true, Ordering::Relaxed);
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SensorView")
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([620.0, 420.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    let poll_shared = shared.clone();
    eframe::run_native(
        "SensorView",
        options,
        Box::new(move |cc| {
            ui::install_fonts(&cc.egui_ctx);
            spawn_poll_thread(cc.egui_ctx.clone(), poll_shared, poll_interval);
            Ok(Box::new(SensorViewApp {
                shared,
                main_state: main_window::MainWindowState::default(),
            }))
        }),
    )
}

struct SensorViewApp {
    shared: Shared,
    main_state: main_window::MainWindowState,
}

impl eframe::App for SensorViewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Theme follows the settings' Color Mode (switchable live).
        let pal = self.shared.palette();
        let light = self.shared.color_mode() == settings::ColorMode::Light;
        ui::apply_theme(ui.ctx(), &pal, light);

        main_window::show(ui, &self.shared, &mut self.main_state);
        ui::show_open_viewports(ui.ctx(), &self.shared);
    }

    fn on_exit(&mut self) {
        if let Ok(st) = self.shared.settings.read() {
            if st.remember_preferences {
                st.save();
            }
        }
    }
}

/// Background thread: poll once per interval, feed the CSV logger, and wake the
/// UI to repaint. The logger lives here so file IO never blocks the UI thread.
fn spawn_poll_thread(ctx: egui::Context, shared: Shared, interval: Duration) {
    std::thread::spawn(move || loop {
        let tree = match shared.monitor.lock() {
            Ok(mut m) => m.poll(),
            Err(_) => break, // poisoned; nothing sensible to do
        };
        if let Ok(mut logger) = shared.logger.lock() {
            if let Some(l) = logger.as_mut() {
                l.log(&tree);
            }
        }
        ctx.request_repaint();
        std::thread::sleep(interval);
    });
}

/// First sensor identifier whose name contains `needle` (case-insensitive).
fn first_sensor_matching(tree: &[model::Hardware], needle: &str) -> Option<String> {
    let needle = needle.to_lowercase();
    fn walk(tree: &[model::Hardware], needle: &str) -> Option<String> {
        for hw in tree {
            for s in &hw.sensors {
                if s.name.to_lowercase().contains(needle) {
                    return Some(s.identifier.clone());
                }
            }
            if let Some(f) = walk(&hw.sub_hardware, needle) {
                return Some(f);
            }
        }
        None
    }
    walk(tree, &needle)
}

/// Window icon (32×32 PNG baked into the binary).
fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/32x32.png");
    let img = image::load_from_memory(bytes)
        .expect("embedded icon is valid PNG")
        .into_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width, height }
}
