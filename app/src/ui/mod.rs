//! HWiNFO-style native UI: main window + deferred viewports (Sensors Status,
//! System Summary, Settings), shared palette/theme, fonts and shared state.

pub mod graph_window;
pub mod main_window;
pub mod sensors_window;
pub mod settings_dialog;
pub mod summary_window;
pub mod widgets;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use eframe::egui::{self, Color32};

use crate::logging::CsvLogger;
use crate::poll::Monitor;
use crate::settings::{AppSettings, ColorMode};
use crate::sysinfo::SystemInfoHandle;

// ---- Which extra windows are open (shared with deferred viewports) ------

#[derive(Default)]
pub struct WindowFlags {
    pub sensors: AtomicBool,
    pub summary: AtomicBool,
    pub settings: AtomicBool,
}

impl WindowFlags {
    pub fn open(flag: &AtomicBool) {
        flag.store(true, Ordering::Relaxed);
    }
    pub fn close(flag: &AtomicBool) {
        flag.store(false, Ordering::Relaxed);
    }
    pub fn is_open(flag: &AtomicBool) -> bool {
        flag.load(Ordering::Relaxed)
    }
}

/// Everything a viewport callback needs. Cheap to clone (all Arcs).
#[derive(Clone)]
pub struct Shared {
    pub monitor: Arc<Mutex<Monitor>>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub sysinfo: SystemInfoHandle,
    pub windows: Arc<WindowFlags>,
    /// Sensor identifiers with an open graph window.
    pub graphs: Arc<RwLock<BTreeSet<String>>>,
    /// Active CSV logger (owned/written by the poll thread).
    pub logger: Arc<Mutex<Option<CsvLogger>>>,
    /// Whether this process is elevated (Some) or unknown/N-A (None). Detected
    /// in-process, so it's correct regardless of sidecar version.
    pub elevated: Option<bool>,
    pub started: Instant,
}

impl Shared {
    pub fn color_mode(&self) -> ColorMode {
        self.settings.read().map(|s| s.color_mode).unwrap_or(ColorMode::Black)
    }

    pub fn palette(&self) -> Palette {
        Palette::of(self.color_mode())
    }

    /// Uptime formatted like HWiNFO's bottom-bar clock (h:mm:ss).
    pub fn uptime_text(&self) -> String {
        let secs = self.started.elapsed().as_secs();
        format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
    }
}

// ---- Palette (HWiNFO Color Modes: Grey / Black / Light) -----------------

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub bg_panel: Color32,
    pub bg_header: Color32,
    pub row_even: Color32,
    pub row_odd: Color32,
    pub grid: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub value: Color32,
    pub warn: Color32,
    pub crit: Color32,
    pub volt: Color32,
    pub clockc: Color32,
    pub tempc: Color32,
    pub fanc: Color32,
    pub ok_badge: Color32,
}

impl Palette {
    pub fn of(mode: ColorMode) -> Self {
        match mode {
            ColorMode::Black => Self {
                bg: Color32::from_rgb(0x12, 0x12, 0x12),
                bg_panel: Color32::from_rgb(0x1b, 0x1b, 0x1b),
                bg_header: Color32::from_rgb(0x26, 0x26, 0x28),
                row_even: Color32::from_rgb(0x1a, 0x1a, 0x1a),
                row_odd: Color32::from_rgb(0x14, 0x14, 0x14),
                grid: Color32::from_rgb(0x38, 0x38, 0x38),
                text: Color32::from_rgb(0xe6, 0xe6, 0xe6),
                text_dim: Color32::from_rgb(0x9a, 0x9a, 0x9a),
                accent: Color32::from_rgb(0x4f, 0xa3, 0xff),
                value: Color32::from_rgb(0xdc, 0xdc, 0xdc),
                warn: Color32::from_rgb(0xe8, 0xc0, 0x50),
                crit: Color32::from_rgb(0xf1, 0x4c, 0x4c),
                volt: Color32::from_rgb(0xf0, 0xd0, 0x30),
                clockc: Color32::from_rgb(0x60, 0xc8, 0xf0),
                tempc: Color32::from_rgb(0xf0, 0x80, 0x60),
                fanc: Color32::from_rgb(0x80, 0xd0, 0x80),
                ok_badge: Color32::from_rgb(0x30, 0xa0, 0x40),
            },
            ColorMode::Grey => Self {
                bg: Color32::from_rgb(0x2e, 0x2e, 0x30),
                bg_panel: Color32::from_rgb(0x38, 0x38, 0x3a),
                bg_header: Color32::from_rgb(0x44, 0x44, 0x48),
                row_even: Color32::from_rgb(0x36, 0x36, 0x38),
                row_odd: Color32::from_rgb(0x30, 0x30, 0x32),
                grid: Color32::from_rgb(0x55, 0x55, 0x58),
                text: Color32::from_rgb(0xea, 0xea, 0xea),
                text_dim: Color32::from_rgb(0xaa, 0xaa, 0xaa),
                accent: Color32::from_rgb(0x6c, 0xb4, 0xff),
                value: Color32::from_rgb(0xe6, 0xe6, 0xe6),
                warn: Color32::from_rgb(0xe8, 0xc0, 0x50),
                crit: Color32::from_rgb(0xf1, 0x4c, 0x4c),
                volt: Color32::from_rgb(0xf0, 0xd0, 0x30),
                clockc: Color32::from_rgb(0x70, 0xd0, 0xf8),
                tempc: Color32::from_rgb(0xf0, 0x88, 0x68),
                fanc: Color32::from_rgb(0x90, 0xd8, 0x90),
                ok_badge: Color32::from_rgb(0x38, 0xa8, 0x48),
            },
            ColorMode::Light => Self {
                bg: Color32::from_rgb(0xf2, 0xf2, 0xf2),
                bg_panel: Color32::from_rgb(0xfa, 0xfa, 0xfa),
                bg_header: Color32::from_rgb(0xdd, 0xdd, 0xe2),
                row_even: Color32::from_rgb(0xee, 0xee, 0xee),
                row_odd: Color32::from_rgb(0xf6, 0xf6, 0xf6),
                grid: Color32::from_rgb(0xc0, 0xc0, 0xc4),
                text: Color32::from_rgb(0x1a, 0x1a, 0x1a),
                text_dim: Color32::from_rgb(0x60, 0x60, 0x60),
                accent: Color32::from_rgb(0x0a, 0x64, 0xc8),
                value: Color32::from_rgb(0x10, 0x10, 0x10),
                warn: Color32::from_rgb(0xb0, 0x80, 0x00),
                crit: Color32::from_rgb(0xc8, 0x20, 0x20),
                volt: Color32::from_rgb(0xa0, 0x80, 0x00),
                clockc: Color32::from_rgb(0x00, 0x78, 0xa8),
                tempc: Color32::from_rgb(0xc0, 0x40, 0x20),
                fanc: Color32::from_rgb(0x20, 0x80, 0x20),
                ok_badge: Color32::from_rgb(0x20, 0x88, 0x30),
            },
        }
    }
}

