//! Finding disks and reading their raw pages.
//!
//! On Linux the native backend talks to the devices directly (SG_IO with ATA PASS-THROUGH
//! for SATA and USB-SATA bridges, NVMe admin commands for NVMe). When that is not possible
//! the `smartctl` backend reads the same data through smartmontools, which knows many more
//! bridges and RAID controllers. Other systems only get the `smartctl` backend.

#[cfg(target_os = "linux")]
pub mod linux;
pub mod smartctl;

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
    #[cfg(not(target_os = "linux"))]
    {
        smartctl::discover()
    }
}

/// Whether this process can send raw commands (root, or the right capabilities).
pub fn privileged() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}
