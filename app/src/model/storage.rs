//! Storage health model: NVMe Health Information Log (Log Identifier 0x02) and
//! ATA/SATA S.M.A.R.T. attributes.
//!
//! These types are protocol-shaped, not OS-shaped, because the telemetry is
//! produced by the *drive firmware* — the only per-OS difference is how the
//! command is delivered (`IOCTL_STORAGE_PROTOCOL_COMMAND` on Windows, an
//! `ioctl` on a `/dev/nvme*` node on Linux, IOKit on macOS). Each backend fills
//! the same structs, so the UI and the web dashboard never branch on platform.
//!
//! Storage health is collected on the **slow lane** (30–60 s, see
//! [`crate::inventory`]): polling S.M.A.R.T. at sensor cadence keeps drives from
//! entering low-power states and wastes their limited log-read budget.

// A transcription of hardware specifications: the NVMe 1.4 health log and the
// ATA attribute tables are carried in full, so a backend can be added without
// first extending the model. Exercised by the tests below until M3 lands.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Which command set produced a [`StorageHealth`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageProtocol {
    Ata,
    Nvme,
    Scsi,
}

/// Overall assessment, mirroring the tri-state used by drive-health tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Good,
    /// Degrading — e.g. remaining life ≤ 10 %, or any reallocated/pending sector.
    Caution,
    /// A threshold has been crossed, or the drive reports a critical warning.
    Bad,
    Unknown,
}

/// One drive's health, whichever protocol it speaks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealth {
    /// Stable id, matching the sensor tree's hardware identifier where possible.
    pub identifier: String,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub protocol: StorageProtocol,
    pub capacity_bytes: Option<u64>,
    pub temperature_c: Option<f32>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    /// Estimated remaining endurance, 0–100 %.
    pub life_remaining_pct: Option<f32>,
    pub total_bytes_written: Option<u128>,
    pub total_bytes_read: Option<u128>,
    pub status: HealthStatus,
    /// Human-readable reasons behind a non-`Good` status.
    pub warnings: Vec<String>,
    /// ATA/SATA only.
    pub attributes: Vec<SmartAttribute>,
    /// NVMe only.
    pub nvme: Option<NvmeHealthLog>,
}

// ---------------------------------------------------------------------------
// ATA / SATA S.M.A.R.T.
// ---------------------------------------------------------------------------

/// Direction in which a healthy value moves. Vendors disagree on this per
/// attribute, so it is carried alongside the reading rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ideal {
    Low,
    High,
    /// Informational counter (power-on hours, start/stop count).
    None,
}

/// A single S.M.A.R.T. attribute row.
///
/// `current`/`worst` are normalized values that start at a vendor baseline
/// (commonly 100, 200 or 253) and degrade toward `threshold`. `raw` is the
/// vendor-defined payload and is the only field with real physical meaning for
/// many attributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartAttribute {
    pub id: u8,
    /// Owned rather than `&'static str` so the struct round-trips through
    /// serde — vendor tables supply the canonical name at construction.
    pub name: String,
    pub current: u8,
    pub worst: u8,
    pub threshold: u8,
    pub raw: u64,
    pub ideal: Ideal,
}

impl SmartAttribute {
    /// A normalized value at or below its threshold is a failure indication.
    pub fn failing(&self) -> bool {
        self.threshold > 0 && self.current <= self.threshold
    }
}

