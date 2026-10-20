//! Values derived from the ATA attributes (life, temperature, power-on hours, host and NAND
//! writes): a port of `CAtaSmart::FillSmartData`, `FillSmartThreshold`, the "Update Life" loop
//! at the end of `CheckSsdSupport`, and `GetTimeUnitType` / `GetPowerOnHours`.
//!
//! CrystalDiskInfo runs `FillSmartData` once while the vendor is still `VENDOR_UNKNOWN`, then
//! `CheckSsdSupport`, whose loop recomputes the vendor-dependent values. `analyze_ata` in
//! `mod.rs` repeats that sequence, so the results equal what CrystalDiskInfo shows right after
//! it starts (and what `DiskInfo.exe /CopyExit` writes). The two loops differ in small ways
//! (life clamping, a few vendor lists); each keeps its own rules here.

use super::types::*;
use Vendor::*;

/// `static_cast<INT>(x)`: truncate to 32 bits, as the original stores the values in an `INT`.
fn int(x: u64) -> i64 {
    x as i32 as i64
}

fn r48_512b(a: &RawAttribute) -> i64 {
    int(a.raw48() / 2 / 1024 / 1024)
}
fn r48_1mb(a: &RawAttribute) -> i64 {
    int(a.raw48() / 1024)
}
fn r48_16mb(a: &RawAttribute) -> i64 {
    int(a.raw48() / 64)
}
fn r48_32mb(a: &RawAttribute) -> i64 {
    // 65536 * 512 / 1024 / 1024 / 1024
    int(a.raw48() / 32)
}

fn is(v: Vendor, list: &[Vendor]) -> bool {
    list.contains(&v)
}

/// Vendors whose host reads/writes (0xF1/0xF2) follow `HostReadsWritesUnit` and default to GB.
const RW_BY_UNIT: &[Vendor] = &[
    SandForce,
    OczVector,
    Corsair,
    Kingston,
    Realtek,
    Wdc,
    Ssstc,
    SkHynix,
    Phison,
    Seagate,
    Marvell,
    Maxiotek,
    Ymtc,
    Scy,
    Recadata,
    MicronMu03,
    SanDiskHp,
    SanDiskHpVenus,
    SanDiskLenovo,
    SanDiskLenovoHelenVenus,
    SanDiskDell,
    AdataIndustrial,
];

/// Vendors whose life comes from 0xE7.
const LIFE_E7: &[Vendor] = &[
    SandForce,
    Corsair,
    Kingston,
    SkHynix,
    Realtek,
    SanDisk,
    Ssstc,
    Apacer,
    JMicron,
    Phison,
    Seagate,
    Maxiotek,
    Ymtc,
    Scy,
    Recadata,
    AdataIndustrial,
];

/// Which of the two loops is running; they disagree on a few details.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// `FillSmartData`.
    Fill,
    /// The "Update Life" loop of `CheckSsdSupport`.
    Update,
}

/// Life clamp: `FillSmartData` drops values `< 0`, `CheckSsdSupport` drops `<= 0` too.
fn clamp_life(life: i32, pass: Pass) -> i32 {
    let low = match pass {
        Pass::Fill => life < 0,
        Pass::Update => life <= 0,
    };
    if low || life > 100 {
        -1
    } else {
        life
    }
}

