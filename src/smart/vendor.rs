//! Vendor detection for ATA disks: a port of `CAtaSmart::CheckSsdSupport` and the `IsSsd*`
//! helpers of CrystalDiskInfo 9.9.3 (`AtaSmart.cpp`).
//!
//! The order of the tests matters (the first match wins, and several helpers set flags or
//! units as a side effect even when they end up returning false), so it is kept verbatim.

use super::types::*;

/// Classify the disk and set its attribute table, units and life flags.
/// Returns early, like the original, for the generic SSD and HDD cases.
pub fn check_ssd_support(asi: &mut AtaState) {
    if is_ssd_old(asi) {
        asi.is_ssd = true;
    }

    let (vendor, key): (Vendor, Option<&'static str>) = if !asi.is_ssd {
        (Vendor::HddGeneral, Some("Smart"))
    } else if is_ssd_adata_industrial(asi) {
        (Vendor::AdataIndustrial, Some("SmartAdataIndustrial"))
    } else if is_ssd_sandisk(asi) {
        // Sets vendor and table itself.
        (asi.vendor, None)
    } else if is_ssd_wdc(asi) {
        (Vendor::Wdc, Some("SmartWdc"))
    } else if is_ssd_seagate(asi) {
        (asi.vendor, None)
    } else if is_ssd_mtron(asi) {
        (Vendor::Mtron, Some("SmartMtron"))
    } else if is_ssd_toshiba(asi) {
        (Vendor::Toshiba, Some("SmartToshiba"))
    } else if is_ssd_jmicron_66x(asi) {
        (Vendor::JMicron, Some("SmartJMicron66x"))
    } else if is_ssd_jmicron_61x(asi) {
        (Vendor::JMicron, Some("SmartJMicron61x"))
    } else if is_ssd_jmicron_60x(asi) {
        asi.is_raw_values8 = true;
        (Vendor::JMicron, Some("SmartJMicron60x"))
    } else if is_ssd_indilinx(asi) {
        asi.is_raw_values8 = true;
        (Vendor::Indilinx, Some("SmartIndilinx"))
    } else if is_ssd_intel_dc(asi) {
        (Vendor::IntelDc, Some("SmartIntelDc"))
    } else if is_ssd_intel(asi) {
        (Vendor::Intel, Some("SmartIntel"))
    } else if is_ssd_samsung(asi) {
        (Vendor::Samsung, Some("SmartSamsung"))
    } else if is_ssd_micron_mu03(asi) {
        (Vendor::MicronMu03, Some("SmartMicronMU03"))
    } else if is_ssd_micron(asi) {
        (Vendor::Micron, Some("SmartMicron"))
    } else if is_ssd_sandforce(asi) {
        asi.is_raw_values7 = true;
        (Vendor::SandForce, Some("SmartSandForce"))
    } else if is_ssd_ocz(asi) {
        (Vendor::Ocz, Some("SmartOcz"))
    } else if is_ssd_ocz_vector(asi) {
        (Vendor::OczVector, Some("SmartOczVector"))
    } else if is_ssd_ssstc(asi) {
        (Vendor::Ssstc, Some("SmartSsstc"))
    } else if is_ssd_plextor(asi) {
        (Vendor::Plextor, Some("SmartPlextor"))
    } else if is_ssd_kingston(asi) {
        (Vendor::Kingston, None)
    } else if is_ssd_corsair(asi) {
        (Vendor::Corsair, Some("SmartCorsair"))
    } else if is_ssd_realtek(asi) {
        (Vendor::Realtek, Some("SmartRealtek"))
    } else if is_ssd_skhynix(asi) {
        (Vendor::SkHynix, Some("SmartSKhynix"))
    } else if is_ssd_kioxia(asi) {
        (Vendor::Kioxia, Some("SmartKioxia"))
    } else if is_ssd_silicon_motion_cvc(asi) {
        (Vendor::SiliconMotionCvc, Some("SmartSiliconMotionCVC"))
    } else if is_ssd_silicon_motion(asi) {
        (Vendor::SiliconMotion, Some("SmartSiliconMotion"))
    } else if is_ssd_phison(asi) {
        (Vendor::Phison, Some("SmartPhison"))
    } else if is_ssd_marvell(asi) {
        (Vendor::Marvell, Some("SmartMarvell"))
    } else if is_ssd_maxiotek(asi) {
        (Vendor::Maxiotek, Some("SmartMaxiotek"))
    } else if is_ssd_apacer(asi) {
        (Vendor::Apacer, Some("SmartApacer"))
    } else if is_ssd_ymtc(asi) {
        (Vendor::Ymtc, Some("SmartYmtc"))
    } else if is_ssd_scy(asi) {
        (Vendor::Scy, Some("SmartScy"))
    } else if is_ssd_recadata(asi) {
        (Vendor::Recadata, Some("SmartRecadata"))
    } else if is_ssd_general(asi) {
        asi.vendor = Vendor::SsdGeneral;
        asi.smart_key = "SmartSsd";
        return;
    } else {
        asi.vendor = Vendor::HddGeneral;
        asi.smart_key = "Smart";
        return;
    };

    asi.vendor = vendor;
    if let Some(key) = key {
        asi.smart_key = key;
    }
    // The HDD branch also falls through to the "Update Life" loop in the original.
    super::derive::check_ssd_support_update(asi);
}