/// Canonical name and ideal direction for a standard ATA attribute id.
///
/// Returns `None` for ids that are vendor-specific; those are resolved by
/// [`vendor_attribute_name`] once the controller is known.
pub fn attribute_name(id: u8) -> Option<(&'static str, Ideal)> {
    let entry = match id {
        0x01 => ("Read Error Rate", Ideal::Low),
        0x02 => ("Throughput Performance", Ideal::High),
        0x03 => ("Spin-Up Time", Ideal::Low),
        0x04 => ("Start/Stop Count", Ideal::None),
        0x05 => ("Reallocated Sectors Count", Ideal::Low),
        0x06 => ("Read Channel Margin", Ideal::None),
        0x07 => ("Seek Error Rate", Ideal::Low),
        0x08 => ("Seek Time Performance", Ideal::High),
        0x09 => ("Power-On Hours", Ideal::None),
        0x0A => ("Spin Retry Count", Ideal::Low),
        0x0B => ("Recalibration Retries", Ideal::Low),
        0x0C => ("Power Cycle Count", Ideal::None),
        0x0D => ("Soft Read Error Rate", Ideal::Low),
        0x16 => ("Current Helium Level", Ideal::High),
        0x17 => ("Helium Condition Lower", Ideal::High),
        0x18 => ("Helium Condition Upper", Ideal::High),
        0xB7 => ("SATA Downshift Error Count", Ideal::Low),
        0xB8 => ("End-to-End Error", Ideal::Low),
        0xBB => ("Reported Uncorrectable Errors", Ideal::Low),
        0xBC => ("Command Timeout", Ideal::Low),
        0xBD => ("High Fly Writes", Ideal::Low),
        0xBE => ("Airflow Temperature", Ideal::Low),
        0xBF => ("G-Sense Error Rate", Ideal::Low),
        0xC0 => ("Power-Off Retract Count", Ideal::Low),
        0xC1 => ("Load/Unload Cycle Count", Ideal::Low),
        0xC2 => ("Temperature", Ideal::Low),
        0xC3 => ("Hardware ECC Recovered", Ideal::None),
        0xC4 => ("Reallocation Event Count", Ideal::Low),
        0xC5 => ("Current Pending Sector Count", Ideal::Low),
        0xC6 => ("Uncorrectable Sector Count", Ideal::Low),
        0xC7 => ("UltraDMA CRC Error Rate", Ideal::Low),
        0xC8 => ("Write Error Rate", Ideal::Low),
        0xF1 => ("Lifetime Writes from Host", Ideal::None),
        0xF2 => ("Lifetime Reads from Host", Ideal::None),
        _ => return None,
    };
    Some(entry)
}

/// SSD controller family, used to resolve the vendor-specific endurance
/// attribute. SATA SSDs fragmented the S.M.A.R.T. spec badly: the same id means
/// different things per controller, so remaining-life needs this lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SsdVendor {
    Intel,
    Samsung,
    Micron,
    Kingston,
    SandForce,
    Indilinx,
    JMicron,
    Mtron,
    Unknown,
}

/// Vendor-specific attribute name for `id`, if that vendor overloads it.
pub fn vendor_attribute_name(vendor: SsdVendor, id: u8) -> Option<&'static str> {
    let name = match (vendor, id) {
        (SsdVendor::Intel, 0xE8) => "Available Reserved Space",
        (SsdVendor::Mtron, 0xBB) => "Total Erase Count",
        (SsdVendor::Samsung, 0xB4) => "Unused Reserved Block Count",
        (SsdVendor::Indilinx, 0xD1) => "Remaining Drive Life",
        (SsdVendor::SandForce, 0xE7) => "SSD Life Left",
        (SsdVendor::JMicron, 0xAA) => "Bad Block Count",
        (SsdVendor::Micron, 0xCA) => "Percentage of Rated Lifetime Used",
        (SsdVendor::Kingston, 0xAB) => "SSD Program Fail Count",
        _ => return None,
    };
    Some(name)
}

