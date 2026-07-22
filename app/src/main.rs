//! SensorView — a HWiNFO-style native hardware monitor.
//!
//! Pure-Rust native app (eframe/egui, no webview). A background thread polls
//! the active [`source::SensorSource`] once per second; [`poll::Monitor`]
//! folds readings into running min/max/avg + history, and [`ui::SensorViewApp`]
//! renders the dense sensor table.

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod poll;
mod source;
mod ui;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

use poll::Monitor;

/// Poll interval. HWiNFO defaults to ~2 s; 1 s feels responsive for a monitor.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

fn main() -> eframe::Result {
    let monitor = Arc::new(Mutex::new(Monitor::new(source::default_source())));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SensorView")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    let poll_monitor = monitor.clone();
    eframe::run_native(
        "SensorView",
        options,
        Box::new(move |cc| {
            spawn_poll_thread(cc.egui_ctx.clone(), poll_monitor);
            Ok(Box::new(ui::SensorViewApp::new(cc, monitor)))
        }),
    )
}

/// Background thread: poll once per interval and wake the UI to repaint.
fn spawn_poll_thread(ctx: egui::Context, monitor: Arc<Mutex<Monitor>>) {
    std::thread::spawn(move || loop {
        {
            match monitor.lock() {
                Ok(mut m) => {
                    m.poll();
                }
                Err(_) => break, // poisoned; nothing sensible to do
            }
        }
        ctx.request_repaint();
        std::thread::sleep(POLL_INTERVAL);
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