fn upper(s: &str) -> String {
    s.to_uppercase()
}

fn is_ssd_old(asi: &AtaState) -> bool {
    let m = upper(&asi.model);
    starts(&m, "OCZ")
        || starts(&m, "SPCC")
        || starts(&m, "PATRIOT")
        || has(&m, "SOLID")
        || has(&m, "SSD")
        || has(&m, "SILICONHARDDISK")
        || starts(&m, "PHOTOFAST")
        || starts(&m, "STT_FTM")
        || starts(&m, "SUPER TALENT")
        || starts(&m, "SANDFORCE")
        || starts(&m, "HANYE")
        || starts(&m, "INTEL")
        || starts(&m, "TOSHIBA THNS")
        || starts(&m, "CSSD")
}

fn is_ssd_mtron(asi: &AtaState) -> bool {
    (asi.id(0) == 0xBB && asi.attributes.len() == 1) || starts(&asi.model, "MTRON")
}

fn is_ssd_jmicron_60x(asi: &AtaState) -> bool {
    asi.ids_start_with(&[0x0C, 0x09, 0xC2, 0xE5, 0xE8, 0xE9])
}

fn is_ssd_jmicron_61x(asi: &AtaState) -> bool {
    asi.ids_start_with(&[
        0x01, 0x02, 0x03, 0x05, 0x07, 0x08, 0x09, 0x0A, 0x0C, 0xA8, 0xAF, 0xC0, 0xC2,
    ])
}

fn is_ssd_jmicron_66x(asi: &AtaState) -> bool {
    asi.ids_start_with(&[
        0x01, 0x02, 0x03, 0x05, 0x07, 0x08, 0x09, 0x0A, 0x0C, 0xA7, 0xA8, 0xA9, 0xAA, 0xAD, 0xAF,
    ]) || starts(&asi.model, "ADATA SU700")
}

fn is_ssd_indilinx(asi: &AtaState) -> bool {
    asi.ids_start_with(&[0x01, 0x09, 0x0C, 0xB8, 0xC3, 0xC4])
}

fn is_ssd_intel_dc(asi: &AtaState) -> bool {
    has(&asi.model, "INTEL SSDSCKHB")
}

fn is_ssd_intel(asi: &AtaState) -> bool {
    let m = upper(&asi.model);
    let mut flag = false;
    if asi.ids_start_with(&[0x03, 0x04, 0x05, 0x09, 0x0C]) {
        flag = (asi.id(5) == 0xC0 && asi.id(6) == 0xE8 && asi.id(7) == 0xE9)
            || (asi.id(5) == 0xC0 && asi.id(6) == 0xE1)
            || (asi.id(5) == 0xAA && asi.id(6) == 0xAB && asi.id(7) == 0xAC);
    }
    has(&m, "INTEL") || has(&m, "SOLIDIGM") || flag
}

