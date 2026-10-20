//! Windows: disk discovery and raw reads through `DeviceIoControl`.
//!
//! - Discovery: `\\.\PhysicalDriveN` opened with no access rights (works without
//!   administrator), `StorageDeviceProperty`, `StorageDeviceSeekPenaltyProperty`,
//!   `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`, and drive letters through
//!   `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`.
//! - NVMe: `IOCTL_STORAGE_QUERY_PROPERTY` with `StorageAdapterProtocolSpecificProperty`
//!   (Identify and log page 0x02), the path of CrystalDiskInfo's
//!   `DoIdentifyDeviceNVMeStorageQuery`. No administrator needed.
//! - SATA: `IOCTL_ATA_PASS_THROUGH` (needs administrator).

use std::ffi::c_void;
use std::io;

use super::{BlockDevice, Transport};
use crate::smart::{ata::AtaPages, nvme::NvmePages};

type Handle = *mut c_void;
const INVALID_HANDLE: Handle = -1isize as Handle;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_RW: u32 = 3;
const OPEN_EXISTING: u32 = 3;

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn DeviceIoControl(
        h: Handle,
        code: u32,
        inbuf: *mut c_void,
        inlen: u32,
        outbuf: *mut c_void,
        outlen: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(h: Handle) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
    fn ShellExecuteW(
        hwnd: Handle,
        op: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Start this executable again with the UAC prompt ("runas").
pub fn relaunch_elevated(args: &[&str]) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let params = args
        .iter()
        .map(|a| format!("\"{}\"", a.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ");
    let (op, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&params));
    // SAFETY: all strings are NUL-terminated and live across the call.
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            1,
        )
    };
    // Values above 32 mean success.
    if r > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!("ShellExecuteW returned {r}")))
    }
}

pub fn is_admin() -> bool {
    // SAFETY: no arguments, no preconditions.
    unsafe { IsUserAnAdmin() != 0 }
}

struct Device(Handle);

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateFileW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

fn open(path: &str, access: u32) -> io::Result<Device> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    let h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_RW,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE {
        Err(io::Error::last_os_error())
    } else {
        Ok(Device(h))
    }
}

/// `DeviceIoControl` with one buffer used for input and output.
fn ioctl(d: &Device, code: u32, input: &[u8], out_len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; input.len().max(out_len)];
    buf[..input.len()].copy_from_slice(input);
    let mut ret = 0u32;
    // SAFETY: both pointers describe `buf`, which lives across the call.
    let ok = unsafe {
        DeviceIoControl(
            d.0,
            code,
            buf.as_mut_ptr().cast(),
            input.len() as u32,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut ret,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(ret as usize);
    Ok(buf)
}

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;
const IOCTL_ATA_PASS_THROUGH: u32 = 0x0004_D02C;

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4)
        .map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn cstr_at(b: &[u8], off: u32) -> String {
    let off = off as usize;
    if off == 0 || off >= b.len() {
        return String::new();
    }
    let end = b[off..]
        .iter()
        .position(|&c| c == 0)
        .map_or(b.len(), |p| off + p);
    String::from_utf8_lossy(&b[off..end]).trim().to_string()
}

/// `STORAGE_PROPERTY_QUERY { PropertyId, QueryType = PropertyStandardQuery }`.
fn property(d: &Device, id: u32, out_len: usize) -> io::Result<Vec<u8>> {
    let mut q = [0u8; 12];
    q[0..4].copy_from_slice(&id.to_le_bytes());
    ioctl(d, IOCTL_STORAGE_QUERY_PROPERTY, &q, out_len)
}

fn transport(bus: u32) -> Transport {
    match bus {
        17 => Transport::Nvme,
        3 | 11 => Transport::Sata,
        7 => Transport::Usb,
        1 | 10 | 8 => Transport::Sas,
        12 => Transport::Mmc,
        _ => Transport::Other,
    }
}

/// Drive letters per physical disk number.
fn letters() -> std::collections::HashMap<u32, Vec<String>> {
    let mut out: std::collections::HashMap<u32, Vec<String>> = Default::default();
    for l in b'A'..=b'Z' {
        let letter = format!("{}:", l as char);
        let Ok(v) = open(&format!("\\\\.\\{letter}"), 0) else {
            continue;
        };
        let Ok(b) = ioctl(&v, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &[], 8 + 24 * 8) else {
            continue;
        };
        let n = u32_at(&b, 0) as usize;
        for k in 0..n {
            let disk = u32_at(&b, 8 + 24 * k);
            out.entry(disk).or_default().push(letter.clone());
        }
    }
    out
}

pub fn discover() -> Vec<BlockDevice> {
    let letters = letters();
    let mut out = Vec::new();
    for n in 0..64u32 {
        let path = format!("\\\\.\\PhysicalDrive{n}");
        let Ok(d) = open(&path, 0) else {
            continue;
        };
        // STORAGE_DEVICE_DESCRIPTOR: offsets of vendor, product, revision, serial; bus type.
        let Ok(desc) = property(&d, 0, 1024) else {
            continue;
        };
        let vendor = cstr_at(&desc, u32_at(&desc, 12));
        let product = cstr_at(&desc, u32_at(&desc, 16));
        let model = if vendor.is_empty() || product.starts_with(&vendor) {
            product
        } else {
            format!("{vendor} {product}")
        };
        let seek = property(&d, 7, 12).ok();
        let size = ioctl(&d, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], 256)
            .ok()
            .and_then(|g| {
                g.get(24..32)
                    .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
            })
            .unwrap_or(0);
        // Card readers without a card report no size; like empty slots on Linux, skip them.
        if size == 0 {
            continue;
        }
        out.push(BlockDevice {
            name: format!("PhysicalDrive{n}"),
            path,
            model,
            serial: cstr_at(&desc, u32_at(&desc, 24)),
            firmware: cstr_at(&desc, u32_at(&desc, 20)),
            size_bytes: size,
            rotational: seek.as_ref().and_then(|s| s.get(8)).map(|&b| b != 0),
            removable: desc.get(10).is_some_and(|&b| b != 0),
            transport: transport(u32_at(&desc, 28)),
            link: String::new(),
            mounts: letters.get(&n).cloned().unwrap_or_default(),
        });
    }
    out
}

