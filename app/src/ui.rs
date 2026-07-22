//! Native HWiNFO-style UI.
//!
//! This module renders the "Sensors" style window: a dense table of
//! Sensor | Current | Min | Max | Average rows, grouped under colored hardware
//! header rows, over a dark grid theme — visually modeled on HWiNFO64's
//! sensors window. Later branches add the System Summary, graphs, logging and
//! the tray icon.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Frame as EFrame, Margin, Panel, RichText};
use egui_extras::{Column, TableBuilder};

use crate::model::{Hardware, HardwareType, SensorType};
use crate::poll::Monitor;

// ---- HWiNFO-ish palette -------------------------------------------------

const BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
const BG_HEADER: Color32 = Color32::from_rgb(0x2d, 0x2d, 0x30);
const ROW_EVEN: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
const ROW_ODD: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
const TEXT: Color32 = Color32::from_rgb(0xe0, 0xe0, 0xe0);
const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);
const ACCENT: Color32 = Color32::from_rgb(0x37, 0x94, 0xff);
const VALUE: Color32 = Color32::from_rgb(0x4e, 0xc9, 0xb0);
const GRID: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);

const ROW_H: f32 = 19.0;
const VALUE_COL_W: f32 = 110.0;

pub struct SensorViewApp {
    monitor: Arc<Mutex<Monitor>>,
    source_name: String,
}

impl SensorViewApp {
    pub fn new(cc: &eframe::CreationContext<'_>, monitor: Arc<Mutex<Monitor>>) -> Self {
        apply_theme(&cc.egui_ctx);
        let source_name = monitor.lock().map(|m| m.source_name().to_string()).unwrap_or_default();
        Self { monitor, source_name }
    }
}

impl eframe::App for SensorViewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let tree = self.monitor.lock().map(|m| m.snapshot()).unwrap_or_default();
        let sensor_count: usize = tree.iter().map(count_sensors).sum();

        // ---- Toolbar --------------------------------------------------
        Panel::top("toolbar")
            .frame(EFrame::new().fill(BG_HEADER).inner_margin(Margin::symmetric(8, 5)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("SensorView").color(ACCENT).strong());
                    ui.label(RichText::new("Hardware Monitor").color(TEXT_DIM).size(11.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Reset Min/Max").size(11.0)).clicked() {
                            if let Ok(mut m) = self.monitor.lock() {
                                m.reset_min_max();
                            }
                        }
                    });
                });
            });

        // ---- Status bar -----------------------------------------------
        Panel::bottom("status")
            .frame(EFrame::new().fill(BG_HEADER).inner_margin(Margin::symmetric(8, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Source: {}", self.source_name)).color(TEXT_DIM).size(11.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{sensor_count} sensors ● live")).color(TEXT_DIM).size(11.0));
                    });
                });
            });

        // ---- Sensors table --------------------------------------------
        egui::CentralPanel::default()
            .frame(EFrame::new().fill(BG))
            .show(ui, |ui| {
                sensors_table(ui, &tree);
            });
    }
}

fn count_sensors(hw: &Hardware) -> usize {
    hw.sensors.len() + hw.sub_hardware.iter().map(count_sensors).sum::<usize>()
}

/// One flattened display row.
enum Row<'a> {
    Group(&'a Hardware),
    Sensor(&'a crate::model::Sensor),
}

fn flatten<'a>(tree: &'a [Hardware], out: &mut Vec<Row<'a>>) {
    for hw in tree {
        out.push(Row::Group(hw));
        for s in &hw.sensors {
            out.push(Row::Sensor(s));
        }
        flatten(&hw.sub_hardware, out);
    }
}

