//! Health verdicts: a port of `CAtaSmart::CheckDiskStatus` (the disk's overall status),
//! of the per-attribute icons in `CDiskInfoDlg::UpdateListCtrl` and of
//! `GetTemperatureClass`, from CrystalDiskInfo 9.9.3.

use serde::{Deserialize, Serialize};

use super::types::*;
use Vendor::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Good,
    Caution,
    Bad,
    Unknown,
}

impl Health {
    pub fn label(self) -> &'static str {
        match self {
            Health::Good => "Good",
            Health::Caution => "Caution",
            Health::Bad => "Bad",
            Health::Unknown => "Unknown",
        }
    }
}

/// The user-tunable caution thresholds (`ThreasholdOfCaution*` and `AlarmTemperature` in
/// `DiskInfo.ini`). The defaults are CrystalDiskInfo's.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Thresholds {
    /// Raw count of reallocated sectors (0x05) that turns an HDD to Caution.
    pub reallocated: u16,
    /// Raw count of pending sectors (0xC5).
    pub pending: u16,
    /// Raw count of uncorrectable sectors (0xC6).
    pub uncorrectable: u16,
    /// Remaining life (%) at or below which an SSD turns to Caution.
    pub life: i32,
    /// Alarm temperature for HDDs and SSDs, in °C.
    pub alarm_hdd: i32,
    pub alarm_ssd: i32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            reallocated: 1,
            pending: 1,
            uncorrectable: 1,
            life: 10,
            alarm_hdd: 50,
            alarm_ssd: 60,
        }
    }
}

impl Thresholds {
    pub fn alarm_for(&self, is_ssd: bool) -> i32 {
        if is_ssd {
            self.alarm_ssd
        } else {
            self.alarm_hdd
        }
    }
}

fn is_virtual(model: &str) -> bool {
    starts(model, "Parallels") || starts(model, "VMware") || starts(model, "QEMU")
}

/// Ids whose normalised value is compared with the threshold in `CheckDiskStatus`.
fn status_checked_id(id: u8) -> bool {
    (0x01..=0x0D).contains(&id)
        || id == 0x16
        || (0xBB..=0xBD).contains(&id)
        || (0xBF..=0xC1).contains(&id)
        || (0xC3..=0xD1).contains(&id)
        || (0xD3..=0xD4).contains(&id)
        || (0xDC..=0xE4).contains(&id)
        || (0xE6..=0xE7).contains(&id)
        || id == 0xF0
        || id == 0xFA
        || id == 0xFE
}

/// Ids compared with the threshold for the list icon (note 0xBE is included here).
fn list_checked_id(id: u8) -> bool {
    (0x01..=0x0D).contains(&id)
        || id == 0x16
        || (0xBB..=0xC1).contains(&id)
        || (0xC3..=0xD1).contains(&id)
        || (0xD3..=0xD4).contains(&id)
        || (0xDC..=0xE4).contains(&id)
        || (0xE6..=0xE7).contains(&id)
        || id == 0xF0
        || id == 0xFA
        || id == 0xFE
}

/// The (attribute id, vendor) pairs that carry the SSD's remaining life.
fn is_life_attribute(asi: &AtaState, id: u8) -> bool {
    let v = asi.vendor;
    let vs = |l: &[Vendor]| l.contains(&v);
    (id == 0xA9
        && (v == Realtek
            || (v == Kingston && asi.host_rw_unit == HostRwUnit::Mb32)
            || v == SiliconMotion))
        || (id == 0xAD && v == Kioxia)
        || (id == 0xB1 && v == Samsung)
        || (id == 0xBB && v == Mtron)
        || (id == 0xCA && vs(&[Micron, MicronMu03, IntelDc, SiliconMotionCvc]))
        || (id == 0xD1 && v == Indilinx)
        || (id == 0xE7
            && vs(&[
                SandForce,
                Corsair,
                Kingston,
                SkHynix,
                Realtek,
                SanDisk,
                Ssstc,
                Apacer,
                Phison,
                JMicron,
                Maxiotek,
                Ymtc,
                Scy,
                Recadata,
                AdataIndustrial,
            ]))
        || (id == 0xE8 && v == Plextor)
        || (id == 0xE9 && vs(&[Intel, Ocz, OczVector, SkHynix]))
        || (id == 0xE9 && v == SanDiskLenovoHelenVenus)
        || (id == 0xE9 && v == Samsung && super::vendor::is_samsung_enterprise_model(&asi.model))
}