/// `CAtaSmart::IsSamsungEnterpriseModel`.
pub fn is_samsung_enterprise_model(model: &str) -> bool {
    let mu = upper(model);
    [
        "SM863", "PM863", "PM863A", "SM883", "PM883", "SM843T", "PM853T",
    ]
    .iter()
    .any(|s| has(&mu, s))
        || ["MZ7KM", "MZ7KH", "MZ7LM", "MZ7LH"]
            .iter()
            .any(|s| has(&mu, s))
}

fn is_ssd_samsung(asi: &mut AtaState) -> bool {
    let mut flag = false;
    if asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xB2, 0xB4]) {
        flag = true;
        asi.host_rw_unit = HostRwUnit::Gb;
    } else if asi.ids_start_with(&[0x09, 0x0C, 0xB2, 0xB3, 0xB4])
        || asi.ids_start_with(&[0x09, 0x0C, 0xB1, 0xB2, 0xB3, 0xB4, 0xB7])
        || asi.ids_start_with(&[0x09, 0x0C, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4])
        || asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xB1, 0xB3, 0xB5, 0xB6])
    {
        flag = true;
    }
    (has(&asi.model, "SAMSUNG") && asi.is_ssd) || (has(&asi.model, "MZ-") && asi.is_ssd) || flag
}

fn is_ssd_sandforce(asi: &AtaState) -> bool {
    let flag = asi.ids_start_with(&[0x01, 0x05, 0x09, 0x0C, 0x0D, 0x64, 0xAA])
        || asi.ids_start_with(&[0x01, 0x05, 0x09, 0x0C, 0xAB, 0xAC])
        || asi.ids_start_with(&[
            0x01, 0x02, 0x03, 0x05, 0x07, 0x08, 0x09, 0x0A, 0x0C, 0xA7, 0xA8, 0xA9, 0xAA, 0xAD,
            0xAF, 0xB1,
        ]);
    has(&asi.model, "SandForce") || flag
}

fn is_ssd_micron_mu03(asi: &mut AtaState) -> bool {
    let m = upper(&asi.model);
    let first = [
        "MICRON_M600",
        "MICRON M600",
        "MICRON_M550",
        "MICRON M550",
        "MICRON_M510",
        "MICRON M510",
        "MICRON_M500",
        "MICRON M500",
        "MICRON_1300",
        "MICRON 1300",
        "MICRON_1100",
        "MICRON 1100",
        "MTFDDA",
    ];
    if first.iter().any(|s| starts(&m, s)) {
        asi.host_rw_unit = HostRwUnit::B512;
        return true;
    }
    let second = [
        "M500SSD", "MX500SSD", "BX500SSD", "MX300SSD", "BX300SSD", "MX200SSD", "BX200SSD",
        "MX100SSD", "BX100SSD",
    ];
    if (second.iter().any(|s| has(&m, s)) || starts(&m, "MTFD")) && !has(&asi.firmware, "MU01") {
        asi.host_rw_unit = HostRwUnit::Mb32;
        return true;
    }
    false
}

fn is_ssd_micron(asi: &AtaState) -> bool {
    let m = upper(&asi.model);
    let flag = asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xB5, 0xB7,
    ]);
    let prefixes = [
        "P600", "C600", "M6-", "M600", "P500", "M5-", "M500", "P400", "C400", "M4-", "M400",
        "P300", "C300", "M3-", "M300", "CRUCIAL", "MICRON", "MTFD",
    ];
    prefixes.iter().any(|s| starts(&m, s))
        // Workaround for Maxiotek C500: firmware starting with "H" is not a Micron.
        || (starts(&m, "C500") && !starts(&asi.firmware, "H"))
        || (starts(&m, "CT") && has(&m, "SSD"))
        || flag
}

