// SensorView bridge sidecar.
//
// Thin console wrapper around LibreHardwareMonitorLib: polls all hardware once
// per second and writes one JSON snapshot per line to stdout, in exactly the
// shape of SensorView's Rust model (src/model.rs). The Rust app spawns this
// process and reads the stream; it never links .NET itself.
//
// The update loop mirrors OpenHardwareMonitor's GUI/UpdateVisitor.cs pattern.
// Full sensor coverage (Super-I/O, MSR, SMBus) requires administrator rights;
// without them LibreHardwareMonitor silently exposes the subset it can reach.

using System.Security.Principal;
using System.Text.Json;
using LibreHardwareMonitor.Hardware;

var computer = new Computer
{
    IsCpuEnabled = true,
    IsGpuEnabled = true,
    IsMemoryEnabled = true,
    IsMotherboardEnabled = true,
    IsControllerEnabled = true,
    IsNetworkEnabled = true,
    IsStorageEnabled = true,
    IsPsuEnabled = true,
    IsBatteryEnabled = true,
};

computer.Open();

// First line: diagnostics meta so the Rust app can explain zero sensors
// (driver blocked / not elevated). ring0_report is the ring0 slice of LHM's
// own report, which names the exact WinRing0 open/install failure.
var isElevated = false;
try
{
    using var identity = WindowsIdentity.GetCurrent();
    isElevated = new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
}
catch { /* ignore */ }

// Flush the tree on Ctrl+C / kill so the driver handle is released cleanly.
AppDomain.CurrentDomain.ProcessExit += (_, _) => computer.Close();
Console.CancelKeyPress += (_, _) => { computer.Close(); Environment.Exit(0); };

var json = new JsonSerializerOptions { WriteIndented = false };

// Use the raw stdout stream so a broken pipe (parent gone) surfaces as an
// IOException we can act on, instead of being swallowed by Console.Out.
using var stdout = Console.OpenStandardOutput();
using var writer = new StreamWriter(stdout) { AutoFlush = false };

// First line: diagnostics meta so the Rust app can explain zero sensors
// (driver blocked / not elevated). ring0_report is the ring0 slice of LHM's
// own report, which names the exact WinRing0 open/install failure.
var ring0Report = ExtractRing0(computer.GetReport());
var lhmVersion = typeof(Computer).Assembly.GetName().Version?.ToString() ?? "?";
writer.WriteLine(JsonSerializer.Serialize(new Dictionary<string, object?>
{
    ["meta"] = new Dictionary<string, object?>
    {
        ["lhm_version"] = lhmVersion,
        ["is_elevated"] = isElevated,
        ["ring0_report"] = ring0Report,
    },
}));
writer.Flush();

var visitor = new UpdateVisitor();

// Watch the parent (Rust app). If it dies, exit promptly so we never orphan —
// an elevated orphan would leak CPU and hold the driver handle.
var parentId = Environment.GetEnvironmentVariable("SENSORVIEW_PARENT_PID");
System.Diagnostics.Process? parent = null;
if (int.TryParse(parentId, out var pid))
{
    try { parent = System.Diagnostics.Process.GetProcessById(pid); } catch { }
}

while (true)
{
    if (parent is { HasExited: true })
    {
        break;
    }
    computer.Accept(visitor);
    var tree = computer.Hardware.Select(MapHardware).ToList();
    try
    {
        writer.WriteLine(JsonSerializer.Serialize(tree, json));
        writer.Flush(); // throws if the parent closed the read end of the pipe
    }
    catch (IOException)
    {
        break; // parent gone → exit
    }
    Thread.Sleep(1000);
}

computer.Close();

// Pull the "Ring0" section out of LHM's full text report — it records whether
// the kernel driver opened, and any install/blocklist error.
static string ExtractRing0(string report)
{
    var lines = report.Replace("\r\n", "\n").Split('\n');
    var kept = new List<string>();
    var capturing = false;
    foreach (var line in lines)
    {
        if (line.StartsWith("Ring0", StringComparison.OrdinalIgnoreCase)
            || line.Contains("WinRing0")
            || line.Contains("Kernel Driver"))
        {
            capturing = true;
        }
        else if (capturing && line.Length > 0 && !char.IsWhiteSpace(line[0]) && line.Contains("Report"))
        {
            capturing = false;
        }
        if (capturing)
        {
            kept.Add(line);
        }
    }
    var text = string.Join("\n", kept).Trim();
    return text.Length == 0 ? "(no ring0 section in report)" : text;
}

static Dictionary<string, object?> MapHardware(IHardware hw)
{
    return new Dictionary<string, object?>
    {
        ["identifier"] = hw.Identifier.ToString(),
        ["name"] = hw.Name,
        ["type"] = MapHardwareType(hw.HardwareType),
        ["sensors"] = hw.Sensors
            .OrderBy(s => s.SensorType).ThenBy(s => s.Index)
            .Select(MapSensor).ToList(),
        ["sub_hardware"] = hw.SubHardware.Select(MapHardware).ToList(),
    };
}

static Dictionary<string, object?> MapSensor(ISensor s)
{
    return new Dictionary<string, object?>
    {
        ["identifier"] = s.Identifier.ToString(),
        ["name"] = s.Name,
        ["type"] = s.SensorType.ToString(), // LHM names match the Rust enum 1:1
        ["index"] = s.Index,
        ["value"] = s.Value,
        ["min"] = (float?)null, // running stats are tracked on the Rust side
        ["max"] = (float?)null,
        ["avg"] = (float?)null,
    };
}

// Rust model.rs HardwareType variant names (LHM names differ slightly).
static string MapHardwareType(HardwareType t) => t switch
{
    HardwareType.Motherboard => "Mainboard",
    HardwareType.SuperIO => "SuperIO",
    HardwareType.Cpu => "Cpu",
    HardwareType.Memory => "Ram",
    HardwareType.GpuNvidia => "GpuNvidia",
    HardwareType.GpuAmd => "GpuAti",
    HardwareType.GpuIntel => "GpuIntel",
    HardwareType.Storage => "Storage",
    HardwareType.Network => "Network",
    HardwareType.Cooler => "Cooler",
    HardwareType.EmbeddedController => "EmbeddedController",
    HardwareType.Psu => "Psu",
    HardwareType.Battery => "Battery",
    _ => "Mainboard",
};

/// <summary>Mirrors OpenHardwareMonitor's GUI/UpdateVisitor.cs.</summary>
class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer) => computer.Traverse(this);

    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (IHardware sub in hardware.SubHardware)
            sub.Accept(this);
    }

    public void VisitSensor(ISensor sensor) { }

    public void VisitParameter(IParameter parameter) { }
}