fn host_writes_f1(asi: &mut AtaState, a: &RawAttribute, pass: Pass) {
    let v = asi.vendor;
    let u = asi.host_rw_unit;
    let sandisk_gb = match pass {
        Pass::Fill => is(v, &[SanDisk, SanDiskCloud]),
        Pass::Update => v == SanDisk,
    } && u == HostRwUnit::Gb;
    if v == SsdGeneral {
        match u {
            HostRwUnit::B512 => asi.host_writes_gb = r48_512b(a),
            HostRwUnit::Mb1 => asi.host_writes_gb = r48_1mb(a),
            HostRwUnit::Mb16 => asi.host_writes_gb = r48_16mb(a),
            HostRwUnit::Mb32 => asi.host_writes_gb = r48_32mb(a),
            HostRwUnit::Gb => asi.host_writes_gb = a.raw_int() as i64,
            HostRwUnit::Unknown => {}
        }
    } else if (v == Toshiba || v == SiliconMotionCvc) && u == HostRwUnit::Gb {
        asi.host_writes_gb = a.raw_int() as i64;
    } else if v == IntelDc {
        asi.nand_writes_gb = r48_32mb(a);
    } else if is(v, &[Intel, Toshiba, Kioxia, SiliconMotion]) {
        asi.host_writes_gb = r48_32mb(a);
    } else if is(v, RW_BY_UNIT) {
        asi.host_writes_gb = match u {
            HostRwUnit::B512 => r48_512b(a),
            HostRwUnit::Mb1 => r48_1mb(a),
            HostRwUnit::Mb16 => r48_16mb(a),
            HostRwUnit::Mb32 => r48_32mb(a),
            _ => a.raw_int() as i64,
        };
    } else if v == Samsung && u == HostRwUnit::Gb {
        asi.host_writes_gb = a.raw_int() as i64;
    } else if is(v, &[Samsung, Apacer, JMicron]) {
        asi.host_writes_gb = r48_512b(a);
    } else if v == Plextor {
        asi.host_writes_gb = r48_32mb(a);
    } else if sandisk_gb {
        asi.host_writes_gb = a.raw_int() as i64;
    } else if v == SanDisk {
        asi.host_writes_gb = r48_512b(a);
    }
}

fn host_reads_f2(asi: &mut AtaState, a: &RawAttribute, pass: Pass) {
    let v = asi.vendor;
    let u = asi.host_rw_unit;
    let sandisk_gb = match pass {
        Pass::Fill => is(v, &[SanDisk, SanDiskCloud]),
        Pass::Update => v == SanDisk,
    } && u == HostRwUnit::Gb;
    if v == SsdGeneral {
        // No 1 MB case for reads in the original.
        match u {
            HostRwUnit::B512 => asi.host_reads_gb = r48_512b(a),
            HostRwUnit::Mb16 => asi.host_reads_gb = r48_16mb(a),
            HostRwUnit::Mb32 => asi.host_reads_gb = r48_32mb(a),
            HostRwUnit::Gb => asi.host_reads_gb = a.raw_int() as i64,
            _ => {}
        }
    } else if (v == Toshiba || v == SiliconMotionCvc) && u == HostRwUnit::Gb {
        asi.host_reads_gb = a.raw_int() as i64;
    } else if is(v, &[Intel, Toshiba, SiliconMotion]) {
        asi.host_reads_gb = r48_32mb(a);
    } else if is(v, RW_BY_UNIT) && v != Phison {
        // Phison is in the 0xF1 list but not in the 0xF2 one.
        asi.host_reads_gb = match u {
            HostRwUnit::B512 => r48_512b(a),
            HostRwUnit::Mb16 => r48_16mb(a),
            HostRwUnit::Mb32 => r48_32mb(a),
            _ => a.raw_int() as i64,
        };
    } else if v == Samsung && u == HostRwUnit::Gb {
        asi.host_reads_gb = a.raw_int() as i64;
    } else if is(v, &[Samsung, JMicron]) {
        asi.host_reads_gb = r48_512b(a);
    } else if v == Plextor {
        asi.host_reads_gb = r48_32mb(a);
    } else if sandisk_gb {
        asi.host_reads_gb = a.raw_int() as i64;
    } else if v == SanDisk {
        asi.host_reads_gb = r48_512b(a);
    } else if pass == Pass::Update && is(v, &[Micron, MicronMu03]) {
        asi.life = clamp_life(a.current as i32, pass);
    }
}

