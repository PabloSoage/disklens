//! Disk health analysis, ported from CrystalDiskInfo, and the report every front end shows.
//!
//! Nothing in this module touches a device: it turns raw pages (IDENTIFY, SMART READ DATA,
//! thresholds, NVMe log 0x02) into a [`DiskReport`]. That keeps it testable from captured
//! dumps on any OS.

pub mod ata;
pub mod derive;
pub mod health;
pub mod names;
pub mod nvme;
pub mod types;
pub mod vendor;

use serde::{Deserialize, Serialize};

pub use health::{Health, Thresholds};
pub use types::{AtaState, RawAttribute, Vendor};

/// One row of the attribute table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeRow {
    pub id: u8,
    pub name: String,
    /// Normalised values; absent for NVMe, whose "attributes" are plain counters.
    pub current: Option<u8>,
    pub worst: Option<u8>,
    pub threshold: Option<u8>,
    /// Raw bytes as CrystalDiskInfo prints them: most significant first, in hex.
    pub raw_hex: String,
    pub raw: u64,
    pub status: Health,
}

/// How the disk is attached and which command set it speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Ata,
    Nvme,
    Scsi,
    Unknown,
}

/// Everything the front ends show about one disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskReport {
    /// Kernel name (`sda`, `nvme0n1`).
    pub name: String,
    pub path: String,
    pub model: String,
    pub firmware: String,
    pub serial: String,
    pub protocol: Protocol,
    pub interface: String,
    pub transfer_mode: String,
    pub standard: String,
    pub features: Vec<String>,
    pub capacity_bytes: u64,
    pub logical_sector_size: u32,
    pub physical_sector_size: u32,
    pub is_ssd: bool,
    /// rpm for spinning disks that report it.
    pub rotation_rate: Option<u16>,
    pub form_factor: String,
    pub vendor: String,
    pub smart_table: String,
    pub smart_supported: bool,

    pub health: Health,
    pub life_percent: Option<i32>,
    pub temperature_c: Option<i32>,
    pub temperature_status: Health,
    pub alarm_temperature_c: i32,
    pub power_on_hours: Option<i64>,
    pub power_on_count: Option<i64>,
    pub host_reads_gb: Option<i64>,
    pub host_writes_gb: Option<i64>,
    pub nand_writes_gb: Option<i64>,

    pub attributes: Vec<AttributeRow>,
    /// Mount points of the disk's partitions (the drive letters of the original).
    pub mounts: Vec<String>,
    /// Where the data came from: `native` (ioctls), `smartctl`, or `sysfs` (no SMART).
    pub source: String,
    /// Why some data is missing (permissions, bridge without pass-through, ...).
    pub notes: Vec<String>,
}

fn opt(v: i64) -> Option<i64> {
    (v >= 0).then_some(v)
}

/// The six raw bytes, most significant first (for NVMe counters, their low 48 bits).
fn raw_hex(a: &RawAttribute) -> String {
    a.raw.iter().rev().map(|b| format!("{b:02X}")).collect()
}

/// Fields that do not come from the SMART analysis.
#[derive(Debug, Clone, Default)]
pub struct Platform {
    pub name: String,
    pub path: String,
    pub interface: String,
    pub transfer_mode: String,
    pub capacity_bytes: u64,
    pub rotational: Option<bool>,
    pub mounts: Vec<String>,
    pub notes: Vec<String>,
    pub source: String,
}

/// What the identify pages say beyond the CrystalDiskInfo state.
#[derive(Debug, Clone)]
pub struct Identity {
    pub protocol: Protocol,
    pub standard: String,
    pub features: Vec<String>,
    pub form_factor: String,
    pub rotation_rate: Option<u16>,
    /// Logical and physical sector sizes.
    pub sectors: (u32, u32),
}

impl Identity {
    fn none() -> Self {
        Identity {
            protocol: Protocol::Unknown,
            standard: String::new(),
            features: Vec::new(),
            form_factor: String::new(),
            rotation_rate: None,
            sectors: (0, 0),
        }
    }
}

