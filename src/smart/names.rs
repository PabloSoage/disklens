//! Attribute names per vendor table, from CrystalDiskInfo's `English.lang` (see
//! `data/smart_attributes.ini`). Lookup works like `UpdateListCtrl`: the key is the id in
//! two upper-case hex digits inside the disk's table, and a missing key reads
//! "Vendor Specific". There is no fallback to the generic `[Smart]` table.

use std::collections::HashMap;
use std::sync::OnceLock;

const DATA: &str = include_str!("../../data/smart_attributes.ini");

type Tables = HashMap<String, HashMap<String, String>>;

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut t: Tables = HashMap::new();
        let mut section = String::new();
        for line in DATA.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_string();
                t.entry(section.clone()).or_default();
            } else if let Some((k, v)) = line.split_once('=') {
                t.entry(section.clone())
                    .or_default()
                    .insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        t
    })
}

pub fn vendor_specific() -> &'static str {
    tables()
        .get("Smart")
        .and_then(|s| s.get("VENDOR_SPECIFIC"))
        .map_or("Vendor Specific", |s| s.as_str())
}

pub fn attribute_name(table: &str, id: u8) -> String {
    tables()
        .get(table)
        .and_then(|s| s.get(&format!("{id:02X}")))
        .cloned()
        .unwrap_or_else(|| vendor_specific().to_string())
}

pub fn table_names() -> Vec<&'static str> {
    let mut v: Vec<&str> = tables().keys().map(|s| s.as_str()).collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names() {
        assert_eq!(attribute_name("Smart", 0x05), "Reallocated Sectors Count");
        assert_eq!(attribute_name("SmartNVMe", 0x05), "Percentage Used");
        assert_eq!(attribute_name("Smart", 0x77), "Vendor Specific");
    }

    #[test]
    fn every_vendor_table_used_by_the_detection_exists() {
        for t in [
            "Smart",
            "SmartSsd",
            "SmartMtron",
            "SmartIndilinx",
            "SmartJMicron60x",
            "SmartJMicron61x",
            "SmartJMicron66x",
            "SmartIntel",
            "SmartIntelDc",
            "SmartSamsung",
            "SmartSandForce",
            "SmartMicron",
            "SmartMicronMU03",
            "SmartOcz",
            "SmartOczVector",
            "SmartPlextor",
            "SmartSanDisk",
            "SmartSanDiskGb",
            "SmartSanDiskDell",
            "SmartSanDiskHp",
            "SmartSanDiskHpVenus",
            "SmartSanDiskLenovo",
            "SmartSanDiskLenovoHelenVenus",
            "SmartSanDiskCloud",
            "SmartToshiba",
            "SmartCorsair",
            "SmartKingston",
            "SmartKingstonSuv",
            "SmartKingstonKC600",
            "SmartKingstonDC500",
            "SmartKingstonSA400",
            "SmartNVMe",
            "SmartRealtek",
            "SmartSKhynix",
            "SmartKioxia",
            "SmartWdc",
            "SmartSsstc",
            "SmartApacer",
            "SmartSiliconMotion",
            "SmartSiliconMotionCVC",
            "SmartPhison",
            "SmartSeagate",
            "SmartSeagateIronWolf",
            "SmartSeagateBarraCuda",
            "SmartMarvell",
            "SmartMaxiotek",
            "SmartYmtc",
            "SmartScy",
            "SmartRecadata",
            "SmartAdataIndustrial",
        ] {
            assert!(tables().contains_key(t), "missing table {t}");
        }
    }
}