fn sensors_table(ui: &mut egui::Ui, tree: &[Hardware]) {
    let mut rows: Vec<Row> = Vec::new();
    flatten(tree, &mut rows);

    // Precompute zebra parity for sensor rows (group rows reset nothing).
    let mut parity = 0usize;
    let parities: Vec<usize> = rows
        .iter()
        .map(|r| match r {
            Row::Group(_) => 0,
            Row::Sensor(_) => {
                parity += 1;
                parity
            }
        })
        .collect();

    TableBuilder::new(ui)
        .column(Column::exact(340.0)) // Sensor name
        .columns(Column::exact(VALUE_COL_W), 4) // Current / Min / Max / Avg
        .header(ROW_H + 2.0, |mut header| {
            for (i, title) in ["Sensor", "Current", "Minimum", "Maximum", "Average"].iter().enumerate() {
                header.col(|ui| {
                    paint_row_bg(ui, BG_HEADER);
                    let text = RichText::new(*title).color(TEXT_DIM).size(11.0).strong();
                    if i == 0 {
                        ui.label(text);
                    } else {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(text);
                        });
                    }
                });
            }
        })
        .body(|body| {
            body.rows(ROW_H, rows.len(), |mut table_row| {
                let idx = table_row.index();
                match &rows[idx] {
                    Row::Group(hw) => {
                        table_row.col(|ui| {
                            paint_row_bg(ui, BG_HEADER);
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{} {}", hardware_glyph(hw.hardware_type), hw.name))
                                        .color(ACCENT)
                                        .size(12.0)
                                        .strong(),
                                );
                            });
                        });
                        for _ in 0..4 {
                            table_row.col(|ui| {
                                paint_row_bg(ui, BG_HEADER);
                            });
                        }
                    }
                    Row::Sensor(s) => {
                        let bg = if parities[idx] % 2 == 0 { ROW_EVEN } else { ROW_ODD };
                        table_row.col(|ui| {
                            paint_row_bg(ui, bg);
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add_space(16.0); // indent under the group header
                                ui.label(RichText::new(&s.name).color(TEXT).size(12.0));
                            });
                        });
                        let cells = [
                            (s.value, VALUE, true),
                            (s.min, TEXT, false),
                            (s.max, TEXT, false),
                            (s.avg, TEXT_DIM, false),
                        ];
                        for (val, color, strong) in cells {
                            table_row.col(|ui| {
                                paint_row_bg(ui, bg);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let mut t = RichText::new(format_value(val, s.sensor_type))
                                        .color(color)
                                        .size(12.0)
                                        .monospace();
                                    if strong {
                                        t = t.strong();
                                    }
                                    ui.label(t);
                                });
                            });
                        }
                    }
                }
            });
        });
}

/// Fill the full cell background (rows are built cell-by-cell in egui_extras).
fn paint_row_bg(ui: &mut egui::Ui, color: Color32) {
    let rect = ui.max_rect().expand2(egui::vec2(4.0, 2.0));
    ui.painter().rect_filled(rect, 0.0, color);
}

fn hardware_glyph(t: HardwareType) -> &'static str {
    match t {
        HardwareType::Cpu => "▣",
        HardwareType::GpuNvidia | HardwareType::GpuAti | HardwareType::GpuIntel => "▤",
        HardwareType::Ram => "▥",
        HardwareType::Mainboard | HardwareType::SuperIO | HardwareType::EmbeddedController => "▦",
        HardwareType::Hdd | HardwareType::Storage => "▧",
        HardwareType::Network => "▩",
        HardwareType::Battery | HardwareType::Psu => "▪",
        _ => "▨",
    }
}

/// HWiNFO-style value formatting: fixed decimals + unit.
fn format_value(value: Option<f32>, t: SensorType) -> String {
    let Some(v) = value else { return "—".to_string() };
    let decimals = match t {
        SensorType::Voltage => 3,
        SensorType::Fan | SensorType::SmallData => 0,
        _ => 1,
    };
    let unit = t.unit();
    if unit.is_empty() {
        format!("{v:.decimals$}")
    } else {
        format!("{v:.decimals$} {unit}")
    }
}

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = BG;
        style.visuals.extreme_bg_color = BG;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.widgets.noninteractive.bg_stroke.color = GRID;
        style.spacing.item_spacing = egui::vec2(4.0, 0.0);
    });
}
