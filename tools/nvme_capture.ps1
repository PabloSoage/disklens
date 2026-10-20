# Capture NVMe Identify Controller and the SMART/Health log (page 0x02) through
# IOCTL_STORAGE_QUERY_PROPERTY (StorageAdapterProtocolSpecificProperty), the same path as
# CrystalDiskInfo's DoIdentifyDeviceNVMeStorageQuery. Writes one JSON per disk in disklens'
# dump format. Usage: pwsh -File nvme_capture.ps1 <outdir>
param([string]$OutDir = ".")

$src = @"
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class Nv {
    [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern SafeFileHandle CreateFile(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool DeviceIoControl(SafeFileHandle h, uint code, byte[] inb, int inl, byte[] outb, int outl, out int ret, IntPtr ov);
    const uint IOCTL_STORAGE_QUERY_PROPERTY = 0x2D1400;
    // STORAGE_PROPERTY_QUERY (8 bytes) + STORAGE_PROTOCOL_SPECIFIC_DATA (40 bytes) + data.
    public static byte[] Query(int drive, int dataType, uint requestValue, uint subValue, int len, out int err) {
        err = 0;
        var h = CreateFile("\\\\.\\PhysicalDrive" + drive, 0, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
        if (h.IsInvalid) { err = Marshal.GetLastWin32Error(); return null; }
        int hdr = 8 + 40;
        var buf = new byte[hdr + len];
        BitConverter.GetBytes(49).CopyTo(buf, 0);   // StorageAdapterProtocolSpecificProperty
        BitConverter.GetBytes(0).CopyTo(buf, 4);    // PropertyStandardQuery
        BitConverter.GetBytes(3).CopyTo(buf, 8);    // ProtocolTypeNvme
        BitConverter.GetBytes(dataType).CopyTo(buf, 12); // 1 identify, 2 log page
        BitConverter.GetBytes(requestValue).CopyTo(buf, 16);
        BitConverter.GetBytes(subValue).CopyTo(buf, 20);
        BitConverter.GetBytes(40).CopyTo(buf, 24);  // ProtocolDataOffset
        BitConverter.GetBytes(len).CopyTo(buf, 28); // ProtocolDataLength
        int ret;
        bool ok = DeviceIoControl(h, IOCTL_STORAGE_QUERY_PROPERTY, buf, buf.Length, buf, buf.Length, out ret, IntPtr.Zero);
        if (!ok) { err = Marshal.GetLastWin32Error(); h.Dispose(); return null; }
        h.Dispose();
        var outb = new byte[len];
        Array.Copy(buf, hdr, outb, 0, len);
        return outb;
    }
}
"@
Add-Type -TypeDefinition $src
New-Item -ItemType Directory -Force $OutDir | Out-Null
function Hex($b) { if ($null -eq $b) { return $null }; ($b | ForEach-Object { $_.ToString("x2") }) -join "" }

foreach ($d in Get-PhysicalDisk | Where-Object BusType -eq "NVMe") {
    $n = [int]$d.DeviceId
    $e1 = 0; $e2 = 0; $e3 = 0
    $ctrl = [Nv]::Query($n, 1, 1, 0, 4096, [ref]$e1)   # Identify, CNS 1 = controller
    $ns   = [Nv]::Query($n, 1, 0, 1, 4096, [ref]$e2)   # Identify, CNS 0 = namespace 1
    $log  = [Nv]::Query($n, 2, 2, 0, 512, [ref]$e3)    # Get Log Page 0x02
    $errors = @()
    if ($null -eq $ctrl) { $errors += "identify controller: win32 error $e1" }
    if ($null -eq $ns) { $errors += "identify namespace: win32 error $e2" }
    if ($null -eq $log) { $errors += "smart log: win32 error $e3" }
    $dump = [ordered]@{
        format = 1
        tool = "nvme_capture.ps1 (Windows IOCTL_STORAGE_QUERY_PROPERTY)"
        captured_at = (Get-Date).ToUniversalTime().ToString("o")
        device = [ordered]@{
            name = "nvme$($n)n1"; path = "\\.\PhysicalDrive$n"; model = $d.FriendlyName; serial = "$($d.SerialNumber)".Trim()
            firmware = "$($d.FirmwareVersion)"; size_bytes = [uint64]$d.Size; rotational = $false; removable = $false
            transport = "nvme"; link = ""; mounts = @()
        }
        source = "windows-storage-query"
        nvme_identify_controller = Hex $ctrl
        nvme_identify_namespace = Hex $ns
        nvme_smart_log = Hex $log
        errors = $errors
    }
    $file = Join-Path $OutDir ("nvme$($n)n1.json")
    $dump | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $file
    "$($d.FriendlyName): ctrl=$([bool]$ctrl) ns=$([bool]$ns) log=$([bool]$log) $($errors -join '; ')"
}