/// The life attributes as the attribute list sees them. The list differs from
/// `is_life_attribute`: 0xE7 includes Seagate and not Phison, 0xAD includes Toshiba, and the
/// Samsung enterprise 0xE9 is absent.
fn is_list_life_attribute(asi: &AtaState, id: u8) -> bool {
    let v = asi.vendor;
    let vs = |l: &[Vendor]| l.contains(&v);
    (id == 0xA9
        && (v == Realtek
            || (v == Kingston && asi.host_rw_unit == HostRwUnit::Mb32)
            || v == SiliconMotion))
        || (id == 0xAD && vs(&[Toshiba, Kioxia]))
        || (id == 0xB1 && v == Samsung)
        || (id == 0xBB && v == Mtron)
        || (id == 0xCA && vs(&[Micron, MicronMu03, IntelDc, SiliconMotionCvc]))
        || (id == 0xD1 && v == Indilinx)
        || (id == 0xE7
            && vs(&[
                SandForce,
                Corsair,
                Kingston,
                SkHynix,
                Realtek,
                SanDisk,
                Ssstc,
                Apacer,
                JMicron,
                Seagate,
                Maxiotek,
                Ymtc,
                Scy,
                Recadata,
                AdataIndustrial,
            ]))
        || (id == 0xE8 && v == Plextor)
        || (id == 0xE9 && vs(&[Intel, Ocz, OczVector, SkHynix]))
        || (id == 0xE9 && v == SanDiskLenovoHelenVenus)
        || (id == 0xE6 && vs(&[SanDiskLenovo, SanDiskDell]))
        || (id == 0xC9 && vs(&[SanDiskHp, SanDiskHpVenus]))
}

/// `CheckDiskStatus`. For some SanDisk OEMs it also stores the life it recomputes, as the
/// original does.
pub fn disk_status(asi: &mut AtaState, t: &Thresholds) -> Health {
    if asi.is_nvme {
        if is_virtual(&asi.model) {
            return Health::Unknown;
        }
        let raw0 = |k: usize| asi.attributes.get(k).map_or(0, |a| a.raw[0]);
        if raw0(0) > 0 {
            return Health::Bad; // Critical warning.
        }
        let (spare, spare_thr) = (raw0(2), raw0(3));
        if spare_thr == 0 || spare_thr > 100 {
            // Available Spare Threshold not supported.
        } else if spare < spare_thr {
            return Health::Bad;
        } else if spare == spare_thr && spare_thr != 100 {
            return Health::Caution;
        }
        return if asi.life > t.life {
            Health::Good
        } else {
            Health::Caution
        };
    }

    if !asi.is_smart_correct || (!asi.is_ssd && !asi.is_threshold_correct) || asi.is_threshold_bug {
        return Health::Unknown;
    }

    let mut error = 0;
    let mut caution = 0;
    let mut flag_unknown = true;

    for j in 0..asi.attributes.len() {
        let a = asi.attributes[j];
        let thr = asi.threshold(j);

        // Duplicate ids make the whole table suspect.
        if asi.attributes[..j]
            .iter()
            .any(|b| b.id != 0 && b.id == a.id)
        {
            return Health::Unknown;
        }

        if asi.vendor == SandForce
            && a.id == 0x01
            && a.current == 0
            && a.raw[0] == 0
            && a.raw[1] == 0
        {
            // SandForce read error rate bug.
        } else if a.id == 0xE8 && asi.flag_life_sandisk_usb_memory {
            // SanDisk USB memory workaround.
        } else if a.id == 0xC2 {
            // Temperature threshold bug.
        } else if asi.is_ssd && asi.is_raw_values8 {
        } else if ((asi.is_ssd && !asi.is_raw_values8) || status_checked_id(a.id))
            && thr != 0
            && a.current < thr
        {
            error += 1;
        }

        if asi.is_ssd && thr != 0 {
            flag_unknown = false;
        }

        if a.id == 0x05 || a.id == 0xC5 || a.id == 0xC6 {
            if a.raw[0..4] != [0xFF; 4] {
                let raw = a.raw16();
                let limit = match a.id {
                    0x05 => t.reallocated,
                    0xC5 => t.pending,
                    _ => t.uncorrectable,
                };
                if limit > 0 && raw >= limit && !asi.is_ssd {
                    caution = 1;
                }
            }
            if !asi.is_ssd {
                flag_unknown = false;
            }
        } else if is_life_attribute(asi, a.id) {
            flag_unknown = false;
            let mut life = if asi.flag_life_no_report {
                -1
            } else if asi.flag_life_raw_value_increment {
                100 - a.raw[0] as i32
            } else if asi.flag_life_raw_value {
                a.raw[0] as i32
            } else {
                a.current as i32
            };
            if life > 100 {
                life = 100;
            }
            let raw_based = asi.flag_life_raw_value || asi.flag_life_raw_value_increment;
            if life == -1 {
            } else if life == 0 || (!raw_based && life < thr as i32) {
                error = 1;
            } else if life <= t.life {
                caution = 1;
            }
        } else if a.id == 0xE6 && (asi.vendor == Wdc || asi.vendor == SanDisk) {
            flag_unknown = false;
            let life = if asi.flag_life_sandisk_0_1 {
                100 - (a.raw[1] as i32 * 256 + a.raw[0] as i32) / 100
            } else {
                100 - a.raw[1] as i32
            }
            .clamp(0, 100);
            if asi.flag_life_sandisk_usb_memory {
            } else if life == 0 {
                error = 1;
            } else if life <= t.life {
                caution = 1;
            }
        } else if (a.id == 0xE6 && (asi.vendor == SanDiskLenovo || asi.vendor == SanDiskDell))
            || (a.id == 0xC9 && (asi.vendor == SanDiskHp || asi.vendor == SanDiskHpVenus))
        {
            flag_unknown = false;
            let life = (a.current as i32).clamp(0, 100);
            asi.life = life;
            if life == 0 {
                error = 1;
            } else if life <= t.life {
                caution = 1;
            }
        }
    }

    if error > 0 {
        Health::Bad
    } else if flag_unknown {
        Health::Unknown
    } else if caution > 0 {
        Health::Caution
    } else {
        Health::Good
    }
}

