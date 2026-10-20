//! Linux: disk discovery through sysfs and raw reads through ioctls.
//!
//! - SATA and SAT bridges (most USB-SATA adapters): SG_IO with ATA PASS-THROUGH (16), and
//!   (12) for bridges that only take the short form (T10 SAT-4, 12.2).
//! - NVMe: `NVME_IOCTL_ADMIN_CMD` with Identify (CNS 1 and 0) and Get Log Page 0x02.
//!
//! Both need root (CAP_SYS_RAWIO for SG_IO pass-through, CAP_SYS_ADMIN for NVMe admin
//! commands on most kernels).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use super::{BlockDevice, Transport};

fn read_trim(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

/// Kernel names that are never physical disks.
fn skipped(name: &str) -> bool {
    [
        "loop", "ram", "zram", "dm-", "md", "sr", "fd", "nbd", "zd", "drbd",
    ]
    .iter()
    .any(|p| name.starts_with(p))
        || (name.starts_with("mmcblk") && name.contains("boot"))
        || name.contains("rpmb")
}

fn transport_of(name: &str, sys: &Path) -> Transport {
    let real = fs::canonicalize(sys).unwrap_or_default();
    let real = real.to_string_lossy();
    // Some NAS systems (TerraMaster TOS) rename NVMe namespaces to `sdz*`, so look at
    // where the block device hangs in sysfs rather than at its name.
    if name.starts_with("nvme") || real.contains("/nvme/") {
        Transport::Nvme
    } else if name.starts_with("vd") || real.contains("/virtio") {
        Transport::Virtio
    } else if name.starts_with("mmcblk") {
        Transport::Mmc
    } else if real.contains("/usb") {
        Transport::Usb
    } else if real.contains("/ata") {
        Transport::Sata
    } else if name.starts_with("sd") {
        Transport::Sas
    } else {
        Transport::Other
    }
}

/// `nvme0n1` -> `nvme0`.
pub fn nvme_controller(name: &str) -> Option<String> {
    let rest = name.strip_prefix("nvme")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then(|| format!("nvme{digits}"))
}

/// The controller of an NVMe block device from its sysfs path
/// (`.../nvme/nvme0/nvme0n1` or, renamed, `.../nvme/nvme0/sdza`), falling back to the name.
pub fn nvme_controller_of(name: &str) -> Option<String> {
    let real = fs::canonicalize(Path::new("/sys/block").join(name)).ok()?;
    let parts: Vec<String> = real
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    parts
        .windows(2)
        .find(|w| w[0] == "nvme" && w[1].starts_with("nvme"))
        .map(|w| w[1].clone())
        .or_else(|| nvme_controller(name))
}

/// Namespace id: sysfs `nsid` when present, else the `nN` suffix, else 1.
fn nvme_nsid(name: &str) -> u32 {
    read_trim(Path::new("/sys/block").join(name).join("nsid"))
        .and_then(|s| s.parse().ok())
        .or_else(|| name.rsplit_once('n').and_then(|(_, n)| n.parse().ok()))
        .unwrap_or(1)
}

fn pcie_gen(speed: &str) -> String {
    let gts: f64 = speed
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let gen = match gts as u32 {
        2 => "1.0",
        5 => "2.0",
        8 => "3.0",
        16 => "4.0",
        32 => "5.0",
        64 => "6.0",
        _ => return speed.to_string(),
    };
    format!("PCIe {gen}")
}

fn nvme_link(ctrl: &str) -> String {
    let base = PathBuf::from(format!("/sys/class/nvme/{ctrl}/device"));
    let get = |f: &str| read_trim(base.join(f)).unwrap_or_default();
    let (cs, cw, ms, mw) = (
        get("current_link_speed"),
        get("current_link_width"),
        get("max_link_speed"),
        get("max_link_width"),
    );
    if cs.is_empty() {
        return String::new();
    }
    format!("{} x{} | {} x{}", pcie_gen(&cs), cw, pcie_gen(&ms), mw)
}

/// Map each disk to the mount points of its partitions (and of itself).
fn mounts_by_disk(disks: &[String]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(text) = fs::read_to_string("/proc/self/mounts") else {
        return out;
    };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(src), Some(dst)) = (it.next(), it.next()) else {
            continue;
        };
        let Some(dev) = src.strip_prefix("/dev/") else {
            continue;
        };
        let dev = kernel_name(src).unwrap_or_else(|| dev.to_string());
        let dst = dst.replace("\\040", " ");
        for disk in disks {
            if stacked_on(disk).contains(&dev) {
                let v = out.entry(disk.clone()).or_default();
                if !v.contains(&dst) {
                    v.push(dst.clone());
                }
            }
        }
    }
    out
}