/// The attribute id carrying remaining endurance for a controller family, and
/// whether its normalized value counts *down* from 100 (life left) or *up*
/// (life used).
pub fn endurance_attribute(vendor: SsdVendor) -> Option<(u8, EnduranceSense)> {
    let entry = match vendor {
        SsdVendor::Intel => (0xE8, EnduranceSense::Remaining),
        SsdVendor::Indilinx => (0xD1, EnduranceSense::Remaining),
        SsdVendor::SandForce => (0xE7, EnduranceSense::Remaining),
        SsdVendor::Samsung => (0xB4, EnduranceSense::Remaining),
        SsdVendor::Micron => (0xCA, EnduranceSense::Consumed),
        _ => return None,
    };
    Some(entry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnduranceSense {
    /// Value *is* the percentage of life left.
    Remaining,
    /// Value is the percentage of life consumed; life left = 100 − value.
    Consumed,
}

// ---------------------------------------------------------------------------
// NVMe Health Information Log — Log Identifier 0x02
// ---------------------------------------------------------------------------

/// Bit positions in the NVMe Critical Warning byte (offset 0).
pub mod critical_warning {
    pub const SPARE_BELOW_THRESHOLD: u8 = 1 << 0;
    pub const TEMPERATURE: u8 = 1 << 1;
    pub const RELIABILITY_DEGRADED: u8 = 1 << 2;
    pub const READ_ONLY: u8 = 1 << 3;
    pub const VOLATILE_BACKUP_FAILED: u8 = 1 << 4;
}

/// Decoded NVMe SMART / Health Information log page.
///
/// NVMe abandoned the fragmented per-vendor attribute ids in favour of one
/// fixed 512-byte structure, so no translation table is needed — only correct
/// offsets and little-endian decoding.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NvmeHealthLog {
    pub critical_warning: u8,
    /// Raw composite temperature in Kelvin, as reported.
    pub composite_temp_k: u16,
    pub available_spare_pct: u8,
    pub available_spare_threshold_pct: u8,
    /// Vendor estimate of endurance consumed. May legitimately exceed 100.
    pub percentage_used: u8,
    /// Counted in units of 1000 × 512 bytes.
    pub data_units_read: u128,
    pub data_units_written: u128,
    pub host_read_commands: u128,
    pub host_write_commands: u128,
    /// Minutes the controller spent executing I/O.
    pub controller_busy_minutes: u128,
    pub power_cycles: u128,
    pub power_on_hours: u128,
    pub unsafe_shutdowns: u128,
    pub media_errors: u128,
    pub error_log_entries: u128,
    pub warning_temp_minutes: u32,
    pub critical_temp_minutes: u32,
}

/// One NVMe data unit as defined by the spec: 1000 × 512 bytes.
pub const NVME_DATA_UNIT_BYTES: u128 = 1000 * 512;

impl NvmeHealthLog {
    /// Parse the 512-byte log page. Returns `None` if the buffer is short.
    ///
    /// All multi-byte fields are little-endian. Offsets follow the NVMe 1.4
    /// specification, §5.14.1.2 (SMART / Health Information).
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 512 {
            return None;
        }
        // 16-byte little-endian counters occupy most of the page.
        let u128_at = |off: usize| -> u128 {
            let mut b = [0u8; 16];
            b.copy_from_slice(&buf[off..off + 16]);
            u128::from_le_bytes(b)
        };
        Some(Self {
            critical_warning: buf[0],
            composite_temp_k: u16::from_le_bytes([buf[1], buf[2]]),
            available_spare_pct: buf[3],
            available_spare_threshold_pct: buf[4],
            percentage_used: buf[5],
            data_units_read: u128_at(32),
            data_units_written: u128_at(48),
            host_read_commands: u128_at(64),
            host_write_commands: u128_at(80),
            controller_busy_minutes: u128_at(96),
            power_cycles: u128_at(112),
            power_on_hours: u128_at(128),
            unsafe_shutdowns: u128_at(144),
            media_errors: u128_at(160),
            error_log_entries: u128_at(176),
            warning_temp_minutes: u32::from_le_bytes([buf[192], buf[193], buf[194], buf[195]]),
            critical_temp_minutes: u32::from_le_bytes([buf[196], buf[197], buf[198], buf[199]]),
        })
    }

    /// Composite temperature in °C. `None` when the drive reports 0 K, which
    /// means "not implemented" rather than absolute zero.
    pub fn temperature_c(&self) -> Option<f32> {
        (self.composite_temp_k > 0).then_some(self.composite_temp_k as f32 - 273.15)
    }

    pub fn bytes_written(&self) -> u128 {
        self.data_units_written * NVME_DATA_UNIT_BYTES
    }

    pub fn bytes_read(&self) -> u128 {
        self.data_units_read * NVME_DATA_UNIT_BYTES
    }

    /// Endurance left, derived from `percentage_used`. Saturates at 0 because
    /// the spec allows the field to exceed 100.
    pub fn life_remaining_pct(&self) -> f32 {
        100.0 - (self.percentage_used as f32).min(100.0)
    }

    /// Human-readable decoding of the critical-warning bitfield.
    pub fn warnings(&self) -> Vec<String> {
        use critical_warning as cw;
        let mut out = Vec::new();
        let w = self.critical_warning;
        if w & cw::SPARE_BELOW_THRESHOLD != 0 {
            out.push("Available spare capacity is below the vendor threshold".into());
        }
        if w & cw::TEMPERATURE != 0 {
            out.push("Temperature is outside the over/under-temperature thresholds".into());
        }
        if w & cw::RELIABILITY_DEGRADED != 0 {
            out.push("NVM subsystem reliability degraded by media errors".into());
        }
        if w & cw::READ_ONLY != 0 {
            out.push("Media has been placed in read-only mode".into());
        }
        if w & cw::VOLATILE_BACKUP_FAILED != 0 {
            out.push("Volatile memory backup device has failed".into());
        }
        out
    }

    /// Overall status from the warning bitfield and remaining endurance.
    pub fn status(&self) -> HealthStatus {
        if self.critical_warning != 0 {
            return HealthStatus::Bad;
        }
        if self.available_spare_pct <= self.available_spare_threshold_pct
            || self.life_remaining_pct() <= 10.0
        {
            return HealthStatus::Caution;
        }
        HealthStatus::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a log page with the fields we assert on, leaving the rest zeroed.
    fn fixture() -> Vec<u8> {
        let mut b = vec![0u8; 512];
        b[0] = critical_warning::RELIABILITY_DEGRADED;
        b[1..3].copy_from_slice(&313u16.to_le_bytes()); // 313 K ≈ 39.85 °C
        b[3] = 100; // available spare
        b[4] = 10; // spare threshold
        b[5] = 7; // percentage used
        b[32..48].copy_from_slice(&40_000_000u128.to_le_bytes()); // data units read
        b[48..64].copy_from_slice(&20_000_000u128.to_le_bytes()); // data units written
        b[112..128].copy_from_slice(&1_234u128.to_le_bytes()); // power cycles
        b[128..144].copy_from_slice(&9_876u128.to_le_bytes()); // power on hours
        b[144..160].copy_from_slice(&3u128.to_le_bytes()); // unsafe shutdowns
        b[192..196].copy_from_slice(&42u32.to_le_bytes()); // warning temp minutes
        b
    }

    #[test]
    fn parses_log_page_fields_at_spec_offsets() {
        let log = NvmeHealthLog::parse(&fixture()).expect("512-byte page parses");
        assert_eq!(log.composite_temp_k, 313);
        assert_eq!(log.available_spare_pct, 100);
        assert_eq!(log.percentage_used, 7);
        assert_eq!(log.data_units_written, 20_000_000);
        assert_eq!(log.power_cycles, 1_234);
        assert_eq!(log.power_on_hours, 9_876);
        assert_eq!(log.unsafe_shutdowns, 3);
        assert_eq!(log.warning_temp_minutes, 42);
        assert_eq!(log.critical_temp_minutes, 0);
    }

    #[test]
    fn derives_temperature_endurance_and_tbw() {
        let log = NvmeHealthLog::parse(&fixture()).unwrap();
        // Kelvin → Celsius.
        assert!((log.temperature_c().unwrap() - 39.85).abs() < 0.01);
        // 100 − percentage_used.
        assert!((log.life_remaining_pct() - 93.0).abs() < f32::EPSILON);
        // 20e6 data units × 1000 × 512 B = 10.24 TB written.
        assert_eq!(log.bytes_written(), 20_000_000 * 1000 * 512);
        assert_eq!(log.bytes_written() as f64 / 1e12, 10.24);
    }

    #[test]
    fn decodes_critical_warning_bitfield() {
        let log = NvmeHealthLog::parse(&fixture()).unwrap();
        assert_eq!(log.status(), HealthStatus::Bad);
        let warnings = log.warnings();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("reliability degraded"));
    }

    #[test]
    fn zero_kelvin_means_unimplemented_not_absolute_zero() {
        let log = NvmeHealthLog::parse(&vec![0u8; 512]).unwrap();
        assert_eq!(log.temperature_c(), None);
        assert_eq!(log.status(), HealthStatus::Caution); // spare 0 <= threshold 0
    }

    #[test]
    fn rejects_short_buffer() {
        assert!(NvmeHealthLog::parse(&[0u8; 511]).is_none());
    }

    #[test]
    fn attribute_table_covers_the_failure_predictors() {
        // The three attributes that actually predict imminent failure.
        assert_eq!(attribute_name(0x05).unwrap().0, "Reallocated Sectors Count");
        assert_eq!(attribute_name(0xC5).unwrap().0, "Current Pending Sector Count");
        assert_eq!(attribute_name(0xC7).unwrap().0, "UltraDMA CRC Error Rate");
        assert!(attribute_name(0xE8).is_none()); // vendor-specific, needs the vendor
        assert_eq!(
            vendor_attribute_name(SsdVendor::Intel, 0xE8),
            Some("Available Reserved Space")
        );
    }

    #[test]
    fn normalized_value_at_threshold_is_failing() {
        let attr = SmartAttribute {
            id: 0x05,
            name: "Reallocated Sectors Count".into(),
            current: 36,
            worst: 36,
            threshold: 36,
            raw: 1024,
            ideal: Ideal::Low,
        };
        assert!(attr.failing());
        // A zero threshold means "no failure criterion", not "always failing".
        assert!(!SmartAttribute { threshold: 0, current: 0, ..attr.clone() }.failing());
    }
}