fn is_ssd_ocz(asi: &AtaState) -> bool {
    let m = upper(&asi.model);
    let flag = starts(&m, "OCZ-TRION")
        || asi.ids_start_with(&[0x01, 0x03, 0x04, 0x05, 0x09, 0x0C, 0xE8, 0xE9]);
    starts(&m, "OCZ") && flag
}

fn is_ssd_ocz_vector(asi: &AtaState) -> bool {
    let m = upper(&asi.model);
    if starts(&m, "RADEON R7") {
        return true;
    }
    let flag = asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xAB, 0xAE, 0xC3, 0xC4, 0xC5, 0xC6])
        || starts(&m, "PANASONIC RP-SSB");
    starts(&m, "OCZ") || flag
}

fn is_ssd_ssstc(asi: &AtaState) -> bool {
    has(&asi.model, "CV8-") || has(&asi.model, "CVB-") || has(&asi.model, "ER2-")
}

fn is_ssd_plextor(asi: &AtaState) -> bool {
    let flag = asi.ids_start_with(&[0x01, 0x05, 0x09, 0x0C, 0xB1, 0xB2, 0xB5, 0xB6]);
    let m = &asi.model;
    starts(m, "PLEXTOR")
        || starts(m, "LITEON")
        || starts(m, "CV6-CQ")
        || starts(m, "CSSD-S6T128NM3PQ")
        || starts(m, "CSSD-S6T256NM3PQ")
        || flag
}

fn is_ssd_sandisk(asi: &mut AtaState) -> bool {
    let m = upper(&asi.model);
    if !(has(&m, "SANDISK") || has(&m, "SD ULTRA") || has(&m, "SDLF1")) {
        return false;
    }
    asi.vendor = Vendor::SanDisk; // Default vendor id for SanDisk.
    if (has(&m, "X600") && has(&m, "2280"))
        || has(&m, "X400")
        || has(&m, "X300")
        || has(&m, "X110")
        || has(&m, "SD5")
    {
        asi.smart_key = if asi.id(2) == 0xAF || asi.id(3) == 0xAF {
            "SmartSanDiskDell"
        } else {
            "SmartSanDiskGb"
        };
        asi.flag_life_sandisk_1 = true;
        asi.host_rw_unit = HostRwUnit::B512;
    } else if has(&m, "Z400") {
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.smart_key = "SmartSanDiskDell";
    } else if has_after_start(&m, "1006") {
        // HP OEM.
        asi.host_rw_unit = HostRwUnit::Mb16;
        if has_after_start(&m, "8U") {
            asi.vendor = Vendor::SanDiskHpVenus;
            asi.smart_key = "SmartSanDiskHpVenus";
        } else {
            asi.vendor = Vendor::SanDiskHp;
            asi.smart_key = "SmartSanDiskHp";
        }
    } else if has(&m, "G1001") {
        // Lenovo OEM.
        asi.flag_life_sandisk_lenovo = true;
        asi.host_rw_unit = HostRwUnit::Gb;
        if has_after_start(&m, "6S") || has_after_start(&m, "7S") || has_after_start(&m, "8U") {
            asi.vendor = Vendor::SanDiskLenovoHelenVenus;
            asi.smart_key = "SmartSanDiskLenovoHelenVenus";
        } else if has(&m, "SD9SB") {
            asi.host_rw_unit = HostRwUnit::Gb;
            asi.nand_writes_unit = NandWritesUnit::Mb1;
            asi.smart_key = "SmartSanDiskGb";
        } else {
            asi.vendor = Vendor::SanDiskLenovo;
            asi.smart_key = "SmartSanDiskLenovo";
        }
    } else if has(&m, "G1012") || has(&m, "Z400S 2.5") {
        // Dell OEM.
        asi.vendor = Vendor::SanDiskDell;
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.smart_key = "SmartSanDiskDell";
    } else if has(&m, "SSD P4") {
        asi.host_rw_unit = HostRwUnit::B512;
        asi.flag_life_sandisk_usb_memory = true; // No life report.
        asi.smart_key = "SmartSanDisk";
    } else if has(&m, "ISSD P4") {
        asi.host_rw_unit = HostRwUnit::B512;
        asi.smart_key = "SmartSanDiskGb";
    } else if has(&m, "SDSSDP") || has(&m, "SDSSDRC") {
        asi.flag_life_sandisk_0_1 = true;
        asi.host_rw_unit = HostRwUnit::B512;
        asi.smart_key = "SmartSanDisk";
    } else if ["SSD U100", "SSD U110", "SSD I100", "SSD I110", "PSSD"]
        .iter()
        .any(|s| has(&m, s))
    {
        asi.flag_life_sandisk_usb_memory = true;
        asi.host_rw_unit = HostRwUnit::B512;
        asi.smart_key = "SmartSanDisk";
    } else if ["SDLF1CRR-", "SDLF1DAR-", "SDLF1CRM-", "SDLF1DAM-"]
        .iter()
        .any(|s| has(&m, s))
    {
        asi.vendor = Vendor::SanDiskCloud;
        asi.flag_life_sandisk_cloud = true;
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.smart_key = "SmartSanDiskCloud";
    } else {
        asi.flag_life_sandisk_1 = true;
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.smart_key = "SmartSanDiskGb";
    }
    true
}

