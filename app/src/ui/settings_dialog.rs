//! HWiNFO-style Settings dialog: tab strip with a fully functional
//! "General / User Interface" tab persisted via [`crate::settings`].

use eframe::egui::{self, Id, RichText};

use super::widgets::square_check;
use super::{Palette, Shared, WindowFlags};
use crate::settings::ColorMode;

#[derive(Clone, Copy, PartialEq, Default)]
enum Tab {
    #[default]
    General,
    Safety,
    Smbus,
    Driver,
    License,
}

pub fn show(ui: &mut egui::Ui, s: &Shared) {
    super::handle_close(ui, &s.windows.settings);
    let pal = s.palette();

    let tab_id = Id::new("settings_tab");
    let mut tab: Tab = ui.ctx().data_mut(|d| *d.get_temp_mut_or(tab_id, Tab::General));

    // ---- Bottom OK / Cancel ---------------------------------------------
    egui::Panel::bottom("settings_buttons")
        .frame(
            egui::Frame::new()
                .fill(pal.bg)
                .inner_margin(egui::Margin::symmetric(10, 6)),
        )
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    WindowFlags::close(&s.windows.settings);
                }
                if ui.button("OK").clicked() {
                    if let Ok(st) = s.settings.read() {
                        st.save();
                    }
                    WindowFlags::close(&s.windows.settings);
                }
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(pal.bg)
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ui, |ui| {
            // ---- Tab strip ---------------------------------------------
            ui.horizontal(|ui| {
                for (t, label) in [
                    (Tab::General, "General / User Interface"),
                    (Tab::Safety, "Safety"),
                    (Tab::Smbus, "SMBus / I2C"),
                    (Tab::Driver, "Driver Management"),
                    (Tab::License, "License Management"),
                ] {
                    let active = tab == t;
                    let text = RichText::new(label).size(11.0).color(if active {
                        pal.text
                    } else {
                        pal.text_dim
                    });
                    if ui.selectable_label(active, text).clicked() {
                        tab = t;
                        ui.ctx().data_mut(|d| d.insert_temp(tab_id, t));
                    }
                }
            });
            ui.separator();

            match tab {
                Tab::General => general_tab(ui, s, &pal),
                Tab::Safety => stub_tab(ui, &pal, "Safety options (watchdog, polling exclusions) arrive with the native sensor engine."),
                Tab::Smbus => stub_tab(ui, &pal, "SMBus / I2C device scanning arrives with the native sensor engine (SPD, Super-I/O)."),
                Tab::Driver => driver_tab(ui, s, &pal),
                Tab::License => stub_tab(ui, &pal, "SensorView is open source — no license management needed."),
            }
        });
}

fn general_tab(ui: &mut egui::Ui, s: &Shared, pal: &Palette) {
    let Ok(mut st) = s.settings.write() else { return };
    let mut changed = false;

    ui.columns(2, |cols| {
        let c = &mut cols[0];
        changed |= square_check(c, &mut st.show_summary_on_startup, "Show System Summary on Startup", pal);
        changed |= square_check(c, &mut st.show_sensors_on_startup, "Show Sensors on Startup", pal);
        changed |= square_check(c, &mut st.minimize_main_on_startup, "Minimize Main Window on Startup", pal);
        changed |= square_check(c, &mut st.minimize_sensors_on_startup, "Minimize Sensors on Startup", pal);
        changed |= square_check(c, &mut st.minimize_sensors_instead_of_closing, "Minimize Sensors instead of closing", pal);
        changed |= square_check(c, &mut st.show_welcome_screen, "Show Welcome Screen and Progress", pal);
        changed |= square_check(c, &mut st.validate_window_positions, "Validate Window Positions", pal);
        changed |= square_check(c, &mut st.auto_start, "Auto Start", pal);
        changed |= square_check(c, &mut st.automatic_update, "Automatic Update", pal);
        changed |= square_check(c, &mut st.flush_buffers_on_start, "Flush Buffers on Start", pal);
        changed |= square_check(c, &mut st.snapshot_cpu_polling, "Snapshot CPU Polling", pal);
        changed |= square_check(c, &mut st.shared_memory_support, "Shared Memory Support", pal);

        c.add_space(8.0);
        c.label(RichText::new("Language:").size(11.0).color(pal.text_dim));
        egui::ComboBox::from_id_salt("lang")
            .selected_text(st.language.clone())
            .show_ui(c, |ui| {
                ui.selectable_value(&mut st.language, "English".to_string(), "English");
            });

        let c = &mut cols[1];
        changed |= square_check(c, &mut st.wake_disabled_gpus, "Wake disabled GPUs", pal);
        changed |= square_check(c, &mut st.poll_sleeping_gpus, "Poll Sleeping GPUs", pal);
        changed |= square_check(c, &mut st.reorder_gpus, "Reorder GPUs", pal);
        changed |= square_check(c, &mut st.prefer_amd_adl, "Prefer AMD ADL", pal);
        changed |= square_check(c, &mut st.presentmon_support, "PresentMon Support", pal);
        changed |= square_check(c, &mut st.remember_preferences, "Remember Preferences", pal);

        c.add_space(10.0);
        c.group(|c| {
            c.label(RichText::new("Color Mode").size(11.0).color(pal.text_dim));
            for (mode, label) in [
                (ColorMode::Grey, "Default (Grey)"),
                (ColorMode::Black, "Default (Black)"),
                (ColorMode::Light, "Disabled (Light)"),
            ] {
                if c.radio(st.color_mode == mode, label).clicked() {
                    st.color_mode = mode;
                    changed = true;
                }
            }
        });

        c.add_space(6.0);
        c.horizontal(|c| {
            if c.button("Backup User Settings").clicked() {
                st.save();
            }
            if c.button("Reset Preferences").clicked() {
                *st = crate::settings::AppSettings::default();
                changed = true;
            }
        });
    });

    if changed {
        st.save();
    }
}

