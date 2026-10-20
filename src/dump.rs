//! Raw captures of a disk, to reproduce an analysis without the hardware.
//!
//! A dump stores the pages exactly as the disk returned them (hex), so a misdetected disk
//! can be reported and turned into a regression test (`tests/captures/`).

use serde::{Deserialize, Serialize};

use crate::device::{Backend, BlockDevice, Transport};
use crate::smart::{self, ata::AtaPages, nvme::NvmePages, DiskReport, Platform, Thresholds};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dump {
    pub format: u32,
    pub tool: String,
    pub captured_at: String,
    pub device: BlockDevice,
    pub source: String,
    #[serde(default)]
    pub ata_identify: Option<String>,
    #[serde(default)]
    pub ata_smart: Option<String>,
    #[serde(default)]
    pub ata_smart_check: Option<String>,
    #[serde(default)]
    pub ata_thresholds: Option<String>,
    #[serde(default)]
    pub nvme_identify_controller: Option<String>,
    #[serde(default)]
    pub nvme_identify_namespace: Option<String>,
    #[serde(default)]
    pub nvme_smart_log: Option<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

impl Dump {
    fn empty(d: &BlockDevice, source: &str) -> Self {
        Dump {
            format: 1,
            tool: format!("disklens {}", env!("CARGO_PKG_VERSION")),
            captured_at: chrono::Utc::now().to_rfc3339(),
            device: d.clone(),
            source: source.into(),
            ata_identify: None,
            ata_smart: None,
            ata_smart_check: None,
            ata_thresholds: None,
            nvme_identify_controller: None,
            nvme_identify_namespace: None,
            nvme_smart_log: None,
            errors: Vec::new(),
        }
    }

    fn set_ata(&mut self, p: &AtaPages) {
        self.ata_identify = Some(hex(&p.identify));
        self.ata_smart = p.smart.as_deref().map(hex);
        self.ata_smart_check = p.smart_check.as_deref().map(hex);
        self.ata_thresholds = p.thresholds.as_deref().map(hex);
    }

    fn set_nvme(&mut self, p: &NvmePages) {
        self.nvme_identify_controller = Some(hex(&p.identify_controller));
        self.nvme_identify_namespace = p.identify_namespace.as_deref().map(hex);
        self.nvme_smart_log = p.smart_log.as_deref().map(hex);
    }

    pub fn ata_pages(&self) -> Option<AtaPages> {
        Some(AtaPages {
            identify: unhex(self.ata_identify.as_deref()?)?,
            smart: self.ata_smart.as_deref().and_then(unhex),
            smart_check: self.ata_smart_check.as_deref().and_then(unhex),
            thresholds: self.ata_thresholds.as_deref().and_then(unhex),
        })
    }

    pub fn nvme_pages(&self) -> Option<NvmePages> {
        Some(NvmePages {
            identify_controller: unhex(self.nvme_identify_controller.as_deref()?)?,
            identify_namespace: self.nvme_identify_namespace.as_deref().and_then(unhex),
            smart_log: self.nvme_smart_log.as_deref().and_then(unhex),
        })
    }

    /// Run the analysis on the stored pages.
    pub fn analyze(&self, t: &Thresholds) -> Option<DiskReport> {
        let d = &self.device;
        let plat = Platform {
            name: d.name.clone(),
            path: d.path.clone(),
            interface: match d.transport {
                Transport::Sata | Transport::Other => String::new(),
                tr => tr.label().into(),
            },
            transfer_mode: d.link.clone(),
            capacity_bytes: d.size_bytes,
            rotational: d.rotational,
            mounts: d.mounts.clone(),
            notes: Vec::new(),
            source: format!("dump ({})", self.source),
        };
        if let Some(p) = self.nvme_pages() {
            smart::analyze_nvme(&p, plat, t)
        } else {
            smart::analyze_ata(&self.ata_pages()?, plat, t)
        }
    }
}

/// Read the raw pages of one disk without analysing them.
pub fn capture(d: &BlockDevice, backend: Backend) -> Dump {
    let mut dump = Dump::empty(d, "native");
    if backend != Backend::Smartctl {
        use crate::device::native;
        let native = if d.transport == Transport::Nvme {
            native::read_nvme(d).map(|p| dump.set_nvme(&p))
        } else {
            native::read_ata(d).map(|(pages, errors)| {
                dump.set_ata(&pages);
                dump.errors.extend(errors);
            })
        };
        match native {
            Ok(()) => return dump,
            Err(e) => dump.errors.push(format!("native: {e}")),
        }
    }
    if backend != Backend::Native {
        match crate::device::smartctl::read(&d.path) {
            Ok((pages, _)) => {
                dump.source = "smartctl".into();
                match pages {
                    crate::device::smartctl::Pages::Ata(p) => dump.set_ata(&p),
                    crate::device::smartctl::Pages::Nvme(p) => dump.set_nvme(&p),
                }
            }
            Err(e) => dump.errors.push(format!("smartctl: {e}")),
        }
    }
    dump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let b = vec![0u8, 1, 0xAB, 0xFF];
        assert_eq!(unhex(&hex(&b)).unwrap(), b);
        assert!(unhex("abc").is_none());
    }
}
