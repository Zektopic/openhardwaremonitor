//! SensorView library entry point (shared by the desktop binary).
//!
//! Scaffold stage: exposes `app_info` (a backend liveness probe) and
//! `demo_snapshot` (static sample data so the UI shell renders end-to-end).
//! The real polling engine and `SensorSource` backends land on
//! feature/sensor-core and feature/lhm-bridge.

mod model;

use model::{Hardware, HardwareType, Sensor, SensorType};

#[tauri::command]
fn app_info() -> String {
    format!("SensorView {} — scaffold OK", env!("CARGO_PKG_VERSION"))
}

/// Static sample tree so the frontend has shape to render before real sensors
/// are wired in. Replaced by live polling on feature/sensor-core.
#[tauri::command]
fn demo_snapshot() -> Vec<Hardware> {
    let sensor = |id: &str, name: &str, t: SensorType, i: u32, v: f32| Sensor {
        identifier: id.to_string(),
        name: name.to_string(),
        sensor_type: t,
        index: i,
        value: Some(v),
        min: Some(v),
        max: Some(v),
        avg: Some(v),
    };

    vec![Hardware {
        identifier: "/cpu/0".into(),
        name: "CPU".into(),
        hardware_type: HardwareType::Cpu,
        sensors: vec![
            sensor("/cpu/0/temperature/0", "Core Max", SensorType::Temperature, 0, 42.0),
            sensor("/cpu/0/clock/0", "Core #1", SensorType::Clock, 0, 3800.0),
            sensor("/cpu/0/load/0", "Total", SensorType::Load, 0, 7.5),
        ],
        sub_hardware: vec![],
    }]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info, demo_snapshot])
        .run(tauri::generate_context!())
        .expect("error while running SensorView");
}