/// Kernel name of a device node through its major:minor, since `/dev/mapper/*` are plain
/// nodes rather than symlinks on some systems.
fn kernel_name(node: &str) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let rdev = fs::metadata(node).ok()?.rdev();
    let (major, minor) = (
        libc::major(rdev as libc::dev_t),
        libc::minor(rdev as libc::dev_t),
    );
    let real = fs::canonicalize(format!("/sys/dev/block/{major}:{minor}")).ok()?;
    real.file_name().map(|f| f.to_string_lossy().to_string())
}

/// The disk, its partitions and everything stacked on them (md RAID, LVM, LUKS...), by
/// following `holders/` in sysfs.
fn stacked_on(disk: &str) -> Vec<String> {
    let mut seen = vec![disk.to_string()];
    let mut todo = vec![Path::new("/sys/block").join(disk)];
    if let Ok(parts) = fs::read_dir(Path::new("/sys/block").join(disk)) {
        for p in parts.flatten() {
            let n = p.file_name().to_string_lossy().to_string();
            if n.starts_with(disk) || p.path().join("partition").exists() {
                seen.push(n);
                todo.push(p.path());
            }
        }
    }
    while let Some(dir) = todo.pop() {
        let Ok(holders) = fs::read_dir(dir.join("holders")) else {
            continue;
        };
        for h in holders.flatten() {
            let n = h.file_name().to_string_lossy().to_string();
            if !seen.contains(&n) {
                seen.push(n.clone());
                todo.push(Path::new("/sys/block").join(&n));
            }
        }
    }
    seen
}