fn driver_tab(ui: &mut egui::Ui, s: &Shared, pal: &Palette) {
    let (source, diag) = s
        .monitor
        .lock()
        .map(|m| (m.source_name().to_string(), m.diagnostics()))
        .unwrap_or_default();

    ui.add_space(6.0);
    ui.label(RichText::new("Sensor Engine").color(pal.accent).strong());
    ui.label(RichText::new(format!("Active source: {source}")).size(11.5).color(pal.text));
    if !diag.engine_version.is_empty() {
        ui.label(RichText::new(&diag.engine_version).size(11.0).color(pal.text_dim));
    }

    // Elevation status badge.
    ui.add_space(4.0);
    super::widgets::badge(ui, "Running as Administrator:", diag.elevated, pal);

    // Guidance depends on what's actually wrong.
    ui.add_space(6.0);
    let blocked = diag.driver_report.to_lowercase().contains("blocked")
        || diag.driver_report.to_lowercase().contains("not signed")
        || diag.driver_report.to_lowercase().contains("failed to load");
    if diag.elevated == Some(false) {
        ui.label(
            RichText::new(
                "⚠ Not elevated. CPU package/core power, effective clocks, Tctl/Tdie and \
                 fan/voltage sensors need Administrator rights. The release build elevates \
                 automatically at launch — or right-click SensorView → Run as administrator.",
            )
            .size(11.0)
            .color(pal.warn),
        );
    } else if blocked {
        ui.label(
            RichText::new(
                "⚠ The kernel driver was blocked from loading. On Windows 11 the \
                 vulnerable-driver blocklist (and Memory Integrity / HVCI) can block the \
                 classic WinRing0 driver. Installing PawnIO (a signed, blocklist-clean \
                 driver LibreHardwareMonitor can use) restores full sensor access.",
            )
            .size(11.0)
            .color(pal.warn),
        );
        ui.hyperlink_to("Get PawnIO", "https://pawnio.eu/");
    } else if diag.elevated == Some(true) {
        ui.label(
            RichText::new("✓ Elevated and the kernel driver is available — full sensor access.")
                .size(11.0)
                .color(pal.ok_badge),
        );
    }

    // Raw driver report for troubleshooting.
    if !diag.driver_report.is_empty() && diag.driver_report != "(no ring0 section in report)" {
        ui.add_space(8.0);
        ui.collapsing(RichText::new("Kernel driver report").size(11.0).color(pal.text_dim), |ui| {
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                ui.label(
                    RichText::new(&diag.driver_report)
                        .size(10.0)
                        .monospace()
                        .color(pal.text_dim),
                );
            });
        });
    }
}

fn stub_tab(ui: &mut egui::Ui, pal: &Palette, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).size(11.5).color(pal.text_dim));
}
