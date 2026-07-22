//! CSV logging — HWiNFO's "Start Logging". One column per sensor, one row per
//! poll tick. Owned and written by the poll thread so UI never blocks on IO.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use crate::model::Hardware;

pub struct CsvLogger {
    writer: BufWriter<File>,
    /// Sensor identifiers in column order (fixed at start; new sensors ignored).
    columns: Vec<String>,
    path: PathBuf,
    rows: u64,
}

impl CsvLogger {
    /// Create a logger, writing the header from the current tree.
    pub fn start(tree: &[Hardware]) -> Result<Self, String> {
        let dir = dirs::document_dir()
            .or_else(dirs::desktop_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = dir.join(format!("SensorView_log_{stamp}.csv"));
        let file = File::create(&path).map_err(|e| e.to_string())?;
        let mut writer = BufWriter::new(file);
        // UTF-8 BOM so Excel renders °C / µ correctly.
        writer.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| e.to_string())?;

        let mut columns = Vec::new();
        let mut header = String::from("Time");
        collect(tree, &mut |s| {
            columns.push(s.identifier.clone());
            let unit = s.sensor_type.unit();
            let label = if unit.is_empty() {
                s.name.clone()
            } else {
                format!("{} [{}]", s.name, unit)
            };
            header.push(',');
            header.push_str(&csv_escape(&label));
        });
        writeln!(writer, "{header}").map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        Ok(Self { writer, columns, path, rows: 0 })
    }

    /// Append one row for the given tree snapshot.
    pub fn log(&mut self, tree: &[Hardware]) {
        // Index current values by identifier.
        let mut values = std::collections::HashMap::new();
        collect(tree, &mut |s| {
            values.insert(s.identifier.clone(), s.value);
        });

        let secs = self.rows; // relative seconds at 1 Hz; good enough for a log
        let mut line = format!("{secs}");
        for id in &self.columns {
            line.push(',');
            if let Some(Some(v)) = values.get(id) {
                line.push_str(&format!("{v:.3}"));
            }
        }
        if writeln!(self.writer, "{line}").is_ok() {
            let _ = self.writer.flush();
            self.rows += 1;
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn rows(&self) -> u64 {
        self.rows
    }
}

/// Visit every sensor in the tree in stable (depth-first) order.
fn collect(tree: &[Hardware], f: &mut impl FnMut(&crate::model::Sensor)) {
    for hw in tree {
        for s in &hw.sensors {
            f(s);
        }
        collect(&hw.sub_hardware, f);
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Hardware, HardwareType, Sensor, SensorType};

    fn tree(v: f32) -> Vec<Hardware> {
        vec![Hardware {
            identifier: "/cpu/0".into(),
            name: "CPU".into(),
            hardware_type: HardwareType::Cpu,
            sensors: vec![Sensor {
                identifier: "/cpu/0/temperature/0".into(),
                name: "Core, Max".into(), // comma → must be quoted in header
                sensor_type: SensorType::Temperature,
                index: 0,
                value: Some(v),
                min: None, max: None, avg: None,
            }],
            sub_hardware: vec![],
        }]
    }

    #[test]
    fn writes_header_and_rows() {
        let mut logger = CsvLogger::start(&tree(40.0)).expect("start logger");
        logger.log(&tree(41.0));
        logger.log(&tree(42.5));
        let path = logger.path().clone();
        assert_eq!(logger.rows(), 2);
        drop(logger); // flush + close

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
        assert!(lines[0].contains("\"Core, Max [°C]\""));
        assert!(lines[1].ends_with("41.000"));
        assert!(lines[2].ends_with("42.500"));
        let _ = std::fs::remove_file(&path);
    }
}