pub fn discover() -> Vec<BlockDevice> {
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !skipped(n))
        .collect();
    names.sort_by_key(|n| natural_key(n));
    let mounts = mounts_by_disk(&names);

    let mut out = Vec::new();
    for name in names {
        let sys = Path::new("/sys/block").join(&name);
        let sectors: u64 = read_trim(sys.join("size"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        let transport = transport_of(&name, &sys);
        let dev = sys.join("device");
        let (model, serial, firmware, link) = if transport == Transport::Nvme {
            let ctrl = nvme_controller_of(&name).unwrap_or_default();
            let c = PathBuf::from(format!("/sys/class/nvme/{ctrl}"));
            (
                read_trim(c.join("model")).unwrap_or_default(),
                read_trim(c.join("serial")).unwrap_or_default(),
                read_trim(c.join("firmware_rev")).unwrap_or_default(),
                nvme_link(&ctrl),
            )
        } else {
            let vendor = read_trim(dev.join("vendor")).unwrap_or_default();
            let model = read_trim(dev.join("model")).unwrap_or_default();
            let model = if vendor.is_empty() || vendor == "ATA" {
                model
            } else {
                format!("{vendor} {model}")
            };
            (
                model,
                read_trim(dev.join("serial")).unwrap_or_default(),
                read_trim(dev.join("rev")).unwrap_or_default(),
                String::new(),
            )
        };
        out.push(BlockDevice {
            path: format!("/dev/{name}"),
            model,
            serial,
            firmware,
            size_bytes: sectors * 512,
            rotational: read_trim(sys.join("queue/rotational")).map(|r| r == "1"),
            removable: read_trim(sys.join("removable")).as_deref() == Some("1"),
            transport,
            link,
            mounts: mounts.get(&name).cloned().unwrap_or_default(),
            name,
        });
    }
    out
}

/// `sda` < `sdb` < `sdaa`, `nvme2n1` < `nvme10n1`.
fn natural_key(s: &str) -> (String, usize, String, u64) {
    let family = if s.starts_with("nvme") {
        "nvme"
    } else {
        s.get(..2).unwrap_or(s)
    };
    let alpha: String = s.chars().take_while(|c| !c.is_ascii_digit()).collect();
    let num: String = s[alpha.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (
        family.to_string(),
        alpha.len(),
        alpha,
        num.parse().unwrap_or(0),
    )
}

fn open_rw(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .or_else(|_| {
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)
        })
}

// ---- SG_IO / ATA PASS-THROUGH ------------------------------------------------------------

const SG_IO: libc::c_ulong = 0x2285;
const SG_DXFER_FROM_DEV: libc::c_int = -3;

#[repr(C)]
struct SgIoHdr {
    interface_id: libc::c_int,
    dxfer_direction: libc::c_int,
    cmd_len: libc::c_uchar,
    mx_sb_len: libc::c_uchar,
    iovec_count: libc::c_ushort,
    dxfer_len: libc::c_uint,
    dxferp: *mut libc::c_void,
    cmdp: *const libc::c_uchar,
    sbp: *mut libc::c_uchar,
    timeout: libc::c_uint,
    flags: libc::c_uint,
    pack_id: libc::c_int,
    usr_ptr: *mut libc::c_void,
    status: libc::c_uchar,
    masked_status: libc::c_uchar,
    msg_status: libc::c_uchar,
    sb_len_wr: libc::c_uchar,
    host_status: libc::c_ushort,
    driver_status: libc::c_ushort,
    resid: libc::c_int,
    duration: libc::c_uint,
    info: libc::c_uint,
}

/// One 512-byte PIO data-in ATA command.
#[derive(Clone, Copy)]
pub struct AtaCmd {
    pub command: u8,
    pub features: u8,
    pub lba_mid: u8,
    pub lba_high: u8,
}

pub const IDENTIFY: AtaCmd = AtaCmd {
    command: 0xEC,
    features: 0,
    lba_mid: 0,
    lba_high: 0,
};
pub const SMART_READ_DATA: AtaCmd = AtaCmd {
    command: 0xB0,
    features: 0xD0,
    lba_mid: 0x4F,
    lba_high: 0xC2,
};
pub const SMART_READ_THRESHOLDS: AtaCmd = AtaCmd {
    command: 0xB0,
    features: 0xD1,
    lba_mid: 0x4F,
    lba_high: 0xC2,
};

/// T10 SAT: protocol 4 (PIO data-in), T_DIR from device, BYT_BLOK, T_LENGTH in the
/// sector count field.
fn cdb(cmd: AtaCmd, long: bool) -> Vec<u8> {
    const PIO_IN: u8 = 4 << 1;
    const FLAGS: u8 = 0x0E;
    if long {
        let mut c = vec![0u8; 16];
        c[0] = 0x85;
        c[1] = PIO_IN;
        c[2] = FLAGS;
        c[4] = cmd.features;
        c[6] = 1;
        c[10] = cmd.lba_mid;
        c[12] = cmd.lba_high;
        c[14] = cmd.command;
        c
    } else {
        let mut c = vec![0u8; 12];
        c[0] = 0xA1;
        c[1] = PIO_IN;
        c[2] = FLAGS;
        c[3] = cmd.features;
        c[4] = 1;
        c[6] = cmd.lba_mid;
        c[7] = cmd.lba_high;
        c[9] = cmd.command;
        c
    }
}

fn sg_io(f: &File, cdb: &[u8], buf: &mut [u8]) -> io::Result<()> {
    let mut sense = [0u8; 32];
    let mut hdr = SgIoHdr {
        interface_id: b'S' as libc::c_int,
        dxfer_direction: SG_DXFER_FROM_DEV,
        cmd_len: cdb.len() as u8,
        mx_sb_len: sense.len() as u8,
        iovec_count: 0,
        dxfer_len: buf.len() as u32,
        dxferp: buf.as_mut_ptr().cast(),
        cmdp: cdb.as_ptr(),
        sbp: sense.as_mut_ptr(),
        timeout: 10_000,
        flags: 0,
        pack_id: 0,
        usr_ptr: std::ptr::null_mut(),
        status: 0,
        masked_status: 0,
        msg_status: 0,
        sb_len_wr: 0,
        host_status: 0,
        driver_status: 0,
        resid: 0,
        duration: 0,
        info: 0,
    };
    // SAFETY: `hdr` points at buffers that outlive the call and match the lengths given.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), SG_IO as _, &mut hdr as *mut SgIoHdr) };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    if hdr.host_status != 0 || (hdr.driver_status & 0x0F) != 0 && hdr.sb_len_wr == 0 {
        return Err(io::Error::other(format!(
            "SG_IO host {:#x} driver {:#x}",
            hdr.host_status, hdr.driver_status
        )));
    }
    if hdr.status != 0 || hdr.sb_len_wr > 0 {
        sense_ok(&sense[..hdr.sb_len_wr as usize])?;
    }
    Ok(())
}

