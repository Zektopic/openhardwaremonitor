//! Polling engine: turns raw [`SensorSource`] snapshots into an enriched tree
//! with running min / max / average and a bounded history per sensor.
//!
//! The UI never talks to a source directly; it reads the [`Monitor`]'s latest
//! enriched snapshot (and per-sensor history for graphs).

use std::collections::{HashMap, VecDeque};

use crate::model::Hardware;
use crate::source::SensorSource;

/// Number of samples kept per sensor for the history graphs.
/// At the default 1 Hz poll this is ~10 minutes.
pub const HISTORY_LEN: usize = 600;

/// Running statistics for a single sensor, keyed by its identifier.
struct Stat {
    min: f32,
    max: f32,
    sum: f64,
    count: u64,
    history: VecDeque<f32>,
}

impl Stat {
    fn new(v: f32) -> Self {
        let mut history = VecDeque::with_capacity(HISTORY_LEN);
        history.push_back(v);
        Self { min: v, max: v, sum: v as f64, count: 1, history }
    }

    fn update(&mut self, v: f32) {
        self.min = self.min.min(v);
        self.max = self.max.max(v);
        self.sum += v as f64;
        self.count += 1;
        if self.history.len() == HISTORY_LEN {
            self.history.pop_front();
        }
        self.history.push_back(v);
    }

    fn avg(&self) -> f32 {
        if self.count == 0 { 0.0 } else { (self.sum / self.count as f64) as f32 }
    }
}

/// Owns the active source, the per-sensor statistics, and the most recent
/// enriched snapshot. Guarded by a `Mutex` at the app level.
pub struct Monitor {
    source: Box<dyn SensorSource>,
    stats: HashMap<String, Stat>,
    latest: Vec<Hardware>,
}

impl Monitor {
    pub fn new(source: Box<dyn SensorSource>) -> Self {
        Self { source, stats: HashMap::new(), latest: Vec::new() }
    }

    pub fn source_name(&self) -> &'static str {
        self.source.name()
    }

    pub fn diagnostics(&self) -> crate::source::Diagnostics {
        self.source.diagnostics()
    }

    /// Poll the source once and fold the readings into the running statistics.
    /// Returns the freshly enriched tree (also cached as `latest`).
    pub fn poll(&mut self) -> Vec<Hardware> {
        let mut tree = self.source.snapshot();
        for hw in &mut tree {
            Self::enrich(hw, &mut self.stats);
        }
        self.latest = tree.clone();
        tree
    }

    fn enrich(hw: &mut Hardware, stats: &mut HashMap<String, Stat>) {
        for s in &mut hw.sensors {
            if let Some(v) = s.value {
                // Seed on first sight (count = 1), otherwise fold in the new reading.
                let stat = stats
                    .entry(s.identifier.clone())
                    .and_modify(|st| st.update(v))
                    .or_insert_with(|| Stat::new(v));
                s.min = Some(stat.min);
                s.max = Some(stat.max);
                s.avg = Some(stat.avg());
            }
        }
        for sub in &mut hw.sub_hardware {
            Self::enrich(sub, stats);
        }
    }

    /// Latest enriched snapshot (cheap clone for the UI).
    pub fn snapshot(&self) -> Vec<Hardware> {
        self.latest.clone()
    }

    /// History samples for one sensor identifier (oldest → newest).
    /// Consumed by the graphs view (feature/ui-graphs-logging).
    #[allow(dead_code)]
    pub fn history(&self, identifier: &str) -> Vec<f32> {
        self.stats
            .get(identifier)
            .map(|s| s.history.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Reset min/max/avg accumulation (keeps history). Mirrors OHM's Reset.
    pub fn reset_min_max(&mut self) {
        for stat in self.stats.values_mut() {
            let last = stat.history.back().copied().unwrap_or(0.0);
            stat.min = last;
            stat.max = last;
            stat.sum = last as f64;
            stat.count = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Hardware, HardwareType, Sensor, SensorType};

    /// Source that emits one temperature sensor cycling through a fixed sequence.
    struct SeqSource {
        values: Vec<f32>,
        i: usize,
    }

    impl SensorSource for SeqSource {
        fn name(&self) -> &'static str { "seq" }
        fn snapshot(&mut self) -> Vec<Hardware> {
            let v = self.values[self.i % self.values.len()];
            self.i += 1;
            vec![Hardware {
                identifier: "/t/0".into(),
                name: "t".into(),
                hardware_type: HardwareType::Cpu,
                sensors: vec![Sensor {
                    identifier: "/t/0/temperature/0".into(),
                    name: "temp".into(),
                    sensor_type: SensorType::Temperature,
                    index: 0,
                    value: Some(v),
                    min: None, max: None, avg: None,
                }],
                sub_hardware: vec![],
            }]
        }
    }

    fn only_sensor(tree: &[Hardware]) -> Sensor {
        tree[0].sensors[0].clone()
    }

    #[test]
    fn tracks_min_max_avg_over_polls() {
        let mut m = Monitor::new(Box::new(SeqSource { values: vec![10.0, 30.0, 20.0], i: 0 }));

        let s = only_sensor(&m.poll()); // 10
        assert_eq!((s.min, s.max, s.avg), (Some(10.0), Some(10.0), Some(10.0)));

        m.poll(); // 30
        let s = only_sensor(&m.poll()); // 20 -> seen 10,30,20
        assert_eq!(s.min, Some(10.0));
        assert_eq!(s.max, Some(30.0));
        assert_eq!(s.avg, Some(20.0));

        assert_eq!(m.history("/t/0/temperature/0"), vec![10.0, 30.0, 20.0]);
    }

    #[test]
    fn reset_min_max_rebases_to_latest() {
        let mut m = Monitor::new(Box::new(SeqSource { values: vec![10.0, 30.0], i: 0 }));
        m.poll(); // 10
        m.poll(); // 30
        m.reset_min_max();
        // History is preserved even though min/max were rebased.
        assert_eq!(m.history("/t/0/temperature/0"), vec![10.0, 30.0]);
        let s = only_sensor(&m.poll()); // 10 again
        assert_eq!(s.min, Some(10.0));
        assert_eq!(s.max, Some(30.0)); // 30 was the rebase point, 10 < 30
    }
}
