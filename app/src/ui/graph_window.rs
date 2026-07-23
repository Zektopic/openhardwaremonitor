//! Per-sensor history graph (HWiNFO's "Show Graph"), hand-painted with the egui
//! painter — autoscaled polyline over a dark grid, with min/max/current labels.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Stroke};

use super::widgets::{self, format_value};
use super::{Palette, Shared};

/// Draw the graph for `identifier`. Called from a deferred viewport.
pub fn show(ui: &mut egui::Ui, s: &Shared, identifier: &str) {
    // Close request → drop it from the open set.
    if ui.ctx().input(|i| i.viewport().close_requested()) {
        if let Ok(mut set) = s.graphs.write() {
            set.remove(identifier);
        }
    }

    let pal = s.palette();
    let frame = s.frame();
    let history = s.store.history(identifier);
    let sensor = frame.find_sensor(identifier).cloned();

    let name = sensor.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| identifier.to_string());
    ui.ctx()
        .send_viewport_cmd(egui::ViewportCommand::Title(format!("{name} — Graph")));

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::same(8)))
        .show(ui, |ui| {
            // Header row: name + current/min/max.
            if let Some(sen) = &sensor {
                ui.horizontal(|ui| {
                    widgets::sensor_icon_at_cursor(ui, sen.sensor_type, &pal);
                    ui.label(RichText::new(&sen.name).color(pal.text).size(12.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format_value(sen.value, sen.sensor_type))
                                .color(widgets::type_color(sen.sensor_type, &pal))
                                .size(12.0)
                                .monospace(),
                        );
                    });
                });
            }

            let color = sensor
                .as_ref()
                .map(|s| widgets::type_color(s.sensor_type, &pal))
                .unwrap_or(pal.accent);
            paint_chart(ui, &history, color, &pal);
        });
}

fn paint_chart(ui: &mut egui::Ui, history: &[f32], line: Color32, pal: &Palette) {
    let rect = ui.available_rect_before_wrap();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, pal.row_odd);

    if history.len() < 2 {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Collecting samples…",
            FontId::proportional(12.0),
            pal.text_dim,
        );
        return;
    }

    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &v in history {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if (hi - lo).abs() < f32::EPSILON {
        hi = lo + 1.0;
        lo -= 1.0;
    }
    let pad = (hi - lo) * 0.08;
    lo -= pad;
    hi += pad;

    // Horizontal grid + axis labels.
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let y = rect.bottom() - t * rect.height();
        p.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, pal.grid.gamma_multiply(0.5)),
        );
        let val = lo + t * (hi - lo);
        p.text(
            Pos2::new(rect.left() + 3.0, y - 1.0),
            Align2::LEFT_BOTTOM,
            format!("{val:.1}"),
            FontId::monospace(9.0),
            pal.text_dim,
        );
    }

    // Polyline over the sample window.
    let n = history.len();
    let pts: Vec<Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = rect.left() + (i as f32 / (n - 1) as f32) * rect.width();
            let norm = (v - lo) / (hi - lo);
            let y = rect.bottom() - norm * rect.height();
            Pos2::new(x, y)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.5, line)));
}