/// Some bridges answer with CHECK CONDITION and an ATA Status Return descriptor even when the
/// command worked; accept that unless the ATA status has ERR set.
fn sense_ok(sense: &[u8]) -> io::Result<()> {
    if sense.is_empty() {
        return Ok(());
    }
    let (key, asc, ascq) = match sense[0] & 0x7F {
        0x72 | 0x73 => (
            sense[1] & 0x0F,
            sense.get(2).copied(),
            sense.get(3).copied(),
        ),
        0x70 | 0x71 => (
            sense.get(2).map_or(0, |b| b & 0x0F),
            sense.get(12).copied(),
            sense.get(13).copied(),
        ),
        _ => return Err(io::Error::other("unknown sense format")),
    };
    // Descriptor 0x09: ATA Status Return, status byte at offset 13 of the descriptor.
    if sense[0] & 0x7F == 0x72 && sense.len() >= 22 && sense[8] == 0x09 {
        let ata_status = sense[8 + 13];
        if ata_status & 0x01 == 0 {
            return Ok(());
        }
    }
    if key == 0 || key == 1 || (asc == Some(0) && ascq == Some(0x1D)) {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "sense key {key:#x} asc {:#x} ascq {:#x}",
        asc.unwrap_or(0),
        ascq.unwrap_or(0)
    )))
}

/// Send one ATA command, trying the 16-byte CDB and then the 12-byte one.
pub fn ata_read(f: &File, cmd: AtaCmd, prefer_long: &mut bool) -> io::Result<Vec<u8>> {
    let mut last = None;
    for long in [*prefer_long, !*prefer_long] {
        let mut buf = vec![0u8; 512];
        match sg_io(f, &cdb(cmd, long), &mut buf) {
            Ok(()) if buf.iter().any(|&b| b != 0) => {
                *prefer_long = long;
                return Ok(buf);
            }
            Ok(()) => last = Some(io::Error::other("empty reply")),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("no reply")))
}

pub struct AtaRead {
    pub pages: crate::smart::ata::AtaPages,
    pub errors: Vec<String>,
}

pub fn read_ata(path: &str) -> io::Result<AtaRead> {
    let f = open_rw(path)?;
    let mut long = true;
    let identify = ata_read(&f, IDENTIFY, &mut long)?;
    // An IDENTIFY with a checksum signature must sum to zero (ACS-4 7.12.7.88).
    if identify[510] == 0xA5 && identify.iter().fold(0u8, |a, &b| a.wrapping_add(b)) != 0 {
        return Err(io::Error::other("IDENTIFY checksum mismatch"));
    }
    let mut errors = Vec::new();
    let mut get = |cmd, what: &str| match ata_read(&f, cmd, &mut long) {
        Ok(p) => Some(p),
        Err(e) => {
            errors.push(format!("{what}: {e}"));
            None
        }
    };
    let smart = get(SMART_READ_DATA, "SMART READ DATA");
    let smart_check = smart
        .as_ref()
        .and_then(|_| get(SMART_READ_DATA, "SMART READ DATA"));
    let thresholds = smart
        .as_ref()
        .and_then(|_| get(SMART_READ_THRESHOLDS, "SMART READ THRESHOLDS"));
    Ok(AtaRead {
        pages: crate::smart::ata::AtaPages {
            identify,
            smart,
            smart_check,
            thresholds,
        },
        errors,
    })
}