/// Life and wear values shared, with small differences, by both loops.
fn life_and_wear(asi: &mut AtaState, a: &RawAttribute, pass: Pass) {
    let v = asi.vendor;
    match a.id {
        0xBB if v == Mtron => asi.life = clamp_life(a.current as i32, pass),
        0xCA if is(v, &[Micron, MicronMu03, IntelDc, SiliconMotionCvc]) => {
            asi.life = clamp_life(a.current as i32, pass)
        }
        0xD1 if v == Indilinx => asi.life = clamp_life(a.current as i32, pass),
        0xC9 if is(v, &[SanDiskHp, SanDiskHpVenus]) => {
            let life = a.current as i32;
            // `<= 0 || > 100` in FillSmartData, only `<= 0` in CheckSsdSupport.
            asi.life = if life <= 0 || (pass == Pass::Fill && life > 100) {
                -1
            } else {
                life
            };
        }
        0xE6 if is(v, &[Wdc, SanDisk]) => {
            let life = if asi.flag_life_sandisk_usb_memory {
                -1
            } else if asi.flag_life_sandisk_0_1 {
                100 - (a.raw[1] as i32 * 256 + a.raw[0] as i32) / 100
            } else if asi.flag_life_sandisk_1 {
                100 - a.raw[1] as i32
            } else if asi.flag_life_sandisk_lenovo {
                a.current as i32
            } else {
                100 - a.raw[1] as i32
            };
            asi.life = if life <= 0 || (pass == Pass::Fill && life > 100) {
                -1
            } else {
                life
            };
        }
        0xE6 if is(v, &[SanDiskLenovo, SanDiskDell]) => {
            let life = a.current as i32;
            // FillSmartData clamps before assigning, so it keeps the raw current value.
            asi.life = if pass == Pass::Update && life <= 0 {
                -1
            } else {
                life
            };
        }
        0xE8 if v == Plextor => asi.life = clamp_life(a.current as i32, pass),
        0xE8 if v == Ocz => asi.host_writes_gb = r48_512b(a),
        0xE9 if is(v, &[Intel, Ocz, OczVector, SkHynix]) => {
            let life = if asi.flag_life_raw_value {
                a.raw[0] as i32
            } else {
                a.current as i32
            };
            asi.life = clamp_life(life, pass);
        }
        0xE9 if v == SanDiskLenovoHelenVenus => asi.life = clamp_life(a.current as i32, pass),
        0xE9 if v == Samsung && super::vendor::is_samsung_enterprise_model(&asi.model) => {
            // Both loops accept 0 here.
            let life = a.current as i32;
            asi.life = if !(0..=100).contains(&life) { -1 } else { life };
        }
        0xE9 if is(v, &[SanDisk, SanDiskLenovo, SanDiskCloud])
            && asi.host_rw_unit == HostRwUnit::Gb =>
        {
            asi.nand_writes_gb = if asi.nand_writes_unit == NandWritesUnit::Mb1 {
                (a.raw32() / 1024) as i64
            } else {
                a.raw_int() as i64
            };
        }
        0xE9 if is(
            v,
            &[Plextor, Kingston, Wdc, Ssstc, Seagate, SiliconMotionCvc],
        ) || (pass == Pass::Fill && v == Ymtc) =>
        {
            asi.nand_writes_gb = a.raw_int() as i64
        }
        0xE9 if is(v, &[JMicron, AdataIndustrial]) => asi.nand_writes_gb = r48_512b(a),
        0xE9 if v == Maxiotek => {
            asi.nand_writes_gb = if asi.host_rw_unit == HostRwUnit::B512 {
                r48_512b(a)
            } else {
                a.raw_int() as i64
            };
        }
        0xE1 if v == Intel => asi.host_writes_gb = r48_32mb(a),
        0xEA if is(v, &[Kingston, Seagate])
            || (v == SkHynix && asi.host_rw_unit == HostRwUnit::Gb) =>
        {
            asi.nand_writes_gb = a.raw_int() as i64
        }
        0xEB if v == IntelDc => asi.host_writes_gb = r48_32mb(a),
        0xF1 => host_writes_f1(asi, a, pass),
        0xF2 => host_reads_f2(asi, a, pass),
        0xF3 if v == Ymtc => {
            if a.raw[0] > 0 {
                asi.temperature = a.raw[0] as i32;
            }
            if asi.temperature >= 100 {
                asi.temperature = -1000;
            }
        }
        0xF3 if v == Intel => asi.nand_writes_gb = r48_32mb(a),
        0xF9 if is(
            v,
            &[
                Intel,
                Realtek,
                Wdc,
                SanDiskHp,
                SanDiskHpVenus,
                SanDiskLenovoHelenVenus,
            ],
        ) || (v == SanDisk && asi.host_rw_unit == HostRwUnit::Gb) =>
        {
            asi.nand_writes_gb = a.raw_int() as i64
        }
        // `* 16 / 1024 / 1024` and `/ 64 / 1024` give the same integer.
        0xF9 if v == OczVector => asi.nand_writes_gb = int(a.raw48() / 64 / 1024),
        0xFA if v == Realtek => asi.nand_writes_gb = a.raw_int() as i64,
        0x64 if v == SandForce => asi.gbytes_erased = a.raw_int() as i64,
        0xAD if is(v, &[Toshiba, Kioxia]) => {
            let life = a.current as i32 - 100;
            asi.life = if life <= 0 || life > 100 { -1 } else { life };
        }
        0xB1 if v == Samsung => {
            asi.wear_leveling_count = a.raw_int() as i64;
            asi.life = clamp_life(a.current as i32, pass);
        }
        0xE7 if is(v, LIFE_E7) => {
            let life = if asi.flag_life_no_report {
                -1
            } else if asi.flag_life_raw_value_increment {
                100 - a.raw[0] as i32
            } else if asi.flag_life_raw_value {
                a.raw[0] as i32
            } else {
                a.current as i32
            };
            asi.life = clamp_life(life, pass);
        }
        0xA9 if v == Realtek
            || (v == Kingston && asi.host_rw_unit == HostRwUnit::Mb32)
            || v == SiliconMotion =>
        {
            let life = if asi.flag_life_raw_value_increment {
                100 - a.raw[0] as i32
            } else if asi.flag_life_raw_value {
                a.raw[0] as i32
            } else {
                a.current as i32
            };
            asi.life = clamp_life(life, pass);
        }
        0xC6 if v == OczVector => asi.host_reads_gb = a.raw_int() as i64,
        0xC7 if v == OczVector => asi.host_writes_gb = a.raw_int() as i64,
        0xF5 if v == SanDiskCloud => asi.life = a.current as i32,
        0xF5 if v == Micron => asi.nand_writes_gb = int(a.raw48() * 8 / 1024 / 1024),
        0xF5 if v == MicronMu03
            || (v == Kingston && asi.host_rw_unit == HostRwUnit::Mb32)
            || v == SiliconMotion
            || v == Scy =>
        {
            asi.nand_writes_gb = r48_32mb(a)
        }
        0xF5 if v == Recadata => {
            asi.nand_writes_gb = match pass {
                Pass::Fill => int(a.raw48()),
                Pass::Update => a.raw_int() as i64,
            }
        }
        0xF6 if is(v, &[Micron, MicronMu03]) => asi.host_writes_gb = r48_512b(a),
        _ => {}
    }
}

