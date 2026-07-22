//! Synthetic sensor source.
//!
//! Produces a small but realistic-looking hardware tree whose values wander over
//! time, so the polling engine, min/max/avg tracking, history graphs and the UI
//! can all be exercised without any drivers or elevation. Replaced at runtime by
//! a real backend when one is available.

use crate::model::{Hardware, HardwareType, Sensor, SensorType};

pub struct DemoSource {
    tick: u64,
}

impl DemoSource {
    pub fn new() -> Self {
        Self { tick: 0 }
    }
}

impl super::SensorSource for DemoSource {
    fn name(&self) -> &'static str {
        "Demo (synthetic data)"
    }

    fn snapshot(&mut self) -> Vec<Hardware> {
        self.tick = self.tick.wrapping_add(1);
        let t = self.tick as f32;

        // Cheap deterministic wobble so values move without needing rand.
        let wob = |phase: f32, amp: f32, base: f32| base + (t * 0.15 + phase).sin() * amp;

        let sensor = |id: &str, name: &str, ty: SensorType, index: u32, value: f32| Sensor {
            identifier: id.to_string(),
            name: name.to_string(),
            sensor_type: ty,
            index,
            value: Some(value),
            // min/max/avg are filled in by the Monitor; leave None here.
            min: None,
            max: None,
            avg: None,
        };

        let cpu = Hardware {
            identifier: "/demo/cpu/0".into(),
            name: "CPU Package".into(),
            hardware_type: HardwareType::Cpu,
            sensors: vec![
                sensor("/demo/cpu/0/temperature/0", "CPU Package", SensorType::Temperature, 0, wob(0.0, 8.0, 48.0)),
                sensor("/demo/cpu/0/load/0", "Total CPU Usage", SensorType::Load, 0, wob(1.0, 30.0, 35.0).max(0.0)),
                sensor("/demo/cpu/0/clock/0", "Core #1", SensorType::Clock, 0, wob(2.0, 400.0, 3800.0)),
                sensor("/demo/cpu/0/power/0", "CPU Package Power", SensorType::Power, 0, wob(3.0, 25.0, 45.0).max(0.0)),
                sensor("/demo/cpu/0/voltage/0", "CPU Core", SensorType::Voltage, 0, wob(4.0, 0.05, 1.20)),
            ],
            sub_hardware: vec![],
        };

        let gpu = Hardware {
            identifier: "/demo/gpu/0".into(),
            name: "GPU".into(),
            hardware_type: HardwareType::GpuNvidia,
            sensors: vec![
                sensor("/demo/gpu/0/temperature/0", "GPU Core", SensorType::Temperature, 0, wob(5.0, 10.0, 55.0)),
                sensor("/demo/gpu/0/load/0", "GPU Core", SensorType::Load, 0, wob(6.0, 40.0, 40.0).max(0.0)),
                sensor("/demo/gpu/0/clock/0", "GPU Core", SensorType::Clock, 0, wob(7.0, 300.0, 1800.0)),
                sensor("/demo/gpu/0/fan/0", "GPU Fan", SensorType::Fan, 0, wob(8.0, 300.0, 1400.0).max(0.0)),
            ],
            sub_hardware: vec![],
        };

        let ram = Hardware {
            identifier: "/demo/ram/0".into(),
            name: "Memory".into(),
            hardware_type: HardwareType::Ram,
            sensors: vec![
                sensor("/demo/ram/0/load/0", "Memory Used", SensorType::Load, 0, wob(9.0, 8.0, 52.0).max(0.0)),
                sensor("/demo/ram/0/data/0", "Memory Used", SensorType::Data, 0, wob(10.0, 1.5, 16.5).max(0.0)),
            ],
            sub_hardware: vec![],
        };

        vec![cpu, gpu, ram]
    }
}
