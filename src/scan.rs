//! Discover every disk, read it with the chosen backend and analyse it.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::device::{self, Backend, BlockDevice, Transport};
use crate::smart::{self, DiskReport, Platform, Thresholds};

/// Options shared by every front end.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanOptions {
    pub backend: Backend,
    pub thresholds: Thresholds,
    /// Only these kernel names (`sda`, `nvme0n1`); all disks when empty.
    pub only: Vec<String>,
    /// Analyse these dump files (or directories of them) instead of the local disks.
    #[serde(default)]
    pub from: Vec<std::path::PathBuf>,
}

/// Load `disklens dump` files, expanding directories to their `*.json`.
pub fn load_dumps(paths: &[std::path::PathBuf]) -> Vec<crate::dump::Dump> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut v: Vec<_> = std::fs::read_dir(p)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|f| f.extension().is_some_and(|x| x == "json"))
                .collect();
            v.sort();
            files.extend(v.into_iter().map(|f| (f, false)));
        } else {
            files.push((p.clone(), true));
        }
    }
    files
        .iter()
        .filter_map(|(f, explicit)| {
            let text = std::fs::read_to_string(f).ok()?;
            match serde_json::from_str(
                text.trim_start_matches(char::from_u32(0xFEFF).unwrap_or(' ')),
            ) {
                Ok(d) => Some(d),
                // Other JSON files in a directory are skipped quietly; a file named on
                // the command line gets the reason.
                Err(e) if *explicit => {
                    eprintln!("{}: not a disklens dump ({e})", f.display());
                    None
                }
                Err(_) => None,
            }
        })
        .collect()
}

fn platform(d: &BlockDevice, source: &str) -> Platform {
    Platform {
        name: d.name.clone(),
        path: d.path.clone(),
        interface: match d.transport {
            Transport::Sata | Transport::Other => String::new(),
            t => t.label().to_string(),
        },
        transfer_mode: d.link.clone(),
        capacity_bytes: d.size_bytes,
        rotational: d.rotational,
        mounts: d.mounts.clone(),
        notes: Vec::new(),
        source: source.to_string(),
    }
}

#[cfg(target_os = "linux")]
fn read_native(d: &BlockDevice, t: &Thresholds) -> Result<DiskReport, String> {
    use crate::device::linux;
    match d.transport {
        Transport::Nvme => {
            let pages = linux::read_nvme(&d.name).map_err(|e| format!("NVMe admin: {e}"))?;
            smart::analyze_nvme(&pages, platform(d, "native"), t).ok_or("bad NVMe identify".into())
        }
        Transport::Sata | Transport::Usb | Transport::Sas | Transport::Other => {
            let read = linux::read_ata(&d.path).map_err(|e| format!("ATA pass-through: {e}"))?;
            let mut p = platform(d, "native");
            p.notes = read.errors;
            smart::analyze_ata(&read.pages, p, t).ok_or("bad IDENTIFY".into())
        }
        Transport::Virtio | Transport::Mmc => Err("no SMART on this kind of device".into()),
    }
}

#[cfg(not(target_os = "linux"))]
fn read_native(_d: &BlockDevice, _t: &Thresholds) -> Result<DiskReport, String> {
    Err("the native backend is Linux-only".into())
}

fn read_smartctl(d: &BlockDevice, t: &Thresholds) -> Result<DiskReport, String> {
    use crate::device::smartctl;
    let (pages, _) = smartctl::read(&d.path).map_err(|e| e.to_string())?;
    let r = match pages {
        smartctl::Pages::Ata(p) => smart::analyze_ata(&p, platform(d, "smartctl"), t),
        smartctl::Pages::Nvme(p) => smart::analyze_nvme(&p, platform(d, "smartctl"), t),
    };
    r.ok_or_else(|| "smartctl returned no usable data".into())
}

/// Read one disk. Never fails: a disk we cannot query still shows up, with the reason.
pub fn read_disk(d: &BlockDevice, opts: &ScanOptions) -> DiskReport {
    let t = &opts.thresholds;
    let mut notes = Vec::new();
    let attempt = match opts.backend {
        Backend::Native => read_native(d, t),
        Backend::Smartctl => read_smartctl(d, t),
        Backend::Auto => read_native(d, t).or_else(|e| {
            notes.push(e);
            if device::smartctl::available() {
                read_smartctl(d, t)
            } else {
                Err("smartctl not installed".into())
            }
        }),
    };
    match attempt {
        Ok(mut r) => {
            if r.model.is_empty() {
                r.model = d.model.clone();
            }
            r.notes.splice(0..0, notes);
            r
        }
        Err(e) => {
            notes.push(e);
            if !device::privileged() {
                notes.push(
                    "reading SMART data needs root (sudo, pkexec or the systemd service)".into(),
                );
            }
            let mut p = platform(d, "sysfs");
            p.notes = notes;
            smart::report_without_smart(&d.model, &d.serial, &d.firmware, p)
        }
    }
}

pub fn scan(opts: &ScanOptions) -> Vec<DiskReport> {
    if !opts.from.is_empty() {
        return load_dumps(&opts.from)
            .iter()
            .filter(|d| {
                opts.only.is_empty()
                    || opts
                        .only
                        .iter()
                        .any(|o| *o == d.device.name || *o == d.device.path)
            })
            .filter_map(|d| d.analyze(&opts.thresholds))
            .collect();
    }
    let disks: Vec<BlockDevice> = device::discover()
        .into_iter()
        .filter(|d| opts.only.is_empty() || opts.only.iter().any(|o| *o == d.name || *o == d.path))
        .collect();
    // Disks are independent and some (USB, sleeping HDDs) are slow: read them in parallel.
    std::thread::scope(|s| {
        let handles: Vec<_> = disks
            .iter()
            .map(|d| s.spawn(|| read_disk(d, opts)))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

/// One sample of the values worth plotting.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    /// Unix seconds.
    pub t: i64,
    pub temperature_c: Option<i32>,
    pub life_percent: Option<i32>,
    pub host_writes_gb: Option<i64>,
}

/// Recent samples per disk, keyed by model + serial so renames (`sdb` -> `sdc`) do not mix.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    pub capacity: usize,
    pub disks: HashMap<String, VecDeque<Sample>>,
}

pub fn disk_key(r: &DiskReport) -> String {
    format!("{} {}", r.model, r.serial)
}

impl History {
    pub fn new(capacity: usize) -> Self {
        History {
            capacity,
            disks: HashMap::new(),
        }
    }

    pub fn record(&mut self, reports: &[DiskReport]) {
        let t = chrono::Utc::now().timestamp();
        for r in reports {
            let q = self.disks.entry(disk_key(r)).or_default();
            q.push_back(Sample {
                t,
                temperature_c: r.temperature_c,
                life_percent: r.life_percent,
                host_writes_gb: r.host_writes_gb,
            });
            while q.len() > self.capacity.max(1) {
                q.pop_front();
            }
        }
    }

    pub fn of(&self, r: &DiskReport) -> Vec<Sample> {
        self.disks
            .get(&disk_key(r))
            .map(|q| q.iter().copied().collect())
            .unwrap_or_default()
    }
}