/// The icon of one row in the attribute list (`UpdateListCtrl`).
pub fn attribute_status(asi: &AtaState, j: usize, t: &Thresholds) -> Health {
    let a = asi.attributes[j];
    let thr = asi.threshold(j);
    let alarm = t.alarm_for(asi.is_ssd);

    if asi.is_nvme {
        let raw = |k: usize| asi.attributes.get(k).map_or([0u8; 6], |a| a.raw);
        if is_virtual(&asi.model) {
            return Health::Unknown;
        }
        if a.id == 0x01 && a.raw[0] != 0 {
            return Health::Bad;
        }
        if a.id == 0x02 {
            let r = raw(1);
            let temp = u16::from_le_bytes([r[0], r[1]]) as i32 - 273;
            if temp >= alarm {
                return Health::Bad;
            } else if temp == alarm {
                return Health::Caution;
            }
            return Health::Good;
        }
        let (spare, spare_thr) = (raw(2)[0], raw(3)[0]);
        if a.id == 0x03 && spare_thr > 100 {
            return Health::Good;
        }
        if a.id == 0x03 && spare == 0 && spare_thr == 0 {
            return Health::Good;
        }
        if a.id == 0x03 && spare < spare_thr {
            return Health::Bad;
        }
        if a.id == 0x03 && spare == spare_thr && spare_thr != 100 {
            return Health::Caution;
        }
        if a.id == 0x05 && (100 - a.raw[0] as i32) <= t.life {
            return Health::Caution;
        }
        return Health::Good;
    }

    if !(asi.is_smart_correct && asi.is_threshold_correct && !asi.is_threshold_bug) {
        return Health::Unknown;
    }

    let below = thr != 0 && a.current < thr;
    if !asi.is_ssd && (a.id == 0x05 || a.id == 0xC5 || a.id == 0xC6) {
        let limit = match a.id {
            0x05 => t.reallocated,
            0xC5 => t.pending,
            _ => t.uncorrectable,
        };
        return if below {
            Health::Bad
        } else if limit > 0 && a.raw16() >= limit {
            Health::Caution
        } else {
            Health::Good
        };
    }
    if a.id == 0xE8 && asi.flag_life_sandisk_usb_memory {
        return Health::Good;
    }
    if a.id == 0xC2 || a.id == 0xB8 {
        // Temperature and end-to-end error are never flagged.
        return Health::Good;
    }
    if a.id == 0xE6 && (asi.vendor == Wdc || asi.vendor == SanDisk) {
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
        let life = life.max(0);
        return if asi.flag_life_sandisk_usb_memory {
            Health::Good
        } else if life == 0 {
            Health::Bad
        } else if life <= t.life {
            Health::Caution
        } else {
            Health::Good
        };
    }
    if is_list_life_attribute(asi, a.id) {
        return if asi.flag_life_raw_value || asi.flag_life_raw_value_increment {
            Health::Good
        } else if a.current == 0 || a.current < thr {
            Health::Bad
        } else if a.current as i32 <= t.life {
            Health::Caution
        } else {
            Health::Good
        };
    }
    if a.id == 0x01 && asi.vendor == SandForce {
        return if a.current == 0 && a.raw[0] == 0 && a.raw[1] == 0 {
            Health::Good
        } else if below {
            Health::Bad
        } else {
            Health::Good
        };
    }
    if (asi.is_ssd && !asi.is_raw_values8) || list_checked_id(a.id) {
        return if below { Health::Bad } else { Health::Good };
    }
    Health::Good
}