/// `FillSmartData`: pack the attribute table and compute the derived values.
pub fn fill_smart_data(asi: &mut AtaState, page: &[u8]) -> bool {
    asi.smart_read_data = page.to_vec();
    asi.attributes.clear();
    for i in 0..MAX_ATTRIBUTE {
        let off = 2 + i * 12;
        if off + 12 > page.len() {
            break;
        }
        let a = RawAttribute::parse(&page[off..off + 12]);
        if a.id == 0 {
            continue;
        }
        asi.attributes.push(a);
    }
    let attrs = asi.attributes.clone();
    for a in &attrs {
        fill_one(asi, a);
    }
    asi.thresholds = vec![0; asi.attributes.len()];
    !asi.attributes.is_empty()
}

fn fill_one(asi: &mut AtaState, a: &RawAttribute) {
    match a.id {
        0x09 => {
            let mut raw = a.raw32();
            if asi.vendor == Indilinx {
                raw = a.worst as u32 * 256 + a.current as u32;
            } else if starts(&asi.model, "Intel") || starts(&asi.model, "INTEL") {
                let fw = asi.firmware.to_uppercase();
                if fw == "522ABBF0" || fw == "520I" || fw == "LB3I" {
                    raw = (a.raw48() / 60) as u32;
                } else if asi.detected_time_unit == TimeUnit::MilliSeconds || raw >= 0x0DA000 {
                    asi.measured_time_unit = TimeUnit::MilliSeconds;
                    let value = a.raw[2] as i64 * 65536 + a.raw[1] as i64 * 256 + a.raw[0] as i64
                        - 0x0DA753;
                    raw = if value < 0 { 0 } else { value as u32 };
                }
            }
            asi.power_on_raw = raw;
            asi.detected_power_on_hours = power_on_hours(raw, asi.detected_time_unit);
            asi.measured_power_on_hours = power_on_hours(raw, asi.measured_time_unit);
        }
        0x0C => {
            let raw = if asi.vendor == Indilinx {
                a.worst as u32 * 256 + a.current as u32
            } else {
                a.raw32()
            };
            asi.power_on_count = raw as i64;
        }
        0xBE => {
            if a.raw[0] > 0 && a.raw[0] < 100 {
                asi.temperature = a.raw[0] as i32;
            }
        }
        // Clean / unclean power-off counts for SanDisk/WD CloudSpeed.
        0xBF if asi.vendor == SanDiskCloud => asi.power_on_count = a.raw32() as i64 + 1,
        0xC0 if asi.vendor == SanDiskCloud => asi.power_on_count += a.raw32() as i64,
        0xC2 => {
            if starts(&asi.model, "SAMSUNG SV") && (a.raw[1] != 0 || a.raw[0] > 70) {
                asi.temperature = a.raw16() as i32 / 10;
            } else if a.raw[0] > 0 && asi.temperature_multiplier < 1.0 {
                asi.temperature = (a.raw[0] as f64 * asi.temperature_multiplier) as i32;
            } else if a.raw[0] > 0 {
                asi.temperature = a.raw[0] as i32;
            }
            if asi.temperature >= 100 {
                asi.temperature = -1000;
            }
        }
        _ => life_and_wear(asi, a, Pass::Fill),
    }
}

