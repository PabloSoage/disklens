//! Fallback backend: smartmontools' `smartctl -j`.
//!
//! smartctl supports more USB bridges and RAID controllers (MegaRAID, cciss, 3ware...) than
//! the native backend. Its JSON is turned back into the raw pages the CrystalDiskInfo logic
//! expects, so both backends go through the same analysis.

use std::process::Command;

use serde_json::Value;

use super::{BlockDevice, Transport};
use crate::smart::{ata::AtaPages, nvme::NvmePages};

pub fn available() -> bool {
    Command::new("smartctl")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn run(args: &[&str]) -> anyhow::Result<Value> {
    let out = Command::new("smartctl").args(args).output()?;
    // smartctl's exit status is a bit mask of disk problems, not only of failures, so the
    // JSON is parsed whatever the status. Bits 0-1 mean the command itself failed.
    let v: Value = serde_json::from_slice(&out.stdout)?;
    if out.status.code().unwrap_or(0) & 0b11 != 0 && v.get("device").is_none() {
        let msg = v["smartctl"]["messages"][0]["string"]
            .as_str()
            .unwrap_or("smartctl failed");
        anyhow::bail!("{msg}");
    }
    Ok(v)
}

/// `smartctl --scan-open`, for systems without the sysfs discovery.
pub fn discover() -> Vec<BlockDevice> {
    let Ok(v) = run(&["-j", "--scan-open"]) else {
        return Vec::new();
    };
    v["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let path = d["name"].as_str()?.to_string();
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            let transport = match d["type"].as_str().unwrap_or("") {
                "nvme" => Transport::Nvme,
                "sat" | "ata" => Transport::Sata,
                t if t.starts_with("usb") => Transport::Usb,
                _ => Transport::Other,
            };
            Some(BlockDevice {
                name,
                path,
                transport,
                ..Default::default()
            })
        })
        .collect()
}

pub enum Pages {
    Ata(AtaPages),
    Nvme(NvmePages),
}