fn is_ssd_kingston(asi: &mut AtaState) -> bool {
    let m = asi.model.clone();
    if !has(&m, "KINGSTON") {
        return false;
    }
    if ["SM2280", "SEDC400", "SKC310", "SHSS", "SUV300", "SKC400"]
        .iter()
        .any(|s| has(&m, s))
    {
        asi.smart_key = "SmartKingston";
        asi.host_rw_unit = HostRwUnit::Gb;
        true
    } else if has(&m, "SA400") {
        // Firmware 03070009 reports life in the normalised value; SBFK62C3 and the rest in
        // the raw value.
        asi.flag_life_raw_value = !starts(&asi.firmware, "03070009");
        asi.smart_key = "SmartKingstonSA400";
        asi.host_rw_unit = HostRwUnit::Gb;
        true
    } else if has(&m, "KC600") {
        asi.smart_key = "SmartKingstonKC600";
        asi.host_rw_unit = HostRwUnit::Mb32;
        true
    } else if has(&m, "DC500") {
        asi.smart_key = "SmartKingstonDC500";
        asi.host_rw_unit = HostRwUnit::Gb;
        true
    } else if has(&m, "SUV400") || has(&m, "SUV500") {
        asi.smart_key = "SmartKingstonSuv";
        asi.host_rw_unit = HostRwUnit::Gb;
        true
    } else {
        false
    }
}

fn is_ssd_corsair(asi: &mut AtaState) -> bool {
    if !starts(&asi.model, "Corsair") {
        return false;
    }
    if has(&asi.model, "Voyager GTX") {
        asi.host_rw_unit = HostRwUnit::Mb1;
    }
    true
}

fn is_ssd_toshiba(asi: &mut AtaState) -> bool {
    if !(has(&upper(&asi.model), "TOSHIBA") && asi.is_ssd) {
        return false;
    }
    let m = &asi.model;
    asi.host_rw_unit = if [
        "THNSNC", "THNSNJ", "THNSNK", "KSG60", "TL100", "TR150", "TR200",
    ]
    .iter()
    .any(|s| has(m, s))
    {
        HostRwUnit::Mb32
    } else {
        HostRwUnit::Gb
    };
    true
}

fn is_ssd_realtek(asi: &mut AtaState) -> bool {
    if asi.ids_start_with(&[0x01, 0x05, 0x09, 0x0C, 0xA1, 0xA2, 0xA3, 0xA4, 0xA6, 0xA7]) {
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.smart_key = "SmartRealtek";
        return true;
    }
    false
}

