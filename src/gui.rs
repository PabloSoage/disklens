//! Desktop window (egui), laid out like CrystalDiskInfo: disk buttons coloured by health,
//! the health and temperature boxes, the information grid and the S.M.A.R.T. table.
//!
//! Reading SMART needs root. When the window runs as a normal user it can start
//! `pkexec disklens stream`, a privileged helper that prints one JSON scan per line, so the
//! password is asked once and the window itself never runs as root.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};

use disklens::scan::{self, History, ScanOptions};
use disklens::smart::{DiskReport, Health, Protocol};
use disklens::units;

const GOOD: Color32 = Color32::from_rgb(56, 139, 253);
const CAUTION: Color32 = Color32::from_rgb(230, 170, 20);
const BAD: Color32 = Color32::from_rgb(232, 64, 64);
const UNKNOWN: Color32 = Color32::from_rgb(128, 128, 128);

fn health_color(h: Health) -> Color32 {
    match h {
        Health::Good => GOOD,
        Health::Caution => CAUTION,
        Health::Bad => BAD,
        Health::Unknown => UNKNOWN,
    }
}

enum Msg {
    Reports(Vec<DiskReport>),
    HelperFailed(String),
}

struct App {
    opts: ScanOptions,
    interval: u64,
    reports: Vec<DiskReport>,
    history: History,
    selected: usize,
    /// Model + serial of the selected disk, so the selection survives reordering.
    selected_key: Option<String>,
    raw_decimal: bool,
    scanning: bool,
    next_scan: Instant,
    last_scan: Option<chrono::DateTime<chrono::Local>>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    helper: Option<Child>,
    status: String,
    show_options: bool,
    show_about: bool,
}

pub fn run(opts: ScanOptions, interval: u64) -> Result<()> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DiskLens")
            .with_app_id("io.github.pablosoage.disklens")
            .with_inner_size([860.0, 720.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    let (tx, rx) = mpsc::channel();
    let app = App {
        opts,
        interval: interval.max(1),
        reports: Vec::new(),
        history: History::new(720),
        selected: 0,
        selected_key: None,
        raw_decimal: false,
        scanning: false,
        next_scan: Instant::now(),
        last_scan: None,
        tx,
        rx,
        helper: None,
        status: String::new(),
        show_options: false,
        show_about: false,
    };
    eframe::run_native("DiskLens", native, Box::new(|_cc| Ok(Box::new(app))))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

impl App {
    fn scan_now(&mut self, ctx: &egui::Context) {
        if self.scanning || self.helper.is_some() {
            return;
        }
        self.scanning = true;
        let (opts, tx, ctx) = (self.opts.clone(), self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Reports(scan::scan(&opts)));
            ctx.request_repaint();
        });
    }

    /// Start the privileged helper once; it keeps sending scans until the window closes.
    fn elevate(&mut self, ctx: &egui::Context) {
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("cannot find the executable: {e}");
                return;
            }
        };
        let backend = format!("{:?}", self.opts.backend).to_lowercase();
        let child = Command::new("pkexec")
            .arg(exe)
            .args([
                "--backend",
                &backend,
                "stream",
                "--interval",
                &self.interval.to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("pkexec: {e}");
                return;
            }
        };
        let out = child.stdout.take().expect("piped stdout");
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<Vec<DiskReport>>(&line) {
                    Ok(r) => {
                        let _ = tx.send(Msg::Reports(r));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::HelperFailed(format!("helper output: {e}")));
                    }
                }
                ctx.request_repaint();
            }
            let _ = tx.send(Msg::HelperFailed("the privileged helper stopped".into()));
            ctx.request_repaint();
        });
        self.helper = Some(child);
        self.status = "Waiting for authentication...".into();
    }

    fn receive(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Reports(r) => {
                    self.history.record(&r);
                    self.reports = r;
                    let by_key = self
                        .selected_key
                        .as_ref()
                        .and_then(|k| self.reports.iter().position(|d| scan::disk_key(d) == *k));
                    self.selected = by_key
                        .unwrap_or(self.selected)
                        .min(self.reports.len().saturating_sub(1));
                    self.scanning = false;
                    self.last_scan = Some(chrono::Local::now());
                    if self.helper.is_some() {
                        self.status = "Running with administrator rights (pkexec).".into();
                    }
                }
                Msg::HelperFailed(e) => {
                    if let Some(mut c) = self.helper.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    self.status = e;
                }
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(mut c) = self.helper.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive();
        if self.helper.is_none() && Instant::now() >= self.next_scan {
            self.scan_now(&ctx);
            self.next_scan = Instant::now() + Duration::from_secs(self.interval);
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.next_scan = Instant::now();
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui, &ctx));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui, &ctx));
        egui::Panel::top("disks").show(ui, |ui| self.disk_buttons(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(r) = self.reports.get(self.selected).cloned() {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.disk(ui, &r));
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(if self.scanning {
                        "Reading disks..."
                    } else {
                        "No disks found."
                    });
                });
            }
        });
        self.windows(&ctx);
    }
}

