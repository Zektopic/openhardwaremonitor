//! Slow-lane telemetry: everything that must NOT be polled at sensor cadence.
//!
//! Three independent reasons drive the split from [`crate::source::SensorSource`]:
//!
//! - **S.M.A.R.T. / NVMe log pages** keep drives from entering low-power states
//!   if polled continuously, and burn a limited log-read budget.
//! - **SPD / EC reads go over SMBus/I²C.** Polling that bus faster than ~2 Hz
//!   collides with firmware and other tools, triggering SMI storms that show up
//!   as audio dropouts and micro-stutter.
//! - **PCIe topology only changes on hotplug**, so re-enumerating every second
//!   is pure waste.
//!
//! Collected on a 30 s cadence by default (see [`crate::poll`]).

// Accessors for slow-lane data the M2/M3 panes and backends will consume; the
// shape is fixed now so those can be added without touching this module.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};

use crate::model::hexblob::HexBlob;
use crate::model::storage::StorageHealth;
use crate::model::topology::PciNode;

/// Everything the slow lane produces, published as one immutable unit.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    /// Monotonic counter — lets the UI and web clients tell a refreshed
    /// inventory from a carried-forward one without deep comparison.
    pub seq: u64,
    pub storage: Vec<StorageHealth>,
    /// Roots of the PCIe hierarchy (one per domain).
    pub topology: Vec<PciNode>,
    /// Raw dumps available to the Hex Viewer.
    pub hex: Vec<HexBlob>,
}

impl Inventory {
    /// Every degraded PCIe link across all domains — surfaced as a headline
    /// warning rather than something you have to go hunting for.
    pub fn degraded_links(&self) -> Vec<&PciNode> {
        self.topology.iter().flat_map(|r| r.degraded_links()).collect()
    }

    /// Drives that are not in `Good` health.
    pub fn unhealthy_drives(&self) -> Vec<&StorageHealth> {
        use crate::model::storage::HealthStatus;
        self.storage
            .iter()
            .filter(|d| !matches!(d.status, HealthStatus::Good | HealthStatus::Unknown))
            .collect()
    }
}

/// A backend that enumerates slow-changing hardware facts.
///
/// Deliberately separate from [`crate::source::SensorSource`] so the two
/// cadences can never be accidentally coupled — a type implementing both still
/// gets called on two different schedules.
pub trait InventorySource: Send {
    fn name(&self) -> &'static str;

    /// Enumerate. Called on the slow lane; may block for seconds.
    fn collect(&mut self) -> Inventory;
}

/// Placeholder used until the per-OS backends land (see the M3 milestone).
/// Returns an empty inventory so every consumer can be written against the real
/// shape today.
pub struct NullInventory;

impl InventorySource for NullInventory {
    fn name(&self) -> &'static str {
        "none"
    }

    fn collect(&mut self) -> Inventory {
        Inventory::default()
    }
}

/// Pick the best available inventory backend for this platform.
pub fn default_inventory() -> Box<dyn InventorySource> {
    Box::new(NullInventory)
}

/// Carry the previous inventory forward, bumping `seq` only when the backend
/// actually produced something — a backend that returned nothing (no
/// permissions, no drives enumerated) must not blank out good data.
pub fn carry_forward(previous: &Arc<Inventory>, fresh: Inventory) -> Arc<Inventory> {
    if fresh.storage.is_empty() && fresh.topology.is_empty() && fresh.hex.is_empty() {
        return previous.clone();
    }
    Arc::new(Inventory { seq: previous.seq + 1, ..fresh })
}

// ---------------------------------------------------------------------------
// Thread 1b — the slow lane
// ---------------------------------------------------------------------------