fn is_ssd_skhynix(asi: &mut AtaState) -> bool {
    let m = asi.model.clone();
    let mut flag = false;
    if has(&m, "SK hynix") || starts(&m, "HFS") || starts(&m, "SHG") {
        flag = true;
        asi.smart_key = "SmartSKhynix";
    }
    // These run whether or not the model matched, as in the original.
    if (has(&m, "HFS") && has(&m, "TND")) || (has(&m, "HFS") && has(&m, "MND")) {
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.flag_life_raw_value_increment = true;
    } else if has(&m, "HFS") && has(&m, "TNF") {
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.flag_life_raw_value = true;
    } else if has(&m, "SC311") || has(&m, "SC401") {
        asi.host_rw_unit = HostRwUnit::B512;
        asi.flag_life_raw_value = true;
    } else {
        asi.host_rw_unit = HostRwUnit::Gb;
    }
    flag
}

fn is_ssd_kioxia(asi: &mut AtaState) -> bool {
    if has(&asi.model, "KIOXIA") {
        asi.host_rw_unit = HostRwUnit::Mb32;
        asi.smart_key = "SmartKioxia";
        return true;
    }
    false
}

fn is_ssd_apacer(asi: &mut AtaState) -> bool {
    if starts(&asi.model, "Apacer")
        || starts(&asi.model, "ZADAK")
        || starts(&asi.firmware, "AP")
        || starts(&asi.firmware, "SF")
        || starts(&asi.firmware, "PN")
    {
        asi.host_rw_unit = HostRwUnit::B512;
        asi.flag_life_raw_value = true;
        asi.smart_key = "SmartApacer";
        return true;
    }
    false
}

fn is_ssd_ymtc(asi: &mut AtaState) -> bool {
    if has(&asi.model, "ZHITAI") {
        asi.host_rw_unit = HostRwUnit::B512;
        asi.smart_key = "SmartYmtc";
        return true;
    }
    false
}

fn is_ssd_scy(asi: &mut AtaState) -> bool {
    if starts(&asi.model, "SCY") {
        asi.host_rw_unit = HostRwUnit::Mb32;
        asi.smart_key = "SmartScy";
        return true;
    }
    false
}

fn is_ssd_recadata(asi: &mut AtaState) -> bool {
    if starts(&asi.model, "RECADATA") {
        asi.host_rw_unit = HostRwUnit::Gb;
        return true;
    }
    false
}

fn is_ssd_silicon_motion_cvc(asi: &mut AtaState) -> bool {
    if has(&asi.model, "CVC-") {
        asi.host_rw_unit = HostRwUnit::Gb;
        return true;
    }
    false
}

fn is_ssd_silicon_motion(asi: &mut AtaState) -> bool {
    let srd = |i: usize| asi.smart_read_data.get(i).copied().unwrap_or(0);
    let flag = if asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0xA0, 0xA1, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAF, 0xB0,
        0xB1, 0xB2, 0xB5, 0xB6, 0xC0,
    ]) || asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0xA0, 0xA1, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0x94, 0x95, 0x96, 0x97,
        0xA9, 0xB1, 0xB5, 0xB6, 0xBB,
    ]) || asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0x94, 0x95, 0x96, 0x97, 0x9F, 0xA0, 0xA1,
    ]) || asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0xA0, 0xA1, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    ]) || asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0xA0, 0xA1, 0xA3, 0x94, 0x95, 0x96, 0x97,
    ]) {
        true
    } else if starts(&asi.model, "TS") {
        // Transcend: the controller signs the vendor area of the SMART page.
        (srd(400) == b'T' && srd(401) == b'S') || (srd(400) == b'S' && srd(401) == b'M')
    } else {
        starts(&asi.model, "ADATA SX950")
    };

    if flag {
        let m = &asi.model;
        let normalised_life = (starts(m, "SSD") && starts(&asi.firmware, "FW"))
            || starts(m, "WT200")
            || starts(m, "WT100")
            || starts(m, "WT ")
            || starts(m, "tecmiyo")
            || (starts(m, "ADATA SU650") && starts(&asi.firmware, "XD0R3C0A"));
        if !normalised_life {
            asi.flag_life_raw_value = true;
        }
    }
    flag
}