pub fn read(path: &str) -> anyhow::Result<(Pages, Value)> {
    let v = run(&["-j", "-a", path])?;
    let proto = v["device"]["protocol"].as_str().unwrap_or("");
    let pages = match proto {
        "NVMe" => Pages::Nvme(nvme_pages(&v)),
        "ATA" => Pages::Ata(ata_pages(&v)),
        other => anyhow::bail!("smartctl: protocol {other:?} has no ATA/NVMe SMART data"),
    };
    Ok((pages, v))
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

fn put_ata_string(w: &mut [u16], from: usize, len: usize, s: &str) {
    let mut b = s.as_bytes().to_vec();
    b.resize(len * 2, b' ');
    for k in 0..len {
        w[from + k] = (b[2 * k] as u16) << 8 | b[2 * k + 1] as u16;
    }
}

fn sata_bits(s: &str) -> u16 {
    if s.starts_with("6.0") {
        3
    } else if s.starts_with("3.0") {
        2
    } else if s.starts_with("1.5") {
        1
    } else {
        0
    }
}

/// Rebuild the IDENTIFY words CrystalDiskInfo reads from what smartctl reports.
fn ata_pages(v: &Value) -> AtaPages {
    let mut w = vec![0u16; 256];
    put_ata_string(&mut w, 10, 10, v["serial_number"].as_str().unwrap_or(""));
    put_ata_string(&mut w, 23, 4, v["firmware_version"].as_str().unwrap_or(""));
    put_ata_string(&mut w, 27, 20, v["model_name"].as_str().unwrap_or(""));
    let major = u(&v["ata_version"]["major_value"]) as u16;
    w[80] = major;
    w[81] = u(&v["ata_version"]["minor_value"]) as u16;
    if v["smart_support"]["available"].as_bool().unwrap_or(false) {
        w[82] |= 1;
    }
    let max = sata_bits(v["interface_speed"]["max"]["string"].as_str().unwrap_or(""));
    let cur = sata_bits(
        v["interface_speed"]["current"]["string"]
            .as_str()
            .unwrap_or(""),
    );
    if max > 0 {
        w[76] = (1u16 << (max + 1)) - 2; // every generation up to the max
        w[77] = cur << 1;
    }
    if v["trim"]["supported"].as_bool().unwrap_or(false) {
        w[169] |= 1;
    }
    let blocks = u(&v["user_capacity"]["blocks"]);
    let logical = u(&v["logical_block_size"]).max(512);
    let physical = u(&v["physical_block_size"]).max(logical);
    w[83] |= 1 << 10; // 48-bit LBA
    for k in 0..4 {
        w[100 + k] = (blocks >> (16 * k)) as u16;
    }
    if physical == 4096 && logical == 512 {
        w[106] = 0x4003;
    }
    w[217] = match v["rotation_rate"].as_u64() {
        Some(0) => 1,
        Some(r) => r as u16,
        None => 0,
    };
    let identify = w.iter().flat_map(|x| x.to_le_bytes()).collect();

    let table = v["ata_smart_attributes"]["table"].as_array();
    let (smart, thresholds) = match table {
        Some(rows) => {
            let mut data = vec![0u8; 512];
            let mut thr = vec![0u8; 512];
            for (i, r) in rows.iter().take(30).enumerate() {
                let off = 2 + i * 12;
                let id = u(&r["id"]) as u8;
                data[off] = id;
                data[off + 1..off + 3]
                    .copy_from_slice(&(u(&r["flags"]["value"]) as u16).to_le_bytes());
                data[off + 3] = u(&r["value"]) as u8;
                data[off + 4] = u(&r["worst"]) as u8;
                data[off + 5..off + 11].copy_from_slice(&u(&r["raw"]["value"]).to_le_bytes()[..6]);
                thr[off] = id;
                thr[off + 1] = u(&r["thresh"]) as u8;
            }
            (Some(data), Some(thr))
        }
        None => (None, None),
    };
    AtaPages {
        identify,
        smart_check: smart.clone(),
        smart,
        thresholds,
    }
}

fn put_str(b: &mut [u8], at: usize, len: usize, s: &str) {
    let mut v = s.as_bytes().to_vec();
    v.resize(len, b' ');
    b[at..at + len].copy_from_slice(&v[..len]);
}

/// Rebuild Identify Controller and log page 0x02 from smartctl's NVMe JSON.
fn nvme_pages(v: &Value) -> NvmePages {
    let mut c = vec![0u8; 4096];
    c[0..2].copy_from_slice(&(u(&v["nvme_pci_vendor"]["id"]) as u16).to_le_bytes());
    put_str(&mut c, 4, 20, v["serial_number"].as_str().unwrap_or(""));
    put_str(&mut c, 24, 40, v["model_name"].as_str().unwrap_or(""));
    put_str(&mut c, 64, 8, v["firmware_version"].as_str().unwrap_or(""));
    c[80..84].copy_from_slice(&(u(&v["nvme_version"]["value"]) as u32).to_le_bytes());
    let cap = u(&v["nvme_total_capacity"]).max(u(&v["user_capacity"]["bytes"]));
    c[280..288].copy_from_slice(&cap.to_le_bytes());
    c[516..520].copy_from_slice(&(u(&v["nvme_number_of_namespaces"]) as u32).to_le_bytes());

    let h = &v["nvme_smart_health_information_log"];
    let smart_log = (!h.is_null()).then(|| {
        let mut l = vec![0u8; 512];
        l[0] = u(&h["critical_warning"]) as u8;
        let k = (h["temperature"].as_i64().unwrap_or(-273) + 273).max(0) as u16;
        l[1..3].copy_from_slice(&k.to_le_bytes());
        l[3] = u(&h["available_spare"]) as u8;
        l[4] = u(&h["available_spare_threshold"]) as u8;
        l[5] = u(&h["percentage_used"]) as u8;
        let fields = [
            (32, "data_units_read"),
            (48, "data_units_written"),
            (64, "host_reads"),
            (80, "host_writes"),
            (96, "controller_busy_time"),
            (112, "power_cycles"),
            (128, "power_on_hours"),
            (144, "unsafe_shutdowns"),
            (160, "media_errors"),
            (176, "num_err_log_entries"),
        ];
        for (off, key) in fields {
            l[off..off + 8].copy_from_slice(&u(&h[key]).to_le_bytes());
        }
        l[192..196].copy_from_slice(&(u(&h["warning_temp_time"]) as u32).to_le_bytes());
        l[196..200].copy_from_slice(&(u(&h["critical_comp_time"]) as u32).to_le_bytes());
        for (i, t) in h["temperature_sensors"]
            .as_array()
            .into_iter()
            .flatten()
            .take(8)
            .enumerate()
        {
            let k = (t.as_i64().unwrap_or(-273) + 273).max(0) as u16;
            l[200 + 2 * i..202 + 2 * i].copy_from_slice(&k.to_le_bytes());
        }
        l
    });
    NvmePages {
        identify_controller: c,
        identify_namespace: None,
        smart_log,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ata_json_round_trips_through_the_analysis() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/smartctl-ata-hdd.json"))
                .unwrap();
        let pages = ata_pages(&v);
        let (asi, info) = crate::smart::ata::analyze(&pages).unwrap();
        assert_eq!(asi.model, "WDC WD40EFRX-68N32N0");
        assert!(!asi.is_ssd);
        assert_eq!(info.transfer_mode_max, "SATA/600");
        assert_eq!(asi.temperature, 31);
        assert_eq!(asi.detected_power_on_hours, 31250);
    }

    #[test]
    fn nvme_json_round_trips_through_the_analysis() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/smartctl-nvme.json")).unwrap();
        let pages = nvme_pages(&v);
        let (asi, info) = crate::smart::nvme::analyze(&pages).unwrap();
        assert_eq!(asi.model, "Samsung SSD 980 PRO 1TB");
        assert_eq!(info.version, "NVM Express 1.3");
        assert_eq!(asi.temperature, 38);
        assert_eq!(asi.life, 98);
    }
}
