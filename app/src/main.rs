//! SensorView — a HWiNFO-style native hardware monitor.
//!
//! Pure-Rust native app (eframe/egui, no webview). A background thread polls
//! the active [`source::SensorSource`]; [`poll::Monitor`] folds readings into
//! running min/max/avg + history; the [`ui`] module renders the HWiNFO-style
//! windows (Main, Sensors Status, System Summary, Settings).

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    // Dev/testing affordance: open the Settings dialog immediately.
    if std::env::var("SENSORVIEW_SHOW_SETTINGS").is_ok() {
        shared.windows.settings.store(true, Ordering::Relaxed);
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

/// Background thread: poll once per interval and wake the UI to repaint.
fn spawn_poll_thread(ctx: egui::Context, shared: Shared, interval: Duration) {
    std::thread::spawn(move || loop {
        {
            match shared.monitor.lock() {
                Ok(mut m) => {
                    m.poll();
                }
                Err(_) => break, // poisoned; nothing sensible to do
            }
        }
        ctx.request_repaint();
        std::thread::sleep(interval);
    });
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