fn is_ssd_phison(asi: &mut AtaState) -> bool {
    let flag = asi.ids_start_with(&[
        0x01, 0x09, 0x0C, 0xA8, 0xAA, 0xAD, 0xC0, 0xC2, 0xDA, 0xE7, 0xF1,
    ]) || asi
        .ids_start_with(&[0x01, 0x09, 0x0C, 0xA8, 0xAA, 0xAD, 0xC0, 0xDA, 0xE7, 0xF1]);
    if flag {
        asi.flag_life_raw_value = !starts(&asi.model, "aigo PSSD P6");
        asi.host_rw_unit = if starts(&asi.firmware, "S9") {
            HostRwUnit::Mb1
        } else {
            HostRwUnit::Gb
        };
    }
    flag
}

fn is_ssd_wdc(asi: &mut AtaState) -> bool {
    if starts(&asi.model, "WDC ") || starts(&asi.model, "WD ") {
        asi.host_rw_unit = if has(&asi.model, "SA530") {
            HostRwUnit::Mb16
        } else {
            HostRwUnit::Gb
        };
        return true;
    }
    false
}

fn is_ssd_seagate(asi: &mut AtaState) -> bool {
    if asi.ids_start_with(&[
        0x01, 0x05, 0x09, 0x0C, 0x64, 0x66, 0x67, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xB1, 0xB7, 0xBB,
    ]) {
        asi.smart_key = "SmartSeagateIronWolf";
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.vendor = Vendor::Seagate;
        true
    } else if asi.ids_start_with(&[
        0x01, 0x09, 0x0C, 0x10, 0x11, 0xA8, 0xAA, 0xAD, 0xAE, 0xB1, 0xC0, 0xC2, 0xDA, 0xE7, 0xE8,
        0xE9, 0xEB, 0xF1, 0xF2,
    ]) {
        asi.smart_key = "SmartSeagate";
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.flag_life_raw_value = true;
        asi.vendor = Vendor::Seagate;
        true
    } else if starts(&asi.model, "Seagate")
        || (!starts(&asi.model, "STT") && starts(&asi.model, "ST"))
        || starts(&asi.model, "ZA")
    {
        if has(&asi.model, "BarraCuda") {
            asi.smart_key = "SmartSeagateBarraCuda";
            asi.flag_life_raw_value = true;
        } else if has(&asi.model, "HM") || has(&asi.model, "FP") {
            asi.smart_key = "SmartSeagate";
            asi.flag_life_raw_value = false;
        } else {
            asi.smart_key = "SmartSeagate";
            asi.flag_life_raw_value = true;
        }
        asi.host_rw_unit = HostRwUnit::Gb;
        asi.vendor = Vendor::Seagate;
        true
    } else {
        false
    }
}

fn is_ssd_marvell(asi: &mut AtaState) -> bool {
    let flag = asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xA1, 0xA4, 0xA5, 0xA6, 0xA7])
        || asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xA4, 0xA5, 0xA6, 0xA7]);
    if flag {
        asi.host_rw_unit = HostRwUnit::Gb;
    }
    let m = upper(&asi.model);
    if starts(&m, "LEXAR") && (starts(&asi.firmware, "SN0") || starts(&asi.firmware, "V6")) {
        asi.host_rw_unit = HostRwUnit::Mb32;
    }
    if starts(&m, "HANYE-Q55") {
        return false;
    }
    flag
}

fn is_ssd_maxiotek(asi: &mut AtaState) -> bool {
    let m = upper(&asi.model);
    if starts(&m, "MAXIO") {
        asi.host_rw_unit = HostRwUnit::Gb;
        true
    } else if starts(&m, "CUSO C5S-EVO") {
        asi.host_rw_unit = HostRwUnit::B512;
        true
    } else {
        (starts(&m, "HANYE-Q55") && asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xA4, 0xA5, 0xA6, 0xA7]))
            || asi.ids_start_with(&[0x05, 0x09, 0x0C, 0xA7, 0xA8, 0xA9])
    }
}

