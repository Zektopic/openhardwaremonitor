//! Raw memory/register blobs for the Hex Viewer pane.
//!
//! Several of the most useful diagnostics are byte dumps rather than scalar
//! sensors: SPD EEPROM off the SMBus, PCI configuration space, an NVMe log
//! page, the SMC key table on macOS, or Embedded Controller RAM. They all
//! reduce to "a base address and some bytes", so one type serves every source.
//!
//! Blobs come from the **slow lane** — SPD and EC reads go over I²C/SMBus, and
//! hammering that bus is what triggers the SMI storms that cause audio dropouts
//! and micro-stutter.

// The hex-dump surface is carried in full so any byte source (SPD, PCI config,
// NVMe log page, SMC key) can be added without extending the model.
// Exercised by the tests below until the M2 Hex Viewer pane lands.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Where a blob was read from. Carries the addressing context needed to label
/// the dump meaningfully in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HexSource {
    /// SPD EEPROM for one DIMM slot, read over SMBus (typically 256 or 512 B).
    SpdEeprom { slot: String, smbus_addr: u8 },
    /// PCI configuration space (256 B legacy, 4096 B extended).
    PciConfig { bdf: String },
    /// An NVMe log page, e.g. 0x02 = SMART / Health Information.
    NvmeLogPage { device: String, page_id: u8 },
    /// A macOS SMC key's raw value bytes.
    SmcKey { key: String, type_code: String },
    /// Embedded Controller RAM window.
    EcRam { offset: u16 },
    /// ACPI table (DSDT, SSDT, …).
    AcpiTable { signature: String },
}

impl HexSource {
    /// Short label for the pane's title bar / tab.
    pub fn label(&self) -> String {
        match self {
            HexSource::SpdEeprom { slot, .. } => format!("SPD — {slot}"),
            HexSource::PciConfig { bdf } => format!("PCI cfg — {bdf}"),
            HexSource::NvmeLogPage { device, page_id } => {
                format!("NVMe {device} log 0x{page_id:02X}")
            }
            HexSource::SmcKey { key, .. } => format!("SMC {key}"),
            HexSource::EcRam { offset } => format!("EC RAM +0x{offset:04X}"),
            HexSource::AcpiTable { signature } => format!("ACPI {signature}"),
        }
    }
}

/// A dump of raw bytes plus enough context to render and label it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexBlob {
    pub source: HexSource,
    /// Address shown in the offset column for byte 0.
    pub base_addr: u64,
    pub bytes: Vec<u8>,
}

/// Bytes shown per row in the classic hex-dump layout.
pub const ROW_WIDTH: usize = 16;

impl HexBlob {
    pub fn new(source: HexSource, base_addr: u64, bytes: Vec<u8>) -> Self {
        Self { source, base_addr, bytes }
    }

    /// Number of rows the dump occupies. An empty blob has no rows.
    pub fn rows(&self) -> usize {
        self.bytes.len().div_ceil(ROW_WIDTH)
    }

    /// Render one row as `offset  hex bytes  |ascii|`.
    ///
    /// The UI draws rows individually (virtualized through
    /// `egui_extras::TableBuilder`) so a 4 KiB extended config space scrolls
    /// without formatting the whole dump every frame. Short final rows are
    /// padded so the ASCII gutter stays aligned.
    pub fn format_row(&self, row: usize) -> Option<String> {
        let start = row * ROW_WIDTH;
        if start >= self.bytes.len() {
            return None;
        }
        let end = (start + ROW_WIDTH).min(self.bytes.len());
        let chunk = &self.bytes[start..end];

        let mut hex = String::with_capacity(ROW_WIDTH * 3);
        for (i, b) in chunk.iter().enumerate() {
            if i > 0 {
                hex.push(' ');
            }
            // Split the row in half, as hex editors conventionally do.
            if i == ROW_WIDTH / 2 {
                hex.push(' ');
            }
            hex.push_str(&format!("{b:02X}"));
        }
        // Pad a short final row: 3 chars per missing byte, plus the mid gap if
        // the row ends before it.
        let missing = ROW_WIDTH - chunk.len();
        if missing > 0 {
            hex.push_str(&" ".repeat(missing * 3));
            if chunk.len() <= ROW_WIDTH / 2 {
                hex.push(' ');
            }
        }

        let ascii: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();

        Some(format!(
            "{:08X}  {}  |{}|",
            self.base_addr + start as u64,
            hex,
            ascii
        ))
    }

    /// The whole dump as text — used by the report exporter and `/api/hex`.
    pub fn to_text(&self) -> String {
        (0..self.rows())
            .filter_map(|r| self.format_row(r))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(len: usize) -> HexBlob {
        HexBlob::new(
            HexSource::PciConfig { bdf: "0000:00:1f.0".into() },
            0,
            (0..len).map(|i| i as u8).collect(),
        )
    }

    #[test]
    fn empty_blob_has_no_rows() {
        let b = blob(0);
        assert_eq!(b.rows(), 0);
        assert_eq!(b.format_row(0), None);
        assert_eq!(b.to_text(), "");
    }

    #[test]
    fn row_count_rounds_up() {
        assert_eq!(blob(1).rows(), 1);
        assert_eq!(blob(16).rows(), 1);
        assert_eq!(blob(17).rows(), 2);
        assert_eq!(blob(512).rows(), 32);
        assert_eq!(blob(4096).rows(), 256);
    }

    #[test]
    fn full_row_layout_matches_classic_hex_dump() {
        let b = HexBlob::new(
            HexSource::EcRam { offset: 0 },
            0,
            b"Hello, hex dump!".to_vec(),
        );
        assert_eq!(
            b.format_row(0).unwrap(),
            "00000000  48 65 6C 6C 6F 2C 20 68  65 78 20 64 75 6D 70 21  |Hello, hex dump!|"
        );
    }

    #[test]
    fn short_final_row_keeps_ascii_gutter_aligned() {
        let full = blob(16).format_row(0).unwrap();
        let short = blob(19).format_row(1).unwrap();
        // The '|' opening the ASCII gutter must land in the same column.
        assert_eq!(full.find('|'), short.find('|'), "\nfull:  {full}\nshort: {short}");
    }

    #[test]
    fn one_byte_row_also_stays_aligned() {
        let full = blob(16).format_row(0).unwrap();
        let one = blob(1).format_row(0).unwrap();
        assert_eq!(full.find('|'), one.find('|'), "\nfull: {full}\none:  {one}");
        assert!(one.ends_with("|.|"), "NUL renders as '.': {one}");
    }

    #[test]
    fn offset_column_starts_at_base_address() {
        let b = HexBlob::new(
            HexSource::SpdEeprom { slot: "DIMM_A1".into(), smbus_addr: 0x50 },
            0x1000,
            vec![0xFF; 32],
        );
        assert!(b.format_row(0).unwrap().starts_with("00001000"));
        assert!(b.format_row(1).unwrap().starts_with("00001010"));
    }

    #[test]
    fn non_printable_bytes_render_as_dots() {
        let b = HexBlob::new(HexSource::EcRam { offset: 0 }, 0, vec![0x00, 0x7F, 0x80, 0xFF]);
        assert!(b.format_row(0).unwrap().ends_with("|....|"));
    }

    #[test]
    fn source_labels_are_human_readable() {
        assert_eq!(
            HexSource::NvmeLogPage { device: "nvme0".into(), page_id: 0x02 }.label(),
            "NVMe nvme0 log 0x02"
        );
    }
}
