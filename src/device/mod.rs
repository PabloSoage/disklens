//! Finding disks and reading their raw pages.
//!
//! The native backend talks to the devices directly:
//! - Linux: SG_IO with ATA PASS-THROUGH for SATA and USB-SATA bridges, NVMe admin commands.
//! - Windows: `IOCTL_STORAGE_QUERY_PROPERTY` for NVMe, `IOCTL_ATA_PASS_THROUGH` for SATA.
//!
//! When that is not possible the `smartctl` backend reads the same data through
//! smartmontools, which knows many more bridges and RAID controllers. Other systems (macOS,
//! the BSDs) only get the `smartctl` backend.

#[cfg(target_os = "linux")]
pub mod linux;
pub mod smartctl;
#[cfg(windows)]
pub mod windows;

/// The native reads, behind one interface for every OS.
pub mod native {
    use super::BlockDevice;
    use crate::smart::{ata::AtaPages, nvme::NvmePages};
    use std::io;

    #[cfg(target_os = "linux")]
    pub fn read_nvme(d: &BlockDevice) -> io::Result<NvmePages> {
        super::linux::read_nvme(&d.name)
    }
    #[cfg(target_os = "linux")]
    pub fn read_ata(d: &BlockDevice) -> io::Result<(AtaPages, Vec<String>)> {
        super::linux::read_ata(&d.path).map(|r| (r.pages, r.errors))
    }

    #[cfg(windows)]
    pub fn read_nvme(d: &BlockDevice) -> io::Result<NvmePages> {
        super::windows::read_nvme(&d.path)
    }
    #[cfg(windows)]
    pub fn read_ata(d: &BlockDevice) -> io::Result<(AtaPages, Vec<String>)> {
        super::windows::read_ata(&d.path).map(|r| (r.pages, r.errors))
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub fn read_nvme(_d: &BlockDevice) -> io::Result<NvmePages> {
        Err(io::Error::other(
            "no native backend on this OS, install smartmontools",
        ))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    pub fn read_ata(_d: &BlockDevice) -> io::Result<(AtaPages, Vec<String>)> {
        Err(io::Error::other(
            "no native backend on this OS, install smartmontools",
        ))
    }
}

use serde::{Deserialize, Serialize};

/// A block device as the OS lists it, before any SMART query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockDevice {
    /// Kernel name: `sda`, `nvme0n1`.
    pub name: String,
    /// Device node: `/dev/sda`.
    pub path: String,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub size_bytes: u64,
    pub rotational: Option<bool>,
    pub removable: bool,
    pub transport: Transport,
    /// `PCIe 4.0 x4 | PCIe 4.0 x4` for NVMe, from sysfs.
    pub link: String,
    pub mounts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Nvme,
    Sata,
    Usb,
    Sas,
    Virtio,
    Mmc,
    #[default]
    Other,
}

impl Transport {
    pub fn label(self) -> &'static str {
        match self {
            Transport::Nvme => "NVM Express",
            Transport::Sata => "Serial ATA",
            Transport::Usb => "USB",
            Transport::Sas => "SAS/SCSI",
            Transport::Virtio => "VirtIO",
            Transport::Mmc => "MMC",
            Transport::Other => "",
        }
    }
}

/// Which way to read SMART data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// Native first, `smartctl` when native fails.
    #[default]
    Auto,
    /// Only the built-in ioctls.
    Native,
    /// Only smartmontools' `smartctl`.
    Smartctl,
}

/// List the disks of this machine.
pub fn discover() -> Vec<BlockDevice> {
    #[cfg(target_os = "linux")]
    {
        linux::discover()
    }
    #[cfg(windows)]
    {
        windows::discover()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        smartctl::discover()
    }
}

/// How to get the rights `privileged` checks, for messages.
pub fn privilege_hint() -> &'static str {
    if cfg!(windows) {
        "run it as administrator (NVMe disks work without it)"
    } else {
        "run it as root (sudo, pkexec or the systemd service)"
    }
}

/// Whether this process can send raw commands (root, or the right capabilities).
pub fn privileged() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(windows)]
    {
        windows::is_admin()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}
