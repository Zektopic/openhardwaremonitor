//! Shared HWiNFO-style widgets: painted sensor-type icons, group header bands,
//! summary panels, feature "chips" and square checkboxes.

use eframe::egui::{self, Color32, Pos2, RichText, Stroke, StrokeKind, Vec2};

use super::Palette;
use crate::model::SensorType;

pub const ROW_H: f32 = 17.0;

/// Icon color per sensor type, HWiNFO-ish (yellow bolts, cyan clocks…).
pub fn type_color(t: SensorType, pal: &Palette) -> Color32 {
    match t {
        SensorType::Voltage | SensorType::Current | SensorType::Power | SensorType::Energy => pal.volt,
        SensorType::Clock | SensorType::Frequency | SensorType::TimeSpan => pal.clockc,
        SensorType::Temperature => pal.tempc,
        SensorType::Fan | SensorType::Flow | SensorType::Control => pal.fanc,
        _ => pal.text_dim,
    }
}

/// Collapsible group header band ("CPU [#0]: AMD Ryzen 7 7700"). Returns the
/// new collapsed state (None = unchanged).
pub fn group_header(ui: &mut egui::Ui, title: &str, collapsed: bool, width: f32, pal: &Palette) -> Option<bool> {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ROW_H + 1.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, pal.bg_header);
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, pal.grid),
    );
    let chev = if collapsed { "▸" } else { "▾" };
    p.text(
        Pos2::new(rect.left() + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        chev,
        egui::FontId::proportional(10.0),
        pal.text_dim,
    );
    p.text(
        Pos2::new(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(11.5),
        pal.text,
    );
    if resp.clicked() {
        Some(!collapsed)
    } else {
        None
    }
}

/// Boxed section with a header strip — the System Summary panel look.
pub fn panel<R>(
    ui: &mut egui::Ui,
    title: &str,
    pal: &Palette,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(pal.bg_panel)
        .stroke(Stroke::new(1.0, pal.grid))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.label(RichText::new(title).color(pal.accent).size(11.5).strong());
            ui.separator();
            add(ui)
        })
        .inner
}

/// Small feature "chip" (the green/dark ISA boxes in the Summary CPU panel).
/// The label never wraps — chips flow as whole units in a wrapped row.
pub fn chip(ui: &mut egui::Ui, label: &str, on: bool, pal: &Palette) {
    let (bg, fg) = if on {
        (pal.ok_badge, Color32::WHITE)
    } else {
        (pal.bg_header, pal.text_dim)
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(2)
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(label).color(fg).size(9.5))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
}

/// Square `[x]` checkbox like HWiNFO's settings dialog.
pub fn square_check(ui: &mut egui::Ui, value: &mut bool, label: &str, pal: &Palette) -> bool {
    let resp = ui
        .horizontal(|ui| {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(13.0), egui::Sense::click());
            let p = ui.painter();
            p.rect_stroke(rect, 0.0, Stroke::new(1.0, pal.text_dim), StrokeKind::Inside);
            if *value {
                p.line_segment(
                    [rect.left_top() + Vec2::splat(2.5), rect.right_bottom() - Vec2::splat(2.5)],
                    Stroke::new(1.6, pal.text),
                );
                p.line_segment(
                    [
                        Pos2::new(rect.right() - 2.5, rect.top() + 2.5),
                        Pos2::new(rect.left() + 2.5, rect.bottom() - 2.5),
                    ],
                    Stroke::new(1.6, pal.text),
                );
            }
            let lr = ui.label(RichText::new(label).size(11.5).color(pal.text));
            r.union(lr.interact(egui::Sense::click()))
        })
        .inner;
    if resp.clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

/// `label: value` row for info grids (Feature pane, Summary fields).
pub fn info_row(ui: &mut egui::Ui, label: &str, value: &str, pal: &Palette) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(pal.text_dim).size(11.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(if value.is_empty() { "—" } else { value }).color(pal.text).size(11.0));
        });
    });
}

/// Colored status badge (UEFI Boot / Secure Boot / HVCI rows).
pub fn badge(ui: &mut egui::Ui, label: &str, ok: Option<bool>, pal: &Palette) {
    let (bg, text) = match ok {
        Some(true) => (pal.ok_badge, "Enabled"),
        Some(false) => (pal.bg_header, "Disabled"),
        None => (pal.bg_header, "—"),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(pal.text_dim).size(11.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::Frame::new()
                .fill(bg)
                .corner_radius(2)
                .inner_margin(egui::Margin::symmetric(6, 1))
                .show(ui, |ui| {
                    ui.label(RichText::new(text).color(Color32::WHITE).size(10.0));
                });
        });
    });
}

/// Format a sensor value with unit, HWiNFO-style decimals.
pub fn format_value(value: Option<f32>, t: SensorType) -> String {
    let Some(v) = value else { return "—".to_string() };
    let decimals = match t {
        SensorType::Voltage => 3,
        SensorType::Fan | SensorType::SmallData => 0,
        SensorType::Data => 0,
        _ => 1,
    };
    let unit = t.unit();
    if unit.is_empty() {
        format!("{v:.decimals$}")
    } else {
        format!("{v:.decimals$} {unit}")
    }
}