/// The "Update Life" loop at the end of `CheckSsdSupport`, run once the vendor is known.
pub fn check_ssd_support_update(asi: &mut AtaState) {
    let attrs = asi.attributes.clone();
    for a in &attrs {
        life_and_wear(asi, a, Pass::Update);
    }
}

/// `FillSmartThreshold`: align SMART READ THRESHOLDS entries with the attribute table.
pub fn fill_smart_threshold(asi: &mut AtaState, page: &[u8]) -> bool {
    asi.thresholds = vec![0; asi.attributes.len()];
    let mut count = 0;
    for i in 0..MAX_ATTRIBUTE {
        let off = 2 + i * 12;
        if off + 12 > page.len() {
            break;
        }
        let (id, value) = (page[off], page[off + 1]);
        if id == 0 {
            continue;
        }
        for (j, a) in asi.attributes.iter().enumerate() {
            if a.id == id {
                asi.thresholds[j] = value;
                count += 1;
            }
        }
    }
    // Some Micron firmwares put the threshold in the attribute's reserved byte instead.
    if asi.vendor == Micron && count == 0 {
        for (j, a) in asi.attributes.iter().enumerate() {
            if a.reserved > 0 {
                asi.thresholds[j] = a.reserved;
            }
        }
    }
    !asi.attributes.is_empty()
}

/// `GetPowerOnHours`.
pub fn power_on_hours(raw: u32, unit: TimeUnit) -> i64 {
    let raw = raw as i64;
    match unit {
        TimeUnit::Unknown => 0,
        TimeUnit::Hours | TimeUnit::MilliSeconds => raw,
        TimeUnit::Minutes => raw / 60,
        TimeUnit::HalfMinutes => raw / 120,
        TimeUnit::Seconds => raw / 60 / 60,
        TimeUnit::TenMinutes => raw / 6,
    }
}

/// `_tstoi`: leading integer of a string, 0 when there is none.
fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().map_or(0, |v| sign * v)
}

/// `_tstof`: leading decimal number of a string, 0.0 when there is none.
fn atof(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let mut dot = false;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')) {
            end = i + 1;
        } else if c == '.' && !dot {
            dot = true;
            end = i + 1;
        } else {
            break;
        }
    }
    s[..end].parse().unwrap_or(0.0)
}