/// `StorageAdapterProtocolSpecificProperty` + `STORAGE_PROTOCOL_SPECIFIC_DATA` for NVMe.
fn nvme_query(
    d: &Device,
    data_type: u32,
    request: u32,
    sub: u32,
    len: usize,
) -> io::Result<Vec<u8>> {
    const HDR: usize = 8 + 40;
    let mut q = vec![0u8; HDR];
    q[0..4].copy_from_slice(&49u32.to_le_bytes()); // StorageAdapterProtocolSpecificProperty
    q[8..12].copy_from_slice(&3u32.to_le_bytes()); // ProtocolTypeNvme
    q[12..16].copy_from_slice(&data_type.to_le_bytes()); // 1 identify, 2 log page
    q[16..20].copy_from_slice(&request.to_le_bytes());
    q[20..24].copy_from_slice(&sub.to_le_bytes());
    q[24..28].copy_from_slice(&40u32.to_le_bytes()); // ProtocolDataOffset
    q[28..32].copy_from_slice(&(len as u32).to_le_bytes()); // ProtocolDataLength
    let mut buf = vec![0u8; HDR + len];
    buf[..HDR].copy_from_slice(&q);
    let out = ioctl(d, IOCTL_STORAGE_QUERY_PROPERTY, &buf, HDR + len)?;
    if out.len() < HDR + len {
        return Err(io::Error::other("short NVMe reply"));
    }
    Ok(out[HDR..HDR + len].to_vec())
}

pub fn read_nvme(path: &str) -> io::Result<NvmePages> {
    let d = open(path, 0)?;
    Ok(NvmePages {
        identify_controller: nvme_query(&d, 1, 1, 0, 4096)?,
        identify_namespace: nvme_query(&d, 1, 0, 1, 4096).ok(),
        smart_log: nvme_query(&d, 2, 2, 0, 512).ok(),
    })
}

/// One PIO data-in command through `ATA_PASS_THROUGH_EX` (48 bytes on 64-bit Windows, data
/// right after it).
fn ata_cmd(
    d: &Device,
    command: u8,
    features: u8,
    lba_mid: u8,
    lba_high: u8,
) -> io::Result<Vec<u8>> {
    const HDR: usize = if cfg!(target_pointer_width = "64") {
        48
    } else {
        40
    };
    let mut b = vec![0u8; HDR + 512];
    b[0..2].copy_from_slice(&(HDR as u16).to_le_bytes()); // Length
    b[2..4].copy_from_slice(&0x0003u16.to_le_bytes()); // ATA_FLAGS_DRDY_REQUIRED | DATA_IN
    b[8..12].copy_from_slice(&512u32.to_le_bytes()); // DataTransferLength
    b[12..16].copy_from_slice(&10u32.to_le_bytes()); // TimeOutValue (s)
    let (off, tf) = if HDR == 48 { (24, 40) } else { (20, 32) };
    b[off..off + 4].copy_from_slice(&(HDR as u32).to_le_bytes()); // DataBufferOffset
                                                                  // CurrentTaskFile: features, count, lba low, lba mid, lba high, device, command.
    b[tf..tf + 7].copy_from_slice(&[features, 1, 0, lba_mid, lba_high, 0xA0, command]);
    let out = ioctl(d, IOCTL_ATA_PASS_THROUGH, &b, HDR + 512)?;
    let status = out.get(tf + 6).copied().unwrap_or(0);
    if status & 0x01 != 0 {
        return Err(io::Error::other(format!("ATA status {status:#x}")));
    }
    let data = out
        .get(HDR..HDR + 512)
        .ok_or_else(|| io::Error::other("short ATA reply"))?;
    if data.iter().all(|&x| x == 0) {
        return Err(io::Error::other("empty reply"));
    }
    Ok(data.to_vec())
}

pub struct AtaRead {
    pub pages: AtaPages,
    pub errors: Vec<String>,
}

pub fn read_ata(path: &str) -> io::Result<AtaRead> {
    let d = open(path, GENERIC_READ | GENERIC_WRITE).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("{e} (ATA pass-through needs administrator)"),
        )
    })?;
    let identify = ata_cmd(&d, 0xEC, 0, 0, 0)?;
    let mut errors = Vec::new();
    let mut get = |features, what: &str| match ata_cmd(&d, 0xB0, features, 0x4F, 0xC2) {
        Ok(p) => Some(p),
        Err(e) => {
            errors.push(format!("{what}: {e}"));
            None
        }
    };
    let smart = get(0xD0, "SMART READ DATA");
    let smart_check = smart.as_ref().and_then(|_| get(0xD0, "SMART READ DATA"));
    let thresholds = smart
        .as_ref()
        .and_then(|_| get(0xD1, "SMART READ THRESHOLDS"));
    Ok(AtaRead {
        pages: AtaPages {
            identify,
            smart,
            smart_check,
            thresholds,
        },
        errors,
    })
}