/// Turn the CrystalDiskInfo state into the report.
pub fn report_from_state(
    mut asi: AtaState,
    id: Identity,
    plat: Platform,
    t: &Thresholds,
) -> DiskReport {
    let health = if asi.attributes.is_empty() && !asi.is_nvme {
        Health::Unknown
    } else {
        health::disk_status(&mut asi, t)
    };
    let alarm = t.alarm_for(asi.is_ssd);
    let attributes = (0..asi.attributes.len())
        .filter(|&j| asi.attributes[j].id != 0 && asi.attributes[j].id != 0xFF)
        .map(|j| {
            let a = asi.attributes[j];
            AttributeRow {
                id: a.id,
                name: names::attribute_name(asi.smart_key, a.id),
                current: (!asi.is_nvme).then_some(a.current),
                worst: (!asi.is_nvme).then_some(a.worst),
                threshold: (!asi.is_nvme && asi.is_threshold_correct).then(|| asi.threshold(j)),
                raw_hex: raw_hex(&a),
                raw: a.raw48(),
                status: health::attribute_status(&asi, j, t),
            }
        })
        .collect();

    let temperature = (asi.temperature != -1000).then_some(asi.temperature);
    let is_ssd = asi.is_ssd || plat.rotational == Some(false);
    DiskReport {
        name: plat.name,
        path: plat.path,
        model: asi.model.clone(),
        firmware: asi.firmware.clone(),
        serial: asi.serial.clone(),
        protocol: id.protocol,
        interface: plat.interface,
        transfer_mode: plat.transfer_mode,
        standard: id.standard,
        features: id.features,
        capacity_bytes: plat.capacity_bytes,
        logical_sector_size: id.sectors.0,
        physical_sector_size: id.sectors.1,
        is_ssd,
        rotation_rate: id.rotation_rate,
        form_factor: id.form_factor,
        vendor: asi.vendor.label().to_string(),
        smart_table: asi.smart_key.to_string(),
        smart_supported: !asi.attributes.is_empty(),
        health,
        life_percent: (asi.life >= 0 && is_ssd).then_some(asi.life),
        temperature_c: temperature,
        temperature_status: health::temperature_status(asi.temperature, alarm),
        alarm_temperature_c: alarm,
        power_on_hours: opt(asi.detected_power_on_hours),
        power_on_count: (asi.is_smart_correct && asi.power_on_count > 0)
            .then_some(asi.power_on_count),
        host_reads_gb: opt(asi.host_reads_gb),
        host_writes_gb: opt(asi.host_writes_gb),
        nand_writes_gb: opt(asi.nand_writes_gb),
        attributes,
        mounts: plat.mounts,
        source: plat.source,
        notes: plat.notes,
    }
}

/// A disk we could see but not query: model and size from sysfs, health unknown.
pub fn report_without_smart(
    model: &str,
    serial: &str,
    firmware: &str,
    plat: Platform,
) -> DiskReport {
    let asi = AtaState::new(model, firmware, serial);
    let ssd = plat.rotational == Some(false);
    let mut r = report_from_state(asi, Identity::none(), plat, &Thresholds::default());
    r.is_ssd = ssd;
    r.vendor = if ssd { "SSD" } else { "" }.to_string();
    r
}

/// Analyse an ATA disk from its raw pages.
pub fn analyze_ata(
    pages: &ata::AtaPages,
    mut plat: Platform,
    t: &Thresholds,
) -> Option<DiskReport> {
    let (asi, info) = ata::analyze(pages)?;
    if plat.interface.is_empty() {
        plat.interface = info.interface.clone();
    }
    if plat.transfer_mode.is_empty() {
        plat.transfer_mode = format!(
            "{} | {}",
            info.transfer_mode_current, info.transfer_mode_max
        );
    }
    if plat.capacity_bytes == 0 {
        plat.capacity_bytes = info.sectors * info.logical_sector_size as u64;
    }
    let rot = (info.rotation_rate > 1).then_some(info.rotation_rate);
    let id = Identity {
        protocol: Protocol::Ata,
        standard: info.major_version,
        features: info.features,
        form_factor: info.form_factor,
        rotation_rate: rot,
        sectors: (info.logical_sector_size, info.physical_sector_size),
    };
    Some(report_from_state(asi, id, plat, t))
}

/// Analyse an NVMe disk from its raw pages.
pub fn analyze_nvme(
    pages: &nvme::NvmePages,
    mut plat: Platform,
    t: &Thresholds,
) -> Option<DiskReport> {
    let (asi, info) = nvme::analyze(pages)?;
    if plat.interface.is_empty() {
        plat.interface = "NVM Express".into();
    }
    if plat.capacity_bytes == 0 {
        plat.capacity_bytes = info.capacity_bytes;
    }
    let lba = if info.lba_size == 0 {
        512
    } else {
        info.lba_size
    };
    let id = Identity {
        protocol: Protocol::Nvme,
        standard: info.version,
        features: info.features,
        form_factor: String::new(),
        rotation_rate: None,
        sectors: (lba, lba),
    };
    Some(report_from_state(asi, id, plat, t))
}
