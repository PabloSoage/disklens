//! Plain terminal output: `list`, `show`, `check` and `dump`.
//!
//! `show` prints the same blocks as CrystalDiskInfo's `DiskInfo.txt`, so the two can be
//! compared field by field.

use std::io::{IsTerminal, Write};
use std::path::Path;

use anyhow::Result;

use disklens::scan::ScanOptions;
use disklens::smart::{DiskReport, Health, Protocol};
use disklens::units;

fn color(h: Health) -> &'static str {
    match h {
        Health::Good => "\x1b[36m",
        Health::Caution => "\x1b[33m",
        Health::Bad => "\x1b[31m",
        Health::Unknown => "\x1b[90m",
    }
}

fn paint(h: Health, s: &str, tty: bool) -> String {
    if tty {
        format!("{}{s}\x1b[0m", color(h))
    } else {
        s.to_string()
    }
}

pub fn health_text(r: &DiskReport) -> String {
    match r.life_percent {
        Some(l) => format!("{} ({l} %)", r.health.label()),
        None => r.health.label().to_string(),
    }
}

pub fn list(reports: &[DiskReport], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(reports)?);
        return Ok(());
    }
    let tty = std::io::stdout().is_terminal();
    if reports.is_empty() {
        println!("No disks found.");
        return Ok(());
    }
    println!(
        "{:<10} {:<16} {:>8} {:>9}  {:<40} MOUNTS",
        "DISK", "HEALTH", "TEMP", "SIZE", "MODEL"
    );
    for r in reports {
        let health = format!("{:<16}", health_text(r));
        let temp = format!("{:>8}", units::temperature(r.temperature_c));
        println!(
            "{:<10} {} {} {:>9}  {:<40} {}",
            r.name,
            paint(r.health, &health, tty),
            paint(r.temperature_status, &temp, tty),
            units::size(r.capacity_bytes),
            r.model,
            r.mounts.join(" ")
        );
    }
    let notes: Vec<_> = reports.iter().filter(|r| r.source == "sysfs").collect();
    if !notes.is_empty() {
        eprintln!();
        for r in notes {
            eprintln!("{}: {}", r.name, r.notes.join("; "));
        }
    }
    Ok(())
}

/// One disk as a `DiskInfo.txt` block.
pub fn render(r: &DiskReport, index: usize) -> String {
    let mut s = String::new();
    let line = "-".repeat(76);
    let mut kv = |k: &str, v: String| s.push_str(&format!("{k:>16} : {v}\n"));
    let rotation = match (r.is_ssd, r.rotation_rate) {
        (true, _) => "---- (SSD)".to_string(),
        (false, Some(rpm)) => format!("{rpm} RPM"),
        (false, None) => "----".to_string(),
    };
    let temperature = r
        .temperature_c
        .map_or("----".into(), |t| format!("{t} C ({} F)", t * 9 / 5 + 32));
    kv("Model", r.model.clone());
    kv("Firmware", r.firmware.clone());
    kv("Serial Number", r.serial.clone());
    kv("Disk Size", units::size(r.capacity_bytes));
    kv("Rotation Rate", rotation);
    kv("Interface", r.interface.clone());
    kv("Standard", r.standard.clone());
    kv("Transfer Mode", r.transfer_mode.clone());
    kv(
        "Power On Hours",
        r.power_on_hours
            .map_or("----".into(), |h| format!("{h} hours")),
    );
    kv(
        "Power On Count",
        r.power_on_count
            .map_or("----".into(), |c| format!("{c} count")),
    );
    if r.host_reads_gb.is_some() {
        kv("Host Reads", units::gb(r.host_reads_gb));
    }
    if r.host_writes_gb.is_some() {
        kv("Host Writes", units::gb(r.host_writes_gb));
    }
    if r.nand_writes_gb.is_some() {
        kv("NAND Writes", units::gb(r.nand_writes_gb));
    }
    kv("Temperature", temperature);
    kv("Health Status", health_text(r));
    kv("Features", r.features.join(", "));
    kv("Mount Points", r.mounts.join(" "));
    let vendor = if r.vendor.is_empty() {
        String::new()
    } else {
        format!("{}, ", r.vendor)
    };
    kv("Device", format!("{} ({vendor}{})", r.path, r.source));

    let mut out = format!("{line}\n ({index}) {}\n{line}\n{s}", r.model);
    for n in &r.notes {
        out.push_str(&format!("{:>16} : {n}\n", "Note"));
    }
    if r.attributes.is_empty() {
        return out;
    }
    out.push_str(&format!("-- S.M.A.R.T. {}\n", "-".repeat(62)));
    if r.protocol == Protocol::Nvme {
        out.push_str("ID RawValues(6) Attribute Name\n");
        for a in &r.attributes {
            out.push_str(&format!("{:02X} {} {}\n", a.id, a.raw_hex, a.name));
        }
    } else {
        out.push_str("ID Cur Wor Thr RawValues(6) Attribute Name\n");
        for a in &r.attributes {
            let thr = a.threshold.map_or("---".into(), |t| format!("{t:_>3}"));
            out.push_str(&format!(
                "{:02X} {:_>3} {:_>3} {} {} {}\n",
                a.id,
                a.current.unwrap_or(0),
                a.worst.unwrap_or(0),
                thr,
                a.raw_hex,
                a.name
            ));
        }
    }
    out
}

pub fn show(reports: &[DiskReport], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(reports)?);
        return Ok(());
    }
    let mut out = std::io::stdout().lock();
    for (i, r) in reports.iter().enumerate() {
        writeln!(out, "{}", render(r, i + 1))?;
    }
    Ok(())
}

/// Worst health of the disks as an exit status (Nagios-like: 0, 1, 2, 3).
pub fn check(reports: &[DiskReport], quiet: bool) -> i32 {
    let worst = reports
        .iter()
        .map(|r| r.health)
        .max_by_key(|h| match h {
            Health::Good => 0,
            Health::Unknown => 1,
            Health::Caution => 2,
            Health::Bad => 3,
        })
        .unwrap_or(Health::Unknown);
    if !quiet {
        for r in reports {
            println!("{}: {} {}", r.name, health_text(r), r.model);
        }
    }
    match worst {
        Health::Good => 0,
        Health::Caution => 1,
        Health::Bad => 2,
        Health::Unknown => 3,
    }
}

/// One JSON array per line. Stops when the reader goes away (the GUI closed).
pub fn stream(opts: &ScanOptions, interval: u64) -> Result<()> {
    let mut out = std::io::stdout().lock();
    loop {
        let line = serde_json::to_string(&disklens::scan::scan(opts))?;
        if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(interval.max(1)));
    }
}

pub fn dump(opts: &ScanOptions, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    for d in disklens::device::discover() {
        let dump = disklens::dump::capture(&d, opts.backend);
        let file = out.join(format!("{}.json", d.name));
        std::fs::write(&file, serde_json::to_string_pretty(&dump)?)?;
        println!("{} -> {}", d.path, file.display());
    }
    Ok(())
}