/// Handle to the slow-lane collector thread.
///
/// The collector runs on its own thread rather than inside the poll loop
/// specifically so a multi-second S.M.A.R.T. read can never delay a 1 s sensor
/// tick. It publishes into an [`ArcSwap`], so the poll thread picks up the
/// latest inventory with a lock-free pointer read.
pub struct InventoryHandle {
    latest: Arc<ArcSwap<Inventory>>,
    running: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl InventoryHandle {
    /// Latest collected inventory. Lock-free; safe to call every tick.
    pub fn latest(&self) -> Arc<Inventory> {
        self.latest.load_full()
    }

    /// A handle with no thread behind it — always returns an empty inventory.
    /// Used on platforms with no backend yet, and in tests.
    pub fn inert() -> Self {
        Self {
            latest: Arc::new(ArcSwap::from_pointee(Inventory::default())),
            running: Arc::new(AtomicBool::new(false)),
            join: None,
        }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for InventoryHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// How finely the collector checks for shutdown while idling between passes.
/// Keeps `stop()` responsive without busy-waiting on a 30 s cadence.
const SHUTDOWN_GRANULARITY: Duration = Duration::from_millis(200);

/// Spawn the slow lane. Collects immediately, then every `cadence`.
pub fn spawn_collector(mut source: Box<dyn InventorySource>, cadence: Duration) -> InventoryHandle {
    let latest = Arc::new(ArcSwap::from_pointee(Inventory::default()));
    let running = Arc::new(AtomicBool::new(true));

    let join = std::thread::Builder::new()
        .name("inventory-poll".into())
        .spawn({
            let latest = latest.clone();
            let running = running.clone();
            move || {
                while running.load(Ordering::Relaxed) {
                    let fresh = source.collect();
                    let current = latest.load_full();
                    latest.store(carry_forward(&current, fresh));

                    // Idle in small steps so shutdown is prompt.
                    let deadline = Instant::now() + cadence;
                    while running.load(Ordering::Relaxed) && Instant::now() < deadline {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        std::thread::sleep(remaining.min(SHUTDOWN_GRANULARITY));
                    }
                }
            }
        })
        .expect("spawning the inventory thread");

    InventoryHandle { latest, running, join: Some(join) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::storage::{HealthStatus, StorageProtocol};

    fn drive(id: &str, status: HealthStatus) -> StorageHealth {
        StorageHealth {
            identifier: id.into(),
            model: "Test SSD".into(),
            serial: "SN0".into(),
            firmware: "1.0".into(),
            protocol: StorageProtocol::Nvme,
            capacity_bytes: None,
            temperature_c: None,
            power_on_hours: None,
            power_cycles: None,
            life_remaining_pct: None,
            total_bytes_written: None,
            total_bytes_read: None,
            status,
            warnings: vec![],
            attributes: vec![],
            nvme: None,
        }
    }

    #[test]
    fn unhealthy_drives_ignores_good_and_unknown() {
        let inv = Inventory {
            storage: vec![
                drive("a", HealthStatus::Good),
                drive("b", HealthStatus::Caution),
                drive("c", HealthStatus::Unknown),
                drive("d", HealthStatus::Bad),
            ],
            ..Default::default()
        };
        let bad: Vec<_> = inv.unhealthy_drives().iter().map(|d| d.identifier.clone()).collect();
        assert_eq!(bad, ["b", "d"]);
    }

    #[test]
    fn empty_collection_carries_the_previous_inventory_forward() {
        let prev = Arc::new(Inventory { seq: 7, storage: vec![drive("a", HealthStatus::Good)], ..Default::default() });
        // A backend that returned nothing must not blank out good data.
        let same = carry_forward(&prev, Inventory::default());
        assert_eq!(same.seq, 7);
        assert_eq!(same.storage.len(), 1);
        assert!(Arc::ptr_eq(&prev, &same), "should reuse the same allocation");

        let fresh = carry_forward(&prev, Inventory { storage: vec![drive("b", HealthStatus::Bad)], ..Default::default() });
        assert_eq!(fresh.seq, 8);
        assert_eq!(fresh.storage[0].identifier, "b");
    }

    #[test]
    fn inert_handle_yields_an_empty_inventory() {
        let h = InventoryHandle::inert();
        assert_eq!(h.latest().seq, 0);
        assert!(h.latest().storage.is_empty());
    }

    /// Backend that reports one drive and counts how often it was asked.
    struct CountingSource(Arc<std::sync::atomic::AtomicUsize>);

    impl InventorySource for CountingSource {
        fn name(&self) -> &'static str {
            "counting"
        }
        fn collect(&mut self) -> Inventory {
            self.0.fetch_add(1, Ordering::Relaxed);
            Inventory { storage: vec![drive("a", HealthStatus::Good)], ..Default::default() }
        }
    }

    #[test]
    fn collector_publishes_then_stops_promptly() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // A long cadence: the test asserts the *first* pass lands immediately
        // and that stop() does not wait it out.
        let mut h = spawn_collector(Box::new(CountingSource(calls.clone())), Duration::from_secs(300));

        let deadline = Instant::now() + Duration::from_secs(5);
        while h.latest().seq == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(h.latest().seq, 1, "collected on startup, not after one cadence");
        assert_eq!(h.latest().storage.len(), 1);

        let t0 = Instant::now();
        h.stop();
        assert!(t0.elapsed() < Duration::from_secs(2), "stop took {:?}", t0.elapsed());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
