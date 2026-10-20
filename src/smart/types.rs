//! Raw S.M.A.R.T. structures and the per-disk state the CrystalDiskInfo logic works on.
//!
//! `AtaState` mirrors the fields of CrystalDiskInfo's `ATA_SMART_INFO` that the vendor
//! detection, the derived values and the health verdict read or write. Keeping the same
//! shape lets `vendor.rs`, `derive.rs` and `health.rs` follow the original line by line.

use serde::{Deserialize, Serialize};

/// Slots in a SMART READ DATA page: (512 - 2 - 150) / 12 = 30.
pub const MAX_ATTRIBUTE: usize = 30;

/// One 12-byte entry of SMART READ DATA (ATA/ATAPI-8, vendor-specific but de facto layout).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawAttribute {
    pub id: u8,
    pub flags: u16,
    pub current: u8,
    pub worst: u8,
    pub raw: [u8; 6],
    pub reserved: u8,
}

impl RawAttribute {
    pub fn parse(b: &[u8]) -> Self {
        RawAttribute {
            id: b[0],
            flags: u16::from_le_bytes([b[1], b[2]]),
            current: b[3],
            worst: b[4],
            raw: [b[5], b[6], b[7], b[8], b[9], b[10]],
            reserved: b[11],
        }
    }

    /// `B8toB16le`: the low two raw bytes.
    pub fn raw16(&self) -> u16 {
        u16::from_le_bytes([self.raw[0], self.raw[1]])
    }

    /// `B8toB32le`: the low four raw bytes.
    pub fn raw32(&self) -> u32 {
        u32::from_le_bytes([self.raw[0], self.raw[1], self.raw[2], self.raw[3]])
    }

    /// `B8toINTle`: the low four raw bytes as a signed 32-bit integer.
    pub fn raw_int(&self) -> i32 {
        self.raw32() as i32
    }

    /// `B8toB64le`: all six raw bytes (the 48-bit raw value).
    pub fn raw48(&self) -> u64 {
        let r = &self.raw;
        u64::from_le_bytes([r[0], r[1], r[2], r[3], r[4], r[5], 0, 0])
    }
}

/// CrystalDiskInfo's `VENDOR_ID`. The discriminants match `AtaSmart.h` so dumps stay comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum Vendor {
    #[default]
    HddGeneral = 0,
    SsdGeneral = 1,
    Mtron = 2,
    Indilinx = 3,
    JMicron = 4,
    Intel = 5,
    Samsung = 6,
    SandForce = 7,
    Micron = 8,
    Ocz = 9,
    Seagate = 10,
    Wdc = 11,
    Plextor = 12,
    SanDisk = 13,
    OczVector = 14,
    Toshiba = 15,
    Corsair = 16,
    Kingston = 17,
    MicronMu03 = 18,
    Nvme = 19,
    Realtek = 20,
    SkHynix = 21,
    Kioxia = 22,
    Ssstc = 23,
    IntelDc = 24,
    Apacer = 25,
    SiliconMotion = 26,
    Phison = 27,
    Marvell = 28,
    Maxiotek = 29,
    Ymtc = 30,
    Scy = 31,
    Recadata = 42,
    SanDiskDell = 43,
    SanDiskHp = 44,
    SanDiskHpVenus = 45,
    SanDiskLenovo = 46,
    SanDiskLenovoHelenVenus = 47,
    SanDiskCloud = 48,
    SiliconMotionCvc = 49,
    AdataIndustrial = 50,
    /// `VENDOR_UNKNOWN` in `AddDiskNVMe` before the NVMe branch overwrites it.
    Unknown = 99,
}

impl Vendor {
    pub fn label(self) -> &'static str {
        use Vendor::*;
        match self {
            HddGeneral => "HDD",
            SsdGeneral => "SSD",
            Mtron => "MTRON",
            Indilinx => "Indilinx",
            JMicron => "JMicron",
            Intel => "Intel",
            Samsung => "Samsung",
            SandForce => "SandForce",
            Micron => "Micron",
            Ocz => "OCZ",
            Seagate => "Seagate",
            Wdc => "WDC",
            Plextor => "Plextor",
            SanDisk => "SanDisk",
            OczVector => "OCZ Vector",
            Toshiba => "Toshiba",
            Corsair => "Corsair",
            Kingston => "Kingston",
            MicronMu03 => "Micron (MU03)",
            Nvme => "NVMe",
            Realtek => "Realtek",
            SkHynix => "SK hynix",
            Kioxia => "KIOXIA",
            Ssstc => "SSSTC",
            IntelDc => "Intel DC",
            Apacer => "Apacer",
            SiliconMotion => "Silicon Motion",
            Phison => "Phison",
            Marvell => "Marvell",
            Maxiotek => "Maxiotek",
            Ymtc => "YMTC",
            Scy => "SCY",
            Recadata => "RECADATA",
            SanDiskDell => "SanDisk (Dell)",
            SanDiskHp => "SanDisk (HP)",
            SanDiskHpVenus => "SanDisk (HP Venus)",
            SanDiskLenovo => "SanDisk (Lenovo)",
            SanDiskLenovoHelenVenus => "SanDisk (Lenovo Helen/Venus)",
            SanDiskCloud => "SanDisk (Cloud)",
            SiliconMotionCvc => "Silicon Motion (CVC)",
            AdataIndustrial => "ADATA Industrial",
            Unknown => "Unknown",
        }
    }
}