impl App {
    fn menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Refresh (F5)").clicked() {
                    self.next_scan = Instant::now();
                }
                if ui.button("Copy report").clicked() {
                    let text: String = self
                        .reports
                        .iter()
                        .enumerate()
                        .map(|(i, r)| crate::cli::render(r, i + 1))
                        .collect::<Vec<_>>()
                        .join("\n");
                    ctx.copy_text(text);
                }
                if ui.button("Copy JSON").clicked() {
                    ctx.copy_text(serde_json::to_string_pretty(&self.reports).unwrap_or_default());
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Function", |ui| {
                ui.checkbox(&mut self.raw_decimal, "Raw values in decimal");
                if ui.button("Options...").clicked() {
                    self.show_options = true;
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    self.show_about = true;
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if !disklens::device::privileged() && self.helper.is_none() && self.opts.from.is_empty()
            {
                ui.colored_label(CAUTION, "Not running as root: SMART data may be missing.");
                if ui.button("Read as administrator").clicked() {
                    self.elevate(ctx);
                }
            }
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(t) = self.last_scan {
                    ui.weak(format!("Updated {}", t.format("%H:%M:%S")));
                }
                if self.scanning {
                    ui.spinner();
                }
            });
        });
    }

    fn disk_buttons(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (i, r) in self.reports.iter().enumerate() {
                let c = health_color(r.health);
                let text = format!(
                    "{}\n{}\n{}",
                    r.health.label(),
                    units::temperature(r.temperature_c),
                    r.name
                );
                let selected = i == self.selected;
                let button = egui::Button::new(RichText::new(text).color(if selected {
                    Color32::WHITE
                } else {
                    c
                }))
                .fill(if selected { c } else { Color32::TRANSPARENT })
                .stroke(Stroke::new(1.5, c))
                .min_size(Vec2::new(86.0, 52.0));
                if ui.add(button).on_hover_text(&r.model).clicked() {
                    self.selected = i;
                    self.selected_key = Some(scan::disk_key(r));
                }
            }
        });
        ui.add_space(4.0);
    }

    fn disk(&mut self, ui: &mut egui::Ui, r: &DiskReport) {
        ui.vertical_centered(|ui| {
            ui.heading(format!("{} : {}", r.model, units::size(r.capacity_bytes)));
        });
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(150.0);
                status_box(
                    ui,
                    "Health Status",
                    r.health.label(),
                    r.life_percent.map(|l| format!("{l} %")),
                    r.health,
                );
                ui.add_space(6.0);
                status_box(
                    ui,
                    "Temperature",
                    &units::temperature(r.temperature_c),
                    None,
                    r.temperature_status,
                );
                ui.add_space(6.0);
                self.temperature_plot(ui, r);
            });
            ui.add_space(10.0);
            egui::Grid::new("info")
                .num_columns(2)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    let rotation = match (r.is_ssd, r.rotation_rate) {
                        (true, _) => "---- (SSD)".to_string(),
                        (false, Some(rpm)) => format!("{rpm} RPM"),
                        _ => "----".to_string(),
                    };
                    let rows: [(&str, String); 13] = [
                        ("Firmware", r.firmware.clone()),
                        ("Serial Number", r.serial.clone()),
                        ("Interface", r.interface.clone()),
                        ("Transfer Mode", r.transfer_mode.clone()),
                        ("Mount Points", r.mounts.join("  ")),
                        ("Standard", r.standard.clone()),
                        ("Features", r.features.join(", ")),
                        ("Host Reads", units::gb(r.host_reads_gb)),
                        ("Host Writes", units::gb(r.host_writes_gb)),
                        ("NAND Writes", units::gb(r.nand_writes_gb)),
                        ("Rotation Rate", rotation),
                        ("Power On Count", units::count(r.power_on_count)),
                        ("Power On Hours", units::hours(r.power_on_hours)),
                    ];
                    for (k, v) in rows {
                        ui.label(RichText::new(k).weak());
                        ui.add(egui::Label::new(v).wrap());
                        ui.end_row();
                    }
                });
        });
        for n in &r.notes {
            ui.colored_label(CAUTION, n);
        }
        ui.add_space(8.0);
        self.attribute_table(ui, r);
    }

    fn temperature_plot(&self, ui: &mut egui::Ui, r: &DiskReport) {
        let samples = self.history.of(r);
        let temps: Vec<f32> = samples
            .iter()
            .filter_map(|s| s.temperature_c)
            .map(|t| t as f32)
            .collect();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 50.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, ui.visuals().weak_text_color()),
            egui::StrokeKind::Inside,
        );
        if temps.len() < 2 {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "history",
                egui::FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
            return;
        }
        let (lo, hi) = temps
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &t| (a.min(t), b.max(t)));
        let span = (hi - lo).max(1.0);
        let n = temps.len() as f32 - 1.0;
        let pts: Vec<egui::Pos2> = temps
            .iter()
            .enumerate()
            .map(|(i, t)| {
                egui::pos2(
                    rect.left() + 4.0 + (rect.width() - 8.0) * i as f32 / n,
                    rect.bottom() - 4.0 - (rect.height() - 8.0) * (t - lo) / span,
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            pts,
            Stroke::new(1.5, health_color(r.temperature_status)),
        ));
        painter.text(
            rect.left_top() + Vec2::new(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            format!("{hi:.0}°"),
            egui::FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
        painter.text(
            rect.left_bottom() + Vec2::new(4.0, -2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{lo:.0}°"),
            egui::FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
    }

    fn attribute_table(&self, ui: &mut egui::Ui, r: &DiskReport) {
        if r.attributes.is_empty() {
            ui.weak("No S.M.A.R.T. data.");
            return;
        }
        let nvme = r.protocol == Protocol::Nvme;
        let mut t = TableBuilder::new(ui)
            .vscroll(false)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(18.0))
            .column(Column::exact(28.0))
            .column(Column::remainder().at_least(140.0).clip(true));
        if !nvme {
            t = t.columns(Column::exact(56.0), 3);
        }
        t = t.column(Column::exact(130.0));
        t.header(20.0, |mut h| {
            h.col(|_| {});
            h.col(|ui| {
                ui.strong("ID");
            });
            h.col(|ui| {
                ui.strong("Attribute Name");
            });
            if !nvme {
                for name in ["Current", "Worst", "Threshold"] {
                    h.col(|ui| {
                        ui.strong(name);
                    });
                }
            }
            h.col(|ui| {
                ui.strong("Raw Values");
            });
        })
        .body(|mut body| {
            for a in &r.attributes {
                body.row(18.0, |mut row| {
                    row.col(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 5.0, health_color(a.status));
                    });
                    row.col(|ui| {
                        ui.monospace(format!("{:02X}", a.id));
                    });
                    row.col(|ui| {
                        ui.label(&a.name);
                    });
                    if !nvme {
                        for v in [a.current, a.worst, a.threshold] {
                            row.col(|ui| {
                                ui.monospace(v.map_or("---".into(), |x| x.to_string()));
                            });
                        }
                    }
                    row.col(|ui| {
                        ui.monospace(if self.raw_decimal {
                            a.raw.to_string()
                        } else {
                            a.raw_hex.clone()
                        });
                    });
                });
            }
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        let mut open = self.show_options;
        egui::Window::new("Options")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let t = &mut self.opts.thresholds;
                egui::Grid::new("opts").num_columns(2).show(ui, |ui| {
                    ui.label("Life caution (%)");
                    ui.add(egui::DragValue::new(&mut t.life).range(0..=100));
                    ui.end_row();
                    ui.label("Sector caution (raw count)");
                    let mut s = t.reallocated;
                    if ui
                        .add(egui::DragValue::new(&mut s).range(0..=65535))
                        .changed()
                    {
                        t.reallocated = s;
                        t.pending = s;
                        t.uncorrectable = s;
                    }
                    ui.end_row();
                    ui.label("Alarm temperature HDD (°C)");
                    ui.add(egui::DragValue::new(&mut t.alarm_hdd).range(20..=90));
                    ui.end_row();
                    ui.label("Alarm temperature SSD (°C)");
                    ui.add(egui::DragValue::new(&mut t.alarm_ssd).range(20..=100));
                    ui.end_row();
                    ui.label("Refresh interval (s)");
                    ui.add(egui::DragValue::new(&mut self.interval).range(1..=3600));
                    ui.end_row();
                });
                if ui.button("Apply and refresh").clicked() {
                    self.next_scan = Instant::now();
                }
            });
        self.show_options = open;

        let mut about = self.show_about;
        egui::Window::new("About DiskLens")
            .open(&mut about)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("DiskLens {}", env!("CARGO_PKG_VERSION")));
                ui.label("Disk health for Linux.");
                ui.label("Health evaluation and attribute names ported from CrystalDiskInfo 9.9.3");
                ui.label("(C) 2008-2025 hiyohiyo, (C) 2025 CrystalMark Inc., MIT License.");
            });
        self.show_about = about;
    }
}

fn status_box(ui: &mut egui::Ui, title: &str, value: &str, sub: Option<String>, h: Health) {
    let c = health_color(h);
    egui::Frame::new()
        .fill(c)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(134.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(title).color(Color32::WHITE).small());
                ui.label(
                    RichText::new(value)
                        .color(Color32::WHITE)
                        .size(22.0)
                        .strong(),
                );
                if let Some(s) = sub {
                    ui.label(RichText::new(s).color(Color32::WHITE).size(16.0));
                }
            });
        });
}
