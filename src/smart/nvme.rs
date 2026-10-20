//! NVMe: Identify Controller / Namespace parsing and the SMART / Health Information log
//! (log page 0x02) mapped onto ATA-style attributes, as `AddDiskNVMe` and
//! `NVMeInterpreter.cpp` do in CrystalDiskInfo.

use serde::{Deserialize, Serialize};

use super::types::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NvmeInfo {
    pub version: String,
    pub pci_vendor_id: u16,
    pub warning_temp_c: Option<i32>,
    pub critical_temp_c: Option<i32>,
    pub capacity_bytes: u64,
    pub namespaces: u32,
    pub lba_size: u32,
    pub features: Vec<String>,
}

fn le16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn le32(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}
fn le64(b: &[u8], i: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[i..i + 8]);
    u64::from_le_bytes(a)
}
fn ascii(b: &[u8]) -> String {
    b.iter()
        .map(|&c| {
            if (0x20..0x7F).contains(&c) {
                c as char
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}
fn kelvin(k: u16) -> Option<i32> {
    (k != 0).then(|| k as i32 - 273)
}

/// The NVMe 128-bit counters, of which CrystalDiskInfo reads the low 64 bits.
pub fn counter(log: &[u8], off: usize) -> u128 {
    let lo = le64(log, off) as u128;
    let hi = le64(log, off + 8) as u128;
    lo | hi << 64
}

#[derive(Debug, Clone, Default)]
pub struct NvmePages {
    pub identify_controller: Vec<u8>,
    pub identify_namespace: Option<Vec<u8>>,
    pub smart_log: Option<Vec<u8>>,
}

/// One slot of the 30-entry attribute array `NVMeInterpreter.cpp` fills.
fn put(slots: &mut [RawAttribute; MAX_ATTRIBUTE], idx: usize, id: u8, bytes: &[u8]) {
    let mut a = RawAttribute {
        id,
        ..Default::default()
    };
    let n = bytes.len().min(6);
    a.raw[..n].copy_from_slice(&bytes[..n]);
    slots[idx] = a;
}

pub fn analyze(pages: &NvmePages) -> Option<(AtaState, NvmeInfo)> {
    let c = &pages.identify_controller;
    if c.len() < 4096 {
        return None;
    }
    let mut asi = AtaState::new(&ascii(&c[24..64]), &ascii(&c[64..72]), &ascii(&c[4..24]));
    asi.is_nvme = true;
    asi.is_ssd = true;
    asi.is_smart_correct = true;
    asi.is_threshold_correct = true;
    asi.host_rw_unit = HostRwUnit::B512;
    asi.vendor = Vendor::Nvme;
    asi.smart_key = "SmartNVMe";

    let mut info = NvmeInfo {
        pci_vendor_id: le16(c, 0),
        ..Default::default()
    };
    let ver = le32(c, 80);
    let (major, minor) = (ver >> 16, (ver >> 8) & 0xFF);
    info.version = if major == 0 {
        "NVM Express 1.0/1.1".into()
    } else {
        format!("NVM Express {major}.{minor}")
    };
    let (wctemp, cctemp) = (le16(c, 266), le16(c, 268));
    info.warning_temp_c = kelvin(wctemp);
    info.critical_temp_c = kelvin(cctemp);
    let threshold_defined = wctemp != 0 || cctemp != 0;
    let thermal_mgmt_defined = le16(c, 324) != 0 || le16(c, 326) != 0;
    info.capacity_bytes = counter(c, 280) as u64;
    info.namespaces = le32(c, 516);

    let oacs = le16(c, 256);
    let oncs = le16(c, 520);
    let vwc = c[525];
    let mut f = vec!["S.M.A.R.T.".to_string()];
    if oncs & (1 << 2) != 0 {
        f.push("TRIM".into());
    }
    if vwc & 1 != 0 {
        f.push("VolatileWriteCache".into());
    }
    if oacs & (1 << 1) != 0 {
        f.push("Format NVM".into());
    }
    if oacs & (1 << 2) != 0 {
        f.push("Firmware Update".into());
    }
    if oacs & (1 << 4) != 0 {
        f.push("Self-test".into());
    }
    info.features = f;

    if let Some(ns) = &pages.identify_namespace {
        if ns.len() >= 4096 {
            let nsze = le64(ns, 0);
            let flbas = (ns[26] & 0x0F) as usize;
            let lbads = ns[128 + flbas * 4 + 2];
            let lba = if (9..=16).contains(&lbads) {
                1u32 << lbads
            } else {
                512
            };
            info.lba_size = lba;
            if info.capacity_bytes == 0 {
                info.capacity_bytes = nsze * lba as u64;
            }
        }
    }

    let Some(log) = &pages.smart_log else {
        asi.is_smart_correct = false;
        return Some((asi, info));
    };
    if log.len() < 512 {
        return None;
    }
    asi.smart_read_data = log.clone();

    asi.temperature = log[2] as i32 * 256 + log[1] as i32 - 273;
    if asi.temperature == -273 || asi.temperature > 100 {
        asi.temperature = -1000;
    }
    asi.life = (100 - log[5] as i32).max(0);
    asi.host_reads_gb = ((le64(log, 0x20) as u128 * 1000) >> 21) as u64 as i32 as i64;
    asi.host_writes_gb = ((le64(log, 0x30) as u128 * 1000) >> 21) as u64 as i32 as i64;
    asi.power_on_count = le32(log, 0x70) as i64;
    asi.detected_power_on_hours = le32(log, 0x80) as i32 as i64;
    asi.measured_power_on_hours = asi.detected_power_on_hours;

    let mut s = [RawAttribute::default(); MAX_ATTRIBUTE];
    put(&mut s, 0, 1, &log[0..1]); // critical warning
    put(&mut s, 1, 2, &log[1..3]); // composite temperature (K)
    put(&mut s, 2, 3, &log[3..4]); // available spare
    put(&mut s, 3, 4, &log[4..5]); // available spare threshold
    put(&mut s, 4, 5, &log[5..6]); // percentage used
    for (k, off) in [32, 48, 64, 80, 96, 112, 128, 144, 160, 176]
        .iter()
        .enumerate()
    {
        put(&mut s, 5 + k, 6 + k as u8, &log[*off..*off + 6]);
    }
    if threshold_defined {
        put(&mut s, 15, 16, &log[192..196]);
        put(&mut s, 16, 17, &log[196..200]);
    }
    for i in 0..8 {
        let b = &log[200 + i * 2..202 + i * 2];
        if b[0] != 0 || b[1] != 0 {
            put(&mut s, 18 + i, 18 + i as u8, b);
        }
    }
    if thermal_mgmt_defined {
        for i in 0..4 {
            put(&mut s, 26 + i, 26 + i as u8, &log[216 + i * 4..220 + i * 4]);
        }
    }
    asi.attributes = s.into_iter().filter(|a| a.id != 0).collect();
    asi.thresholds = vec![0; asi.attributes.len()];
    Some((asi, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn controller(model: &str) -> Vec<u8> {
        let mut c = vec![0u8; 4096];
        c[0..2].copy_from_slice(&0x144Du16.to_le_bytes());
        let put = |c: &mut Vec<u8>, at: usize, len: usize, s: &str| {
            let mut b = s.as_bytes().to_vec();
            b.resize(len, b' ');
            c[at..at + len].copy_from_slice(&b);
        };
        put(&mut c, 4, 20, "S5GXNF0N123456");
        put(&mut c, 24, 40, model);
        put(&mut c, 64, 8, "2B2QEXM7");
        c[80..84].copy_from_slice(&0x0001_0400u32.to_le_bytes());
        c[266..268].copy_from_slice(&355u16.to_le_bytes()); // 82 C
        c[268..270].copy_from_slice(&358u16.to_le_bytes());
        c
    }

    fn log(temp_k: u16, spare: u8, thr: u8, used: u8, written_units: u64) -> Vec<u8> {
        let mut l = vec![0u8; 512];
        l[1..3].copy_from_slice(&temp_k.to_le_bytes());
        l[3] = spare;
        l[4] = thr;
        l[5] = used;
        l[48..56].copy_from_slice(&written_units.to_le_bytes());
        l[0x80..0x84].copy_from_slice(&1234u32.to_le_bytes());
        l[200..202].copy_from_slice(&310u16.to_le_bytes());
        l
    }

    #[test]
    fn maps_the_log_like_cdi() {
        let pages = NvmePages {
            identify_controller: controller("Samsung SSD 970 EVO Plus 1TB"),
            identify_namespace: None,
            smart_log: Some(log(313, 100, 10, 3, 2_097_152)),
        };
        let (a, info) = analyze(&pages).unwrap();
        assert_eq!(a.model, "Samsung SSD 970 EVO Plus 1TB");
        assert_eq!(info.version, "NVM Express 1.4");
        assert_eq!(a.temperature, 40);
        assert_eq!(a.life, 97);
        // 2^21 units of 512,000 bytes = 1000 GiB.
        assert_eq!(a.host_writes_gb, 1000);
        assert_eq!(a.detected_power_on_hours, 1234);
        let ids: Vec<u8> = a.attributes.iter().map(|x| x.id).collect();
        assert_eq!(&ids[..15], &(1..=15).collect::<Vec<u8>>()[..]);
        assert!(ids.contains(&16) && ids.contains(&17) && ids.contains(&18));
        assert!(!ids.contains(&19));
    }
}