// ---- NVMe admin commands ---------------------------------------------------------------------

#[repr(C)]
#[derive(Default)]
struct NvmePassthru {
    opcode: u8,
    flags: u8,
    rsvd1: u16,
    nsid: u32,
    cdw2: u32,
    cdw3: u32,
    metadata: u64,
    addr: u64,
    metadata_len: u32,
    data_len: u32,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
    timeout_ms: u32,
    result: u32,
}

/// `_IOWR('N', 0x41, struct nvme_admin_cmd)`, 72 bytes.
const NVME_IOCTL_ADMIN_CMD: libc::c_ulong = 0xC048_4E41;

fn nvme_admin(f: &File, opcode: u8, nsid: u32, cdw10: u32, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut cmd = NvmePassthru {
        opcode,
        nsid,
        addr: buf.as_mut_ptr() as u64,
        data_len: len as u32,
        cdw10,
        timeout_ms: 10_000,
        ..Default::default()
    };
    // SAFETY: `addr`/`data_len` describe `buf`, which lives until after the call.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), NVME_IOCTL_ADMIN_CMD as _, &mut cmd) };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    if r > 0 {
        return Err(io::Error::other(format!("NVMe status {r:#x}")));
    }
    Ok(buf)
}

pub fn read_nvme(name: &str) -> io::Result<crate::smart::nvme::NvmePages> {
    let ctrl = nvme_controller_of(name).unwrap_or_else(|| name.to_string());
    let f = open_rw(&format!("/dev/{ctrl}")).or_else(|_| open_rw(&format!("/dev/{name}")))?;
    let identify_controller = nvme_admin(&f, 0x06, 0, 1, 4096)?;
    let nsid = nvme_nsid(name);
    let identify_namespace = nvme_admin(&f, 0x06, nsid, 0, 4096).ok();
    // Get Log Page 0x02, whole controller, NUMDL = 512 / 4 - 1.
    let smart_log = nvme_admin(&f, 0x02, 0xFFFF_FFFF, (127 << 16) | 0x02, 512).ok();
    Ok(crate::smart::nvme::NvmePages {
        identify_controller,
        identify_namespace,
        smart_log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_struct_sizes() {
        assert_eq!(std::mem::size_of::<NvmePassthru>(), 72);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<SgIoHdr>(), 88);
    }

    #[test]
    fn cdbs() {
        let c = cdb(SMART_READ_DATA, true);
        assert_eq!(
            c,
            [0x85, 0x08, 0x0E, 0, 0xD0, 0, 1, 0, 0, 0, 0x4F, 0, 0xC2, 0, 0xB0, 0]
        );
        let s = cdb(IDENTIFY, false);
        assert_eq!(s, [0xA1, 0x08, 0x0E, 0, 1, 0, 0, 0, 0, 0xEC, 0, 0]);
    }

    #[test]
    fn names() {
        assert_eq!(nvme_controller("nvme10n1").as_deref(), Some("nvme10"));
        assert_eq!(pcie_gen("16.0 GT/s PCIe"), "PCIe 4.0");
        assert!(skipped("loop3") && skipped("dm-0") && !skipped("sda"));
        let mut v = vec!["sdaa", "sdb", "sda", "nvme10n1", "nvme2n1"];
        v.sort_by_key(|n| natural_key(n));
        assert_eq!(v, ["nvme2n1", "nvme10n1", "sda", "sdb", "sdaa"]);
    }
}
