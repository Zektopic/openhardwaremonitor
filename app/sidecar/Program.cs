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

// Flush the tree on Ctrl+C / kill so the driver handle is released cleanly.
AppDomain.CurrentDomain.ProcessExit += (_, _) => computer.Close();
Console.CancelKeyPress += (_, _) => { computer.Close(); Environment.Exit(0); };

var visitor = new UpdateVisitor();
var json = new JsonSerializerOptions { WriteIndented = false };
var stdout = Console.Out;

while (true)
{
    computer.Accept(visitor);
    var tree = computer.Hardware.Select(MapHardware).ToList();
    stdout.WriteLine(JsonSerializer.Serialize(tree, json));
    stdout.Flush();
    Thread.Sleep(1000);
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
