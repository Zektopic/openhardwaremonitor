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
    /// ACPI table (DSDT, SSDT, …). Firmware usually publishes several SSDTs
    /// under one signature: `index` is this table's position and `of` is how
    /// many the firmware declared, so the UI can say "2 of 15" honestly even
    /// where the OS API can only hand back one of them.
    AcpiTable { signature: String, index: usize, of: usize },
    /// The raw SMBIOS/DMI table blob.
    Smbios { version: String },
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
            HexSource::AcpiTable { signature, index, of } if *of > 1 => {
                format!("ACPI {signature} ({} of {of})", index + 1)
            }
            HexSource::AcpiTable { signature, .. } => format!("ACPI {signature}"),
            HexSource::Smbios { version } => format!("SMBIOS {version}"),
        }
    }
}

/// A named byte range inside a blob — the structure the raw dump encodes.
///
/// This is what turns a wall of hex into something readable: the ACPI header's
/// signature/length/checksum fields, an SMBIOS version stamp, a PCI vendor ID.
/// The viewer tints these ranges and names them under the cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexRegion {
    pub start: usize,
    pub len: usize,
    pub label: String,
    pub kind: RegionKind,
}

/// Drives the tint a region gets, so the same colour always means the same
/// thing across blob types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionKind {
    /// Identifiers: signatures, vendor IDs, OEM strings.
    Identity,
    /// Sizes, counts, offsets.
    Length,
    /// Checksums and revisions.
    Checksum,
    /// Payload after a header.
    Payload,
}

impl HexRegion {
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.start + self.len
    }
}

/// A dump of raw bytes plus enough context to render and label it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexBlob {
    pub source: HexSource,
    /// Address shown in the offset column for byte 0.
    pub base_addr: u64,
    pub bytes: Vec<u8>,
    /// Known structure within `bytes`. May be empty for opaque dumps.
    #[serde(default)]
    pub regions: Vec<HexRegion>,
}

/// Bytes shown per row in the classic hex-dump layout.
pub const ROW_WIDTH: usize = 16;

impl HexBlob {
    pub fn new(source: HexSource, base_addr: u64, bytes: Vec<u8>) -> Self {
        Self { source, base_addr, bytes, regions: Vec::new() }
    }

    pub fn with_regions(mut self, regions: Vec<HexRegion>) -> Self {
        self.regions = regions;
        self
    }

    /// The innermost region covering `offset`, if any.
    pub fn region_at(&self, offset: usize) -> Option<&HexRegion> {
        // Smallest match wins so a field inside a larger block is reported.
        self.regions
            .iter()
            .filter(|r| r.contains(offset))
            .min_by_key(|r| r.len)
    }

    /// Offset of the next occurrence of `needle` at or after `from`.
    pub fn find(&self, needle: &[u8], from: usize) -> Option<usize> {
        if needle.is_empty() || needle.len() > self.bytes.len() {
            return None;
        }
        let last = self.bytes.len() - needle.len();
        // Starting past the last possible match means "no match from here" —
        // clamping `from` down to `last` instead would silently search
        // *backwards* and re-report a hit the caller has already passed.
        if from > last {
            return None;
        }
        (from..=last).find(|&i| &self.bytes[i..i + needle.len()] == needle)
    }

    /// Wrapping search, so repeated "find next" cycles through the blob.
    pub fn find_wrapping(&self, needle: &[u8], from: usize) -> Option<usize> {
        self.find(needle, from).or_else(|| self.find(needle, 0))
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

/// Byte order used when decoding multi-byte values at the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    pub fn label(self) -> &'static str {
        match self {
            Endian::Little => "LE",
            Endian::Big => "BE",
        }
    }
}

/// Decode the bytes at `offset` as each of the common scalar types.
///
/// This is the "what does this actually mean" half of a hex viewer: a length
/// field, a version stamp or a float is unreadable as hex pairs but obvious the
/// moment it is decoded. Entries whose type does not fit before the end of the
/// blob are omitted rather than shown as zero.
pub fn inspect(bytes: &[u8], offset: usize, endian: Endian) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let Some(&b) = bytes.get(offset) else { return out };
    let rest = &bytes[offset..];

    // Reading N bytes at the cursor in the selected order.
    let take = |n: usize| -> Option<u64> {
        let slice = rest.get(..n)?;
        Some(match endian {
            Endian::Little => slice.iter().rev().fold(0u64, |a, &x| (a << 8) | x as u64),
            Endian::Big => slice.iter().fold(0u64, |a, &x| (a << 8) | x as u64),
        })
    };

    out.push(("int8", (b as i8).to_string()));
    out.push(("uint8", b.to_string()));
    if let Some(v) = take(2) {
        out.push(("int16", (v as u16 as i16).to_string()));
        out.push(("uint16", (v as u16).to_string()));
    }
    if let Some(v) = take(4) {
        out.push(("int32", (v as u32 as i32).to_string()));
        out.push(("uint32", (v as u32).to_string()));
        out.push(("float32", format_float(f32::from_bits(v as u32) as f64)));
    }
    if let Some(v) = take(8) {
        out.push(("int64", (v as i64).to_string()));
        out.push(("uint64", v.to_string()));
        out.push(("float64", format_float(f64::from_bits(v))));
    }

    out.push(("binary", format!("{b:08b}")));
    out.push((
        "char",
        if (0x20..0x7f).contains(&b) { (b as char).to_string() } else { "—".into() },
    ));
    // Firmware blobs are full of fixed-length ASCII tags (ACPI signatures, OEM
    // IDs), so a short string preview earns its place.
    let ascii: String = rest
        .iter()
        .take(8)
        .map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '.' })
        .collect();
    out.push(("ascii[8]", ascii));
    out
}

