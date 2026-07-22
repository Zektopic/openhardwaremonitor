//! Sensor data sources.
//!
//! Everything the app displays comes through the [`SensorSource`] trait, so the
//! backend that actually reads hardware can change without touching the polling
//! engine or the UI. Two backends are planned:
//!
//! - `demo` — synthetic data (this branch), keeps the app useful with no drivers.
//! - `lhm_bridge` — LibreHardwareMonitor .NET sidecar (feature/lhm-bridge).
//! - `native` — pure-Rust engine (feature/native-*).

pub mod demo;

use crate::model::Hardware;

/// A backend that can produce a full snapshot of the machine's hardware tree.
///
/// Implementations are polled on a fixed interval by [`crate::poll::Monitor`];
/// they should return current instantaneous values and leave min/max/avg
/// tracking to the monitor.
pub trait SensorSource: Send {
    /// Human-readable backend name (shown in the status bar / about box).
    fn name(&self) -> &'static str;

    /// Read a fresh snapshot of the hardware tree. Called once per poll tick.
    fn snapshot(&mut self) -> Vec<Hardware>;
}

/// Pick the best available source for the current build/platform.
///
/// Today this is always the demo source; feature/lhm-bridge makes this return
/// the LHM bridge on Windows (falling back to demo if the sidecar is missing).
pub fn default_source() -> Box<dyn SensorSource> {
    Box::new(demo::DemoSource::new())
}
