//! LibreHardwareMonitor bridge backend (Windows).
//!
//! Spawns the bundled .NET sidecar (`sensorview-bridge.exe`, see `sidecar/`)
//! which prints one JSON hardware-tree snapshot per line on stdout, in exactly
//! the shape of [`crate::model`]. A reader thread keeps the latest parsed tree;
//! [`SensorSource::snapshot`] hands out clones of it.
//!
//! Full coverage (Super-I/O, MSR, SMBus) requires the app to run elevated —
//! the release build carries a `requireAdministrator` manifest, matching
//! HWiNFO's own behavior. Without elevation the sidecar still reports the
//! subset it can reach (GPU, storage, network, battery, …).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::model::Hardware;

pub struct LhmBridge {
    child: Child,
    latest: Arc<Mutex<Vec<Hardware>>>,
}

const SIDECAR_EXE: &str = "sensorview-bridge.exe";

impl LhmBridge {
    /// Spawn the sidecar and wait (briefly) for its first snapshot.
    pub fn spawn() -> Result<Self, String> {
        let exe = find_sidecar().ok_or_else(|| format!("{SIDECAR_EXE} not found"))?;

        let mut cmd = Command::new(&exe);
        cmd.stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {e}", exe.display()))?;
        let stdout = child.stdout.take().ok_or("sidecar stdout unavailable")?;

        let latest: Arc<Mutex<Vec<Hardware>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = latest.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(tree) = serde_json::from_str::<Vec<Hardware>>(&line) {
                    if let Ok(mut slot) = sink.lock() {
                        *slot = tree;
                    }
                }
            }
        });

        // LHM's Computer.Open() enumerates all buses; give it a moment. If the
        // sidecar dies or stays silent, report failure so the caller can fall
        // back to another source.
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if !latest.lock().map(|t| t.is_empty()).unwrap_or(true) {
                return Ok(Self { child, latest });
            }
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("sidecar exited early: {status}"));
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                return Err("sidecar produced no snapshot within 15 s".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl super::SensorSource for LhmBridge {
    fn name(&self) -> &'static str {
        "LibreHardwareMonitor bridge"
    }

    fn snapshot(&mut self) -> Vec<Hardware> {
        self.latest.lock().map(|t| t.clone()).unwrap_or_default()
    }
}

impl Drop for LhmBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Locate the sidecar: next to our exe (packaged install), then the dev
/// publish folder (repo checkout).
fn find_sidecar() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            candidates.push(dir.join(SIDECAR_EXE));
            candidates.push(dir.join("sidecar").join(SIDECAR_EXE));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("sidecar")
            .join("publish")
            .join(SIDECAR_EXE),
    );
    candidates.into_iter().find(|p| p.is_file())
}