/// Floats here are usually garbage (we are decoding arbitrary bytes), so keep
/// the rendering short and make non-finite values obvious rather than noisy.
fn format_float(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let a = v.abs();
    if a != 0.0 && !(1e-4..1e9).contains(&a) {
        format!("{v:.4e}")
    } else {
        format!("{v:.6}")
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
        // Firmware publishes several SSDTs; the index keeps them distinguishable.
        assert_eq!(
            HexSource::AcpiTable { signature: "DSDT".into(), index: 0, of: 1 }.label(),
            "ACPI DSDT"
        );
        assert_eq!(
            HexSource::AcpiTable { signature: "SSDT".into(), index: 2, of: 15 }.label(),
            "ACPI SSDT (3 of 15)"
        );
    }

    #[test]
    fn inspector_decodes_both_byte_orders() {
        // 0x01 0x02 0x03 0x04 → 0x04030201 LE, 0x01020304 BE.
        let bytes = [0x01u8, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let le: Vec<_> = inspect(&bytes, 0, Endian::Little);
        let get = |v: &Vec<(&str, String)>, k: &str| {
            v.iter().find(|(n, _)| *n == k).map(|(_, s)| s.clone()).unwrap()
        };
        assert_eq!(get(&le, "uint32"), 0x04030201u32.to_string());
        assert_eq!(get(&le, "uint16"), 0x0201u16.to_string());

        let be = inspect(&bytes, 0, Endian::Big);
        assert_eq!(get(&be, "uint32"), 0x01020304u32.to_string());
        assert_eq!(get(&be, "uint8"), "1", "single bytes are order-independent");
    }

    #[test]
    fn inspector_omits_types_that_run_past_the_end() {
        let bytes = [0xFFu8, 0x00, 0x01]; // only 3 bytes left
        let v = inspect(&bytes, 0, Endian::Little);
        let names: Vec<_> = v.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"uint16"));
        // A uint32 would read past the buffer — omitted, not silently zero-padded.
        assert!(!names.contains(&"uint32"));
        assert!(!names.contains(&"uint64"));
        // int8 of 0xFF is -1, which is the point of showing both signednesses.
        assert_eq!(v.iter().find(|(n, _)| *n == "int8").unwrap().1, "-1");

        // Past the end entirely yields nothing rather than panicking.
        assert!(inspect(&bytes, 3, Endian::Little).is_empty());
        assert!(inspect(&[], 0, Endian::Little).is_empty());
    }

    #[test]
    fn search_finds_and_wraps() {
        let b = HexBlob::new(
            HexSource::EcRam { offset: 0 },
            0,
            vec![0x00, 0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xDE, 0xAD],
        );
        assert_eq!(b.find(&[0xDE, 0xAD], 0), Some(1));
        assert_eq!(b.find(&[0xDE, 0xAD], 2), Some(6));
        assert_eq!(b.find(&[0xDE, 0xAD], 7), None);
        // "Find next" past the last hit cycles back to the first.
        assert_eq!(b.find_wrapping(&[0xDE, 0xAD], 7), Some(1));
        assert_eq!(b.find(&[], 0), None);
        assert_eq!(b.find(&[0x11], 0), None);
        // A needle longer than the blob can't match and must not panic.
        assert_eq!(b.find(&[0u8; 99], 0), None);
    }

    #[test]
    fn innermost_region_wins_at_an_offset() {
        let b = HexBlob::new(HexSource::EcRam { offset: 0 }, 0, vec![0; 64]).with_regions(vec![
            HexRegion { start: 0, len: 36, label: "Header".into(), kind: RegionKind::Payload },
            HexRegion { start: 4, len: 4, label: "Length".into(), kind: RegionKind::Length },
        ]);
        // Offset 5 sits in both; the narrower field is the useful answer.
        assert_eq!(b.region_at(5).unwrap().label, "Length");
        assert_eq!(b.region_at(0).unwrap().label, "Header");
        assert!(b.region_at(40).is_none());
    }
}