/// `GetTemperatureClass`: the colour of the temperature box.
pub fn temperature_status(temperature: i32, alarm: i32) -> Health {
    if temperature >= alarm {
        Health::Bad
    } else if temperature == -1000 {
        Health::Unknown
    } else {
        Health::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdd(attrs: &[(u8, u8, u64, u8)]) -> AtaState {
        let mut a = AtaState::new("WDC WD40EFRX-68N32N0", "82.00A82", "X");
        a.is_smart_correct = true;
        a.is_threshold_correct = true;
        for &(id, cur, raw, thr) in attrs {
            let b = raw.to_le_bytes();
            a.attributes.push(RawAttribute {
                id,
                current: cur,
                worst: cur,
                raw: [b[0], b[1], b[2], b[3], b[4], b[5]],
                ..Default::default()
            });
            a.thresholds.push(thr);
        }
        a
    }

    #[test]
    fn healthy_hdd_is_good() {
        let mut a = hdd(&[(0x01, 200, 0, 51), (0x05, 200, 0, 140), (0xC5, 200, 0, 0)]);
        assert_eq!(disk_status(&mut a, &Thresholds::default()), Health::Good);
    }

    #[test]
    fn one_reallocated_sector_is_caution() {
        let mut a = hdd(&[(0x05, 200, 1, 140)]);
        assert_eq!(disk_status(&mut a, &Thresholds::default()), Health::Caution);
        assert_eq!(
            attribute_status(&a, 0, &Thresholds::default()),
            Health::Caution
        );
    }

    #[test]
    fn below_threshold_is_bad() {
        let mut a = hdd(&[(0x05, 100, 2000, 140)]);
        assert_eq!(disk_status(&mut a, &Thresholds::default()), Health::Bad);
        assert_eq!(attribute_status(&a, 0, &Thresholds::default()), Health::Bad);
    }

    #[test]
    fn temperature_below_threshold_is_ignored() {
        let mut a = hdd(&[(0x05, 200, 0, 140), (0xC2, 30, 30, 45)]);
        assert_eq!(disk_status(&mut a, &Thresholds::default()), Health::Good);
    }

    #[test]
    fn duplicate_ids_are_unknown() {
        let mut a = hdd(&[(0x05, 200, 0, 140), (0x05, 200, 0, 140)]);
        assert_eq!(disk_status(&mut a, &Thresholds::default()), Health::Unknown);
    }

    #[test]
    fn hdd_without_thresholds_is_unknown() {
        let mut a = hdd(&[(0x05, 200, 0, 140)]);
        a.is_threshold_correct = false;
        assert_eq!(disk_status(&mut a, &Thresholds::default()), Health::Unknown);
    }

    #[test]
    fn temperature_box() {
        assert_eq!(temperature_status(49, 50), Health::Good);
        assert_eq!(temperature_status(50, 50), Health::Bad);
        assert_eq!(temperature_status(-1000, 50), Health::Unknown);
    }
}
