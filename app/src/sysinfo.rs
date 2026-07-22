//! Static system information for the Main window tree and the System Summary.
//!
//! Queried once at startup on a background thread (WMI/COM on Windows; minimal
//! fallbacks elsewhere). Anything a source can't provide stays `None` and the
//! UI renders "—" — honest placeholders until the native engine (SMBus SPD,
//! CPUID, NVML/ADL) fills them in.

use std::sync::{Arc, RwLock};

#[derive(Debug, Default, Clone)]
pub struct CpuInfo {
    pub name: String,
    pub cores: Option<u32>,
    pub threads: Option<u32>,
    pub base_clock_mhz: Option<u32>,
    pub max_clock_mhz: Option<u32>,
    pub l2_kb: Option<u32>,
    pub l3_kb: Option<u32>,
    pub socket: Option<String>,
    /// ISA feature names detected at runtime (for the Summary features grid).
    pub features: Vec<(&'static str, bool)>,
}

#[derive(Debug, Default, Clone)]
pub struct BoardInfo {
    pub product: String,
    pub manufacturer: String,
    pub bios_version: String,
    pub bios_date: String,
}

#[derive(Debug, Default, Clone)]
pub struct MemoryModule {
    pub bank: String,
    pub manufacturer: String,
    pub part_number: String,
    pub capacity_gb: f64,
    pub speed_mts: Option<u32>,
    pub configured_speed_mts: Option<u32>,
    pub voltage_mv: Option<u32>,
    pub memory_type: String,
}

#[derive(Debug, Default, Clone)]
pub struct GpuInfo {
    pub name: String,
    /// WMI AdapterRAM (u32, capped at 4 GB) — kept for the native engine to
    /// replace with NVML/ADL truth; not displayed while unreliable.
    #[allow(dead_code)]
    pub vram_gb: Option<f64>,
    pub driver_version: String,
}

#[derive(Debug, Default, Clone)]
pub struct DriveInfo {
    pub model: String,
    pub interface: String,
    pub size_gb: Option<f64>,
}

#[derive(Debug, Default, Clone)]
pub struct OsInfo {
    pub caption: String,
    pub build: String,
    pub arch: String,
    pub uefi_boot: Option<bool>,
    pub secure_boot: Option<bool>,
}

#[derive(Debug, Default, Clone)]
pub struct SystemInfo {
    pub computer_name: String,
    pub user_name: String,
    pub cpu: CpuInfo,
    pub board: BoardInfo,
    pub memory_modules: Vec<MemoryModule>,
    pub total_memory_gb: Option<f64>,
    pub gpus: Vec<GpuInfo>,
    pub drives: Vec<DriveInfo>,
    pub os: OsInfo,
}

/// Shared handle: `None` until the background query completes.
pub type SystemInfoHandle = Arc<RwLock<Option<SystemInfo>>>;

/// Kick off the (slow) WMI enumeration without blocking the UI.
pub fn spawn_query() -> SystemInfoHandle {
    let handle: SystemInfoHandle = Arc::new(RwLock::new(None));
    let sink = handle.clone();
    std::thread::spawn(move || {
        let info = query();
        if let Ok(mut slot) = sink.write() {
            *slot = Some(info);
        }
    });
    handle
}

fn cpu_features() -> Vec<(&'static str, bool)> {
    #[cfg(target_arch = "x86_64")]
    {
        vec![
            ("MMX", is_x86_feature_detected!("mmx")),
            ("SSE", is_x86_feature_detected!("sse")),
            ("SSE2", is_x86_feature_detected!("sse2")),
            ("SSE3", is_x86_feature_detected!("sse3")),
            ("SSSE3", is_x86_feature_detected!("ssse3")),
            ("SSE4.1", is_x86_feature_detected!("sse4.1")),
            ("SSE4.2", is_x86_feature_detected!("sse4.2")),
            ("SSE4A", is_x86_feature_detected!("sse4a")),
            ("AVX", is_x86_feature_detected!("avx")),
            ("AVX2", is_x86_feature_detected!("avx2")),
            ("AVX-512F", is_x86_feature_detected!("avx512f")),
            ("FMA", is_x86_feature_detected!("fma")),
            ("BMI1", is_x86_feature_detected!("bmi1")),
            ("BMI2", is_x86_feature_detected!("bmi2")),
            ("AES-NI", is_x86_feature_detected!("aes")),
            ("SHA", is_x86_feature_detected!("sha")),
            ("RDRAND", is_x86_feature_detected!("rdrand")),
            ("RDSEED", is_x86_feature_detected!("rdseed")),
            ("POPCNT", is_x86_feature_detected!("popcnt")),
            ("F16C", is_x86_feature_detected!("f16c")),
        ]
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn query() -> SystemInfo {
    use std::collections::HashMap;
    use wmi::{Variant, WMIConnection};

    let mut info = SystemInfo {
        computer_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
        user_name: std::env::var("USERNAME").unwrap_or_default(),
        ..Default::default()
    };
    info.cpu.features = cpu_features();

    let Ok(wmi) = WMIConnection::new() else { return info };

    type Row = HashMap<String, Variant>;

    let s = |v: Option<&Variant>| -> String {
        match v {
            Some(Variant::String(x)) => x.trim().to_string(),
            _ => String::new(),
        }
    };
    let u = |v: Option<&Variant>| -> Option<u32> {
        match v {
            Some(Variant::UI4(x)) => Some(*x),
            Some(Variant::I4(x)) => u32::try_from(*x).ok(),
            Some(Variant::UI2(x)) => Some(*x as u32),
            Some(Variant::String(x)) => x.parse().ok(),
            _ => None,
        }
    };
    let u64v = |v: Option<&Variant>| -> Option<u64> {
        match v {
            Some(Variant::UI8(x)) => Some(*x),
            Some(Variant::I8(x)) => u64::try_from(*x).ok(),
            Some(Variant::UI4(x)) => Some(*x as u64),
            Some(Variant::String(x)) => x.parse().ok(),
            _ => None,
        }
    };

    if let Ok(rows) = wmi.raw_query::<Row>(
        "SELECT Name, NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed, L2CacheSize, L3CacheSize, SocketDesignation FROM Win32_Processor",
    ) {
        if let Some(r) = rows.first() {
            info.cpu.name = s(r.get("Name"));
            info.cpu.cores = u(r.get("NumberOfCores"));
            info.cpu.threads = u(r.get("NumberOfLogicalProcessors"));
            info.cpu.max_clock_mhz = u(r.get("MaxClockSpeed"));
            info.cpu.base_clock_mhz = u(r.get("MaxClockSpeed"));
            info.cpu.l2_kb = u(r.get("L2CacheSize"));
            info.cpu.l3_kb = u(r.get("L3CacheSize"));
            info.cpu.socket = Some(s(r.get("SocketDesignation"))).filter(|x| !x.is_empty());
        }
    }

    if let Ok(rows) = wmi.raw_query::<Row>("SELECT Product, Manufacturer FROM Win32_BaseBoard") {
        if let Some(r) = rows.first() {
            info.board.product = s(r.get("Product"));
            info.board.manufacturer = s(r.get("Manufacturer"));
        }
    }
    if let Ok(rows) = wmi.raw_query::<Row>("SELECT SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS") {
        if let Some(r) = rows.first() {
            info.board.bios_version = s(r.get("SMBIOSBIOSVersion"));
            let date = s(r.get("ReleaseDate"));
            // WMI CIM_DATETIME: yyyymmddHHMMSS… → mm/dd/yyyy like HWiNFO shows.
            if date.len() >= 8 {
                info.board.bios_date = format!("{}/{}/{}", &date[4..6], &date[6..8], &date[0..4]);
            }
        }
    }

    if let Ok(rows) = wmi.raw_query::<Row>(
        "SELECT BankLabel, DeviceLocator, Manufacturer, PartNumber, Capacity, Speed, ConfiguredClockSpeed, ConfiguredVoltage, SMBIOSMemoryType FROM Win32_PhysicalMemory",
    ) {
        let mut total = 0.0;
        for r in &rows {
            let capacity_gb = u64v(r.get("Capacity")).map(|b| b as f64 / (1u64 << 30) as f64).unwrap_or(0.0);
            total += capacity_gb;
            let mem_type = match u(r.get("SMBIOSMemoryType")) {
                Some(26) => "DDR4",
                Some(34) => "DDR5",
                Some(24) => "DDR3",
                _ => "DRAM",
            };
            info.memory_modules.push(MemoryModule {
                bank: {
                    let bank = s(r.get("BankLabel"));
                    let loc = s(r.get("DeviceLocator"));
                    if bank.is_empty() { loc } else { format!("{bank}/{loc}") }
                },
                manufacturer: s(r.get("Manufacturer")),
                part_number: s(r.get("PartNumber")),
                capacity_gb,
                speed_mts: u(r.get("Speed")),
                configured_speed_mts: u(r.get("ConfiguredClockSpeed")),
                voltage_mv: u(r.get("ConfiguredVoltage")),
                memory_type: mem_type.to_string(),
            });
        }
        if total > 0.0 {
            info.total_memory_gb = Some(total);
        }
    }

    if let Ok(rows) = wmi.raw_query::<Row>("SELECT Name, AdapterRAM, DriverVersion FROM Win32_VideoController") {
        for r in &rows {
            info.gpus.push(GpuInfo {
                name: s(r.get("Name")),
                vram_gb: u64v(r.get("AdapterRAM")).map(|b| b as f64 / (1u64 << 30) as f64),
                driver_version: s(r.get("DriverVersion")),
            });
        }
    }

    if let Ok(rows) = wmi.raw_query::<Row>("SELECT Model, InterfaceType, Size FROM Win32_DiskDrive") {
        for r in &rows {
            info.drives.push(DriveInfo {
                model: s(r.get("Model")),
                interface: s(r.get("InterfaceType")),
                size_gb: u64v(r.get("Size")).map(|b| b as f64 / 1_000_000_000.0),
            });
        }
    }

    if let Ok(rows) = wmi.raw_query::<Row>("SELECT Caption, BuildNumber, OSArchitecture FROM Win32_OperatingSystem") {
        if let Some(r) = rows.first() {
            info.os.caption = s(r.get("Caption"));
            info.os.build = s(r.get("BuildNumber"));
            info.os.arch = s(r.get("OSArchitecture"));
        }
    }

    // Secure Boot / UEFI: registry flag (no clean WMI class for it).
    info.os.secure_boot = read_secure_boot();
    info.os.uefi_boot = info.os.secure_boot.map(|_| true);

    info
}

#[cfg(windows)]
fn read_secure_boot() -> Option<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State",
            "/v",
            "UEFISecureBootEnabled",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("0x1") {
        Some(true)
    } else if text.contains("0x0") {
        Some(false)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn query() -> SystemInfo {
    SystemInfo {
        computer_name: std::env::var("HOSTNAME").unwrap_or_default(),
        user_name: std::env::var("USER").unwrap_or_default(),
        cpu: CpuInfo { features: cpu_features(), ..Default::default() },
        ..Default::default()
    }
}