/// `HOST_READS_WRITES_UNIT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HostRwUnit {
    #[default]
    Unknown,
    B512,
    Mb1,
    Mb16,
    Mb32,
    Gb,
}

/// `NAND_WRITES_UNIT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NandWritesUnit {
    #[default]
    Unknown,
    Gb,
    Mb1,
}

/// `POWER_ON_HOURS_UNIT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimeUnit {
    #[default]
    Unknown,
    Hours,
    Minutes,
    HalfMinutes,
    Seconds,
    TenMinutes,
    MilliSeconds,
}

/// `TRANSFER_MODE`, only the ordering matters (`>= SATA_300`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum TransferMode {
    #[default]
    Unknown,
    Pio,
    PioDma,
    UltraDma16,
    UltraDma25,
    UltraDma33,
    UltraDma44,
    UltraDma66,
    UltraDma100,
    UltraDma133,
    Sata150,
    Sata300,
    Sata600,
}

/// The CrystalDiskInfo per-disk state, for SATA/ATA disks and for NVMe disks mapped onto
/// ATA-style attributes the way `NVMeInterpreter.cpp` does.
#[derive(Debug, Clone, Default)]
pub struct AtaState {
    pub model: String,
    pub firmware: String,
    pub serial: String,
    pub major: u32,
    pub is_ssd: bool,
    pub is_nvme: bool,
    pub transfer_mode: TransferMode,

    /// Attributes in device order, as `FillSmartData` packs them (id 0 entries dropped).
    pub attributes: Vec<RawAttribute>,
    /// Threshold for `attributes[j]`, 0 when the device reports none (`Threshold[j]`).
    pub thresholds: Vec<u8>,
    /// The raw 512-byte SMART READ DATA page (some detections peek into it).
    pub smart_read_data: Vec<u8>,

    pub vendor: Vendor,
    pub smart_key: &'static str,
    pub host_rw_unit: HostRwUnit,
    pub nand_writes_unit: NandWritesUnit,
    pub detected_time_unit: TimeUnit,
    pub measured_time_unit: TimeUnit,
    pub temperature_multiplier: f64,

    pub is_raw_values8: bool,
    pub is_raw_values7: bool,
    pub is_smart_correct: bool,
    pub is_threshold_correct: bool,
    pub is_threshold_bug: bool,

    pub flag_life_raw_value: bool,
    pub flag_life_raw_value_increment: bool,
    pub flag_life_no_report: bool,
    pub flag_life_sandisk_0_1: bool,
    pub flag_life_sandisk_1: bool,
    pub flag_life_sandisk_lenovo: bool,
    pub flag_life_sandisk_cloud: bool,
    pub flag_life_sandisk_usb_memory: bool,

    /// Derived values; -1 (or -1000 for temperature) means "not reported", as in the original.
    pub life: i32,
    pub temperature: i32,
    pub power_on_raw: u32,
    pub detected_power_on_hours: i64,
    pub measured_power_on_hours: i64,
    pub power_on_count: i64,
    pub host_writes_gb: i64,
    pub host_reads_gb: i64,
    pub nand_writes_gb: i64,
    pub gbytes_erased: i64,
    pub wear_leveling_count: i64,
}

impl AtaState {
    pub fn new(model: &str, firmware: &str, serial: &str) -> Self {
        AtaState {
            model: model.trim().to_string(),
            firmware: firmware.trim().to_string(),
            serial: serial.trim().to_string(),
            smart_key: "Smart",
            temperature_multiplier: 1.0,
            life: -1,
            temperature: -1000,
            detected_power_on_hours: -1,
            measured_power_on_hours: -1,
            host_writes_gb: -1,
            host_reads_gb: -1,
            nand_writes_gb: -1,
            gbytes_erased: -1,
            wear_leveling_count: -1,
            ..Default::default()
        }
    }

    /// `asi.Attribute[k].Id`, with the zero padding of the fixed 30-slot C array.
    pub fn id(&self, k: usize) -> u8 {
        self.attributes.get(k).map_or(0, |a| a.id)
    }

    /// Whether the first attributes have exactly these ids, the signature test used all over
    /// the vendor detection.
    pub fn ids_start_with(&self, ids: &[u8]) -> bool {
        ids.iter().enumerate().all(|(k, &id)| self.id(k) == id)
    }

    pub fn threshold(&self, j: usize) -> u8 {
        self.thresholds.get(j).copied().unwrap_or(0)
    }
}

/// `CString::Find(s) == 0`.
pub fn starts(hay: &str, needle: &str) -> bool {
    hay.starts_with(needle)
}

/// `CString::Find(s) >= 0`.
pub fn has(hay: &str, needle: &str) -> bool {
    hay.contains(needle)
}

/// `CString::Find(s) > 0`: found, and not at the start.
pub fn has_after_start(hay: &str, needle: &str) -> bool {
    hay.find(needle).is_some_and(|i| i > 0)
}
