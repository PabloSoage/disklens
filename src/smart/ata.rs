//! ATA IDENTIFY DEVICE parsing and the `AddDisk` sequence of CrystalDiskInfo for an ATA disk.

use serde::{Deserialize, Serialize};

use super::derive;
use super::types::*;
use super::vendor;

/// The IDENTIFY DEVICE words CrystalDiskInfo reads (ATA8-ACS / ACS-4 word numbers).
#[derive(Debug, Clone)]
pub struct Identify {
    w: Vec<u16>,
}

impl Identify {
    pub fn parse(page: &[u8]) -> Option<Self> {
        if page.len() < 512 {
            return None;
        }
        let w = page[..512]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(Identify { w })
    }

    pub fn word(&self, i: usize) -> u16 {
        self.w[i]
    }

    fn dword(&self, i: usize) -> u32 {
        self.w[i] as u32 | (self.w[i + 1] as u32) << 16
    }

    fn qword(&self, i: usize) -> u64 {
        (0..4).fold(0u64, |acc, k| acc | (self.w[i + k] as u64) << (16 * k))
    }

    /// ATA strings store two characters per word, high byte first (`ChangeByteOrder`).
    fn string(&self, from: usize, to: usize) -> String {
        let mut bytes = Vec::with_capacity((to - from) * 2);
        for &w in &self.w[from..to] {
            bytes.push((w >> 8) as u8);
            bytes.push(w as u8);
        }
        let s: String = bytes
            .iter()
            .map(|&b| {
                if (0x20..0x7F).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect();
        s.trim().to_string()
    }

    pub fn serial(&self) -> String {
        self.string(10, 20)
    }
    pub fn firmware(&self) -> String {
        self.string(23, 27)
    }
    pub fn model(&self) -> String {
        self.string(27, 47)
    }
}

/// What the information panel shows about an ATA disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AtaInfo {
    pub major_version: String,
    pub minor_version_code: u16,
    pub interface: String,
    pub transfer_mode_current: String,
    pub transfer_mode_max: String,
    /// 1 for SSDs, rpm for HDDs that report it, 0 otherwise.
    pub rotation_rate: u16,
    pub form_factor: String,
    pub logical_sector_size: u32,
    pub physical_sector_size: u32,
    pub sectors: u64,
    pub buffer_size_bytes: u64,
    pub features: Vec<String>,
    pub apm: Option<u8>,
    pub aam: Option<u8>,
}

/// `GetAtaMajorVersion`.
pub fn major_version(w80: u16) -> (u32, String) {
    if w80 == 0 || w80 == 0xFFFF {
        return (0, "----".into());
    }
    let major = (1..=14).rev().find(|&i| (w80 >> i) & 1 == 1).unwrap_or(0) as u32;
    let s = if major >= 9 {
        format!("ACS-{}", major - 7)
    } else if major == 8 {
        "ATA8-ACS".into()
    } else if major >= 4 {
        format!("ATA/ATAPI-{major}")
    } else if major == 0 {
        "----".into()
    } else {
        format!("ATA-{major}")
    };
    (major, s)
}

/// `GetTransferMode`: (mode, current, max, interface type).
pub fn transfer_mode(
    w63: u16,
    w76: u16,
    w77: u16,
    w88: u16,
) -> (TransferMode, String, String, String) {
    let mut tm = TransferMode::Unknown;
    let (mut cur, mut max, mut kind) = (String::new(), String::new(), String::new());
    if w63 & 0x0700 != 0 {
        tm = TransferMode::PioDma;
        cur = "PIO/DMA".into();
        max = cur.clone();
    }
    if w88 & 0x7F != 0 {
        kind = "Parallel ATA".into();
    }
    let udma = [
        (0x0040, TransferMode::UltraDma133, "UDMA/133"),
        (0x0020, TransferMode::UltraDma100, "UDMA/100"),
        (0x0010, TransferMode::UltraDma66, "UDMA/66"),
        (0x0008, TransferMode::UltraDma44, "UDMA/44"),
        (0x0004, TransferMode::UltraDma33, "UDMA/33"),
        (0x0002, TransferMode::UltraDma25, "UDMA/25"),
        (0x0001, TransferMode::UltraDma16, "UDMA/16"),
    ];
    if let Some(&(_, m, s)) = udma.iter().find(|(bit, _, _)| w88 & bit != 0) {
        tm = m;
        max = s.into();
    }
    if let Some(&(_, _, s)) = udma.iter().find(|(bit, _, _)| w88 & (bit << 8) != 0) {
        cur = s.into();
    }
    if w76 != 0 && w76 != 0xFFFF {
        cur = "SATA/150".into();
        max = cur.clone();
        kind = "Serial ATA".into();
    }
    if w76 & 0x0010 != 0 {
        tm = TransferMode::Unknown;
        cur = "----".into();
        max = cur.clone();
    } else if w76 & 0x0008 != 0 {
        tm = TransferMode::Sata600;
        cur = "----".into();
        max = "SATA/600".into();
    } else if w76 & 0x0004 != 0 {
        tm = TransferMode::Sata300;
        cur = "----".into();
        max = "SATA/300".into();
    } else if w76 & 0x0002 != 0 {
        tm = TransferMode::Sata150;
        cur = "----".into();
        max = "SATA/150".into();
    }
    match (w77 & 0x000E) >> 1 {
        3 => cur = "SATA/600".into(),
        2 => cur = "SATA/300".into(),
        1 => cur = "SATA/150".into(),
        _ => {}
    }
    (tm, cur, max, kind)
}

const FORM_FACTORS: [&str; 6] = [
    "",
    "5.25 inch",
    "3.5 inch",
    "2.5 inch",
    "1.8 inch",
    "< 1.8 inch",
];

/// The raw pages read from an ATA disk. `smart_check` is a second SMART READ DATA, compared
/// with the first one as `CheckSmartAttributeCorrect` does.
#[derive(Debug, Clone, Default)]
pub struct AtaPages {
    pub identify: Vec<u8>,
    pub smart: Option<Vec<u8>>,
    pub smart_check: Option<Vec<u8>>,
    pub thresholds: Option<Vec<u8>>,
}

/// The `AddDisk` sequence for one ATA disk: identify, `FillSmartData`, `CheckSsdSupport`,
/// `FillSmartThreshold` and the firmware workarounds that follow.
pub fn analyze(pages: &AtaPages) -> Option<(AtaState, AtaInfo)> {
    let id = Identify::parse(&pages.identify)?;
    let mut asi = AtaState::new(&id.model(), &id.firmware(), &id.serial());
    let mut info = AtaInfo::default();

    let fw_int: String = asi.firmware.replace('.', "");
    if starts(&asi.model, "ADATA SSD") && fw_int.parse::<i64>().ok() == Some(346) {
        asi.temperature_multiplier = 0.5;
    }

    let (major, major_s) = major_version(id.word(80));
    asi.major = major;
    info.major_version = major_s;
    info.minor_version_code = id.word(81);
    let (tm, cur, max, kind) = transfer_mode(id.word(63), id.word(76), id.word(77), id.word(88));
    asi.transfer_mode = tm;
    info.transfer_mode_current = cur;
    info.transfer_mode_max = max;
    info.interface = kind;

    asi.detected_time_unit = derive::time_unit_type(&asi.model, &asi.firmware, major, tm);
    // `MeasuredTimeUnitType` starts as unknown; only these two units are copied at this
    // point (the main window shows the detected hours).
    if matches!(
        asi.detected_time_unit,
        TimeUnit::MilliSeconds | TimeUnit::TenMinutes
    ) {
        asi.measured_time_unit = asi.detected_time_unit;
    }

    let (w82, w83, w84, w86) = (id.word(82), id.word(83), id.word(84), id.word(86));
    let mut features = Vec::new();
    let smart_supported = major >= 3 && w82 & 1 != 0;
    if smart_supported {
        features.push("S.M.A.R.T.".to_string());
    }
    if major >= 3 && w83 & (1 << 3) != 0 {
        features.push("APM".into());
        if w86 & (1 << 3) != 0 {
            info.apm = Some((id.word(91) & 0xFF) as u8);
        }
    }
    if major >= 5 && w83 & (1 << 9) != 0 {
        features.push("AAM".into());
        if w86 & (1 << 9) != 0 {
            info.aam = Some((id.word(94) & 0xFF) as u8);
        }
    }
    let lba48 = major >= 5 && w83 & (1 << 10) != 0;
    if lba48 {
        features.push("48bit LBA".into());
    }
    if major >= 6 && id.word(76) & (1 << 8) != 0 {
        features.push("NCQ".into());
    }
    if major >= 7 && id.word(169) & 1 != 0 {
        features.push("TRIM".into());
    }
    if major >= 7 && id.word(214) & 1 != 0 {
        features.push("NV Cache".into());
    }
    if major >= 7 && w84 & (1 << 4) != 0 {
        features.push("Streaming".into());
    }
    if major >= 7 && w84 & (1 << 5) != 0 {
        features.push("GPL".into());
    }
    if major >= 7 && id.word(78) & (1 << 8) != 0 {
        features.push("DevSleep".into());
    }
    if w82 & (1 << 5) != 0 && w86 & (1 << 5) != 0 {
        features.push("VolatileWriteCache".into());
    }
    info.features = features;

    let rot = id.word(217);
    if major >= 7 && rot == 0x01 {
        asi.is_ssd = true;
        info.rotation_rate = 1;
    } else if major >= 7 && (0x401..0xFFFF).contains(&rot) {
        info.rotation_rate = rot;
    }
    let ff = (id.word(168) & 0xF) as usize;
    if major >= 7 && (1..=5).contains(&ff) {
        info.form_factor = FORM_FACTORS[ff].into();
    }

    let sector_size = id.word(106);
    let mut logical = 512u32;
    let mut physical = 512u32;
    if sector_size & 0xC000 == 0x4000 {
        if sector_size & 0x000F == 0x3 {
            physical = 4096;
        } else if sector_size & 0x1000 != 0 {
            let wps = id.dword(117);
            if wps != 256 && wps != 0 {
                logical = wps * 2;
            }
        }
    }
    info.logical_sector_size = logical;
    info.physical_sector_size = physical.max(logical);
    info.sectors = if lba48 {
        id.qword(100) & 0x0000_FFFF_FFFF_FFFF
    } else {
        (id.dword(60) & 0x0FFF_FFFF) as u64
    };
    info.buffer_size_bytes = id.word(21) as u64 * 512;

    // SMART READ DATA, vendor detection, thresholds.
    if let Some(page) = &pages.smart {
        if derive::fill_smart_data(&mut asi, page) {
            vendor::check_ssd_support(&mut asi);
            if let Some(check) = &pages.smart_check {
                let mut again = AtaState::new(&asi.model, &asi.firmware, &asi.serial);
                derive::fill_smart_data(&mut again, check);
                asi.is_smart_correct = again.attributes.len() == asi.attributes.len()
                    && again
                        .attributes
                        .iter()
                        .zip(&asi.attributes)
                        .all(|(a, b)| a.id == b.id);
            } else {
                asi.is_smart_correct = true;
            }
            if let Some(thr) = &pages.thresholds {
                asi.is_threshold_correct = derive::fill_smart_threshold(&mut asi, thr);
            }
        }
    }

    // Firmware bugs that make the thresholds meaningless.
    if (asi.vendor == Vendor::SandForce
        && ((starts(&asi.model, "OCZ-VERTEX3") && starts(&asi.firmware, "2.02"))
            || (starts(&asi.model, "OCZ-VERTEX2") && starts(&asi.firmware, "1.27"))))
        || (starts(&asi.model, "SSD G2 Series") && starts(&asi.firmware, "3.6.5"))
    {
        asi.is_threshold_bug = true;
    }

    if !asi.is_smart_correct {
        asi.detected_power_on_hours = -1;
        asi.measured_power_on_hours = -1;
        asi.power_on_count = 0;
        asi.temperature = -1000;
    }

    if starts(&asi.model, "Intel") && asi.measured_power_on_hours > 0x0DA753 {
        asi.power_on_raw = asi.power_on_raw.wrapping_sub(0x0DA753);
        asi.detected_power_on_hours -= 0x0DA753;
        asi.measured_power_on_hours -= 0x0DA753;
    }

    Some((asi, info))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build an IDENTIFY DEVICE page with the given strings and words set.
    pub fn identify_page(model: &str, fw: &str, serial: &str, words: &[(usize, u16)]) -> Vec<u8> {
        let mut w = vec![0u16; 256];
        let mut put = |from: usize, len: usize, s: &str| {
            let mut b = s.as_bytes().to_vec();
            b.resize(len * 2, b' ');
            for k in 0..len {
                w[from + k] = (b[2 * k] as u16) << 8 | b[2 * k + 1] as u16;
            }
        };
        put(10, 10, serial);
        put(23, 4, fw);
        put(27, 20, model);
        for &(i, v) in words {
            w[i] = v;
        }
        w.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn strings_are_byte_swapped() {
        let p = identify_page("Samsung SSD 870 EVO 1TB", "SVT02B6Q", "S6PUNX0R123456", &[]);
        let id = Identify::parse(&p).unwrap();
        assert_eq!(id.model(), "Samsung SSD 870 EVO 1TB");
        assert_eq!(id.firmware(), "SVT02B6Q");
        assert_eq!(id.serial(), "S6PUNX0R123456");
    }

    #[test]
    fn major_versions() {
        assert_eq!(major_version(0x0FF0).1, "ACS-4");
        assert_eq!(major_version(0x01F0).1, "ATA8-ACS");
        assert_eq!(major_version(0x007E).1, "ATA/ATAPI-6");
        assert_eq!(major_version(0xFFFF).0, 0);
    }

    #[test]
    fn sata_600_negotiated_300() {
        let (tm, cur, max, kind) = transfer_mode(0x0007, 0x000E, 0x0004, 0x007F);
        assert_eq!(tm, TransferMode::Sata600);
        assert_eq!(max, "SATA/600");
        assert_eq!(cur, "SATA/300");
        assert_eq!(kind, "Serial ATA");
    }
}