fn is_ssd_adata_industrial(asi: &mut AtaState) -> bool {
    let m = upper(&asi.model);
    if [
        "ADATA_IM2S",
        "ADATA_IMSS",
        "ADATA_ISSS",
        "IM2S",
        "IMSS",
        "ISSS",
    ]
    .iter()
    .any(|s| starts(&m, s))
    {
        asi.host_rw_unit = HostRwUnit::B512;
        return true;
    }
    false
}

fn is_ssd_general(asi: &mut AtaState) -> bool {
    asi.host_rw_unit = HostRwUnit::Unknown;
    if starts(&asi.model, "ADATA SP580") {
        asi.host_rw_unit = HostRwUnit::B512;
    } else if starts(&asi.model, "LITEON IT LMT") {
        asi.host_rw_unit = HostRwUnit::Mb32;
    } else if starts(&asi.model, "LITEON S960") {
        asi.host_rw_unit = HostRwUnit::Gb;
    }
    asi.is_ssd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_ids(model: &str, fw: &str, ssd: bool, ids: &[u8]) -> AtaState {
        let mut a = AtaState::new(model, fw, "S");
        a.is_ssd = ssd;
        a.attributes = ids
            .iter()
            .map(|&id| RawAttribute {
                id,
                current: 100,
                worst: 100,
                ..Default::default()
            })
            .collect();
        a.thresholds = vec![0; ids.len()];
        a
    }

    #[test]
    fn hdd_stays_generic() {
        let mut a = with_ids(
            "WDC WD40EFRX-68N32N0",
            "82.00A82",
            false,
            &[1, 3, 4, 5, 7, 9],
        );
        check_ssd_support(&mut a);
        assert_eq!(a.vendor, Vendor::HddGeneral);
        assert_eq!(a.smart_key, "Smart");
    }

    #[test]
    fn samsung_860_evo_signature() {
        let mut a = with_ids(
            "Samsung SSD 860 EVO 500GB",
            "RVT04B6Q",
            true,
            &[
                0x05, 0x09, 0x0C, 0xB1, 0xB3, 0xB5, 0xB6, 0xB7, 0xBB, 0xBE, 0xC3, 0xC7, 0xEB, 0xF1,
            ],
        );
        check_ssd_support(&mut a);
        assert_eq!(a.vendor, Vendor::Samsung);
        assert_eq!(a.smart_key, "SmartSamsung");
    }

    #[test]
    fn crucial_mx500_is_micron_mu03_with_32mb_units() {
        let mut a = with_ids("CT500MX500SSD1", "M3CR046", true, &[0x01, 0x05, 0x09, 0x0C]);
        check_ssd_support(&mut a);
        assert_eq!(a.vendor, Vendor::MicronMu03);
        assert_eq!(a.host_rw_unit, HostRwUnit::Mb32);
    }

    #[test]
    fn kingston_sa400_life_flag_depends_on_firmware() {
        let mut a = with_ids("KINGSTON SA400S37240G", "03070009", true, &[1, 9, 12]);
        check_ssd_support(&mut a);
        assert_eq!(a.smart_key, "SmartKingstonSA400");
        assert!(!a.flag_life_raw_value);
        let mut b = with_ids("KINGSTON SA400S37240G", "SBFK71E0", true, &[1, 9, 12]);
        check_ssd_support(&mut b);
        assert!(b.flag_life_raw_value);
    }

    #[test]
    fn model_ssd_marks_old_ssd() {
        let mut a = with_ids("Generic SSD 128GB", "1.0", false, &[1, 9, 12]);
        check_ssd_support(&mut a);
        assert!(a.is_ssd);
        assert_eq!(a.vendor, Vendor::SsdGeneral);
        assert_eq!(a.smart_key, "SmartSsd");
    }

    #[test]
    fn find_greater_than_zero_semantics() {
        assert!(!has_after_start("1006X", "1006"));
        assert!(has_after_start("SDSA1006", "1006"));
    }
}