// ---- Theme / fonts ------------------------------------------------------

pub fn apply_theme(ctx: &egui::Context, pal: &Palette, light: bool) {
    ctx.set_theme(if light { egui::Theme::Light } else { egui::Theme::Dark });
    let pal = *pal;
    ctx.all_styles_mut(move |style| {
        style.visuals.panel_fill = pal.bg;
        style.visuals.window_fill = pal.bg_panel;
        style.visuals.extreme_bg_color = pal.bg;
        style.visuals.override_text_color = Some(pal.text);
        style.visuals.widgets.noninteractive.bg_stroke.color = pal.grid;
        style.spacing.item_spacing = egui::vec2(4.0, 2.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}

/// Load Segoe UI from the Windows fonts dir for HWiNFO-faithful text.
/// Silently keeps egui's default fonts when unavailable (non-Windows, CI).
pub fn install_fonts(ctx: &egui::Context) {
    let candidates = [r"C:\Windows\Fonts\segoeui.ttf"];
    let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("segoe".into(), Arc::new(egui::FontData::from_owned(bytes)));
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "segoe".into());
    }
    ctx.set_fonts(fonts);
}

// ---- Viewport registration ---------------------------------------------

/// Re-register every open deferred viewport. Must be called each frame from
/// the root `App::ui`.
pub fn show_open_viewports(ctx: &egui::Context, shared: &Shared) {
    if WindowFlags::is_open(&shared.windows.sensors) {
        let s = shared.clone();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("sensors"),
            egui::ViewportBuilder::default()
                .with_title("SensorView Sensors Status")
                .with_inner_size([560.0, 760.0])
                .with_min_inner_size([300.0, 300.0]),
            move |ui, _class| sensors_window::show(ui, &s),
        );
    }
    if WindowFlags::is_open(&shared.windows.summary) {
        let s = shared.clone();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("summary"),
            egui::ViewportBuilder::default()
                .with_title("SensorView - System Summary")
                .with_inner_size([900.0, 640.0])
                .with_min_inner_size([700.0, 500.0]),
            move |ui, _class| summary_window::show(ui, &s),
        );
    }
    if WindowFlags::is_open(&shared.windows.settings) {
        let s = shared.clone();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("settings"),
            egui::ViewportBuilder::default()
                .with_title("SensorView - Settings")
                .with_inner_size([680.0, 430.0])
                .with_resizable(false),
            move |ui, _class| settings_dialog::show(ui, &s),
        );
    }
    // One deferred viewport per open sensor graph.
    let open_graphs: Vec<String> = shared
        .graphs
        .read()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    for id in open_graphs {
        let s = shared.clone();
        let vid = egui::ViewportId::from_hash_of(("graph", &id));
        let graph_id = id.clone();
        ctx.show_viewport_deferred(
            vid,
            egui::ViewportBuilder::default()
                .with_title("SensorView - Graph")
                .with_inner_size([420.0, 240.0]),
            move |ui, _class| graph_window::show(ui, &s, &graph_id),
        );
    }
}

/// Standard close-request handling for a deferred viewport: flips its flag.
pub fn handle_close(ui: &egui::Ui, flag: &AtomicBool) {
    if ui.ctx().input(|i| i.viewport().close_requested()) {
        WindowFlags::close(flag);
    }
}