/// `GetTimeUnitType`.
pub fn time_unit_type(model: &str, firmware: &str, major: u32, mode: TransferMode) -> TimeUnit {
    // The original upper-cases the model first, so the mixed-case "Corsair Performance Pro"
    // test below can never match. Kept as is to give the same answers.
    let model = model.to_uppercase();
    if starts(&model, "FUJITSU") {
        if major >= 8 {
            TimeUnit::Hours
        } else {
            TimeUnit::Seconds
        }
    } else if starts(&model, "HITACHI_DK") {
        TimeUnit::Minutes
    } else if starts(&model, "MAXTOR") {
        if mode >= TransferMode::Sata300
            || starts(&model, "MAXTOR 6H")
            || starts(&model, "MAXTOR 7H500")
            || starts(&model, "MAXTOR 6L0")
            || starts(&model, "MAXTOR 4K")
        {
            TimeUnit::Hours
        } else {
            TimeUnit::Minutes
        }
    } else if starts(&model, "SAMSUNG") {
        let tail: String = {
            let chars: Vec<char> = firmware.chars().collect();
            chars[chars.len().saturating_sub(3)..].iter().collect()
        };
        let n = atoi(&tail);
        if mode >= TransferMode::Sata300 {
            TimeUnit::Hours
        } else if (-39..=-23).contains(&n)
            || ["SAMSUNG SV", "SAMSUNG SP", "SAMSUNG HM", "SAMSUNG MP"]
                .iter()
                .any(|p| starts(&model, p))
        {
            TimeUnit::HalfMinutes
        } else {
            TimeUnit::Hours
        }
    } else if ((starts(&model, "CFD_CSSD-S6TM128NMPQ") || starts(&model, "CFD_CSSD-S6TM256NMPQ"))
        && (starts(firmware, "VM21") || starts(firmware, "VN21")))
        || ((has(&model, "PX-128M2P") || has(&model, "PX-256M2P")) && atof(firmware) < 1.059)
        || (starts(&model, "Corsair Performance Pro") && atof(firmware) < 1.059)
    {
        TimeUnit::TenMinutes
    } else if (starts(&model, "INTEL SSDSC2CW") && has_after_start(&model, "A3"))
        || (starts(&model, "INTEL SSDSC2BW") && has_after_start(&model, "A3"))
        || (starts(&model, "INTEL SSDSC2CT") && has_after_start(&model, "A3"))
    {
        TimeUnit::MilliSeconds
    } else {
        TimeUnit::Hours
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr(id: u8, current: u8, raw: u64) -> RawAttribute {
        let b = raw.to_le_bytes();
        RawAttribute {
            id,
            current,
            worst: current,
            raw: [b[0], b[1], b[2], b[3], b[4], b[5]],
            ..Default::default()
        }
    }

    #[test]
    fn units_convert_like_the_original() {
        let a = attr(0xF1, 100, 2 * 1024 * 1024 * 10); // 10 GiB in 512-byte sectors
        assert_eq!(r48_512b(&a), 10);
        let b = attr(0xF1, 100, 320); // 320 x 32 MiB = 10 GiB
        assert_eq!(r48_32mb(&b), 10);
    }

    #[test]
    fn temperature_from_c2_and_be() {
        let mut s = AtaState::new("WDC WD40EFRX", "82.00A82", "X");
        s.detected_time_unit = TimeUnit::Hours;
        s.measured_time_unit = TimeUnit::Hours;
        let mut page = vec![0u8; 512];
        let a = attr(0xC2, 115, 0x0000_2E00_0F00_0024); // 36 C, min/max in upper bytes
        page[2] = a.id;
        page[5] = a.current;
        page[7..13].copy_from_slice(&a.raw);
        fill_smart_data(&mut s, &page);
        assert_eq!(s.temperature, 36);
    }

    #[test]
    fn power_on_hours_units() {
        assert_eq!(power_on_hours(7200, TimeUnit::Minutes), 120);
        assert_eq!(power_on_hours(7200, TimeUnit::HalfMinutes), 60);
        assert_eq!(power_on_hours(600, TimeUnit::TenMinutes), 100);
        assert_eq!(power_on_hours(5, TimeUnit::Unknown), 0);
    }

    #[test]
    fn samsung_old_firmware_half_minutes() {
        assert_eq!(
            time_unit_type("SAMSUNG HD080HJ", "WT100-33", 7, TransferMode::Sata150),
            TimeUnit::HalfMinutes
        );
        assert_eq!(
            time_unit_type("SAMSUNG HD103UJ", "1AA01113", 8, TransferMode::Sata300),
            TimeUnit::Hours
        );
    }

    #[test]
    fn corsair_performance_pro_never_matches_after_uppercasing() {
        assert_eq!(
            time_unit_type("Corsair Performance Pro", "1.0", 8, TransferMode::Sata600),
            TimeUnit::Hours
        );
    }
}
