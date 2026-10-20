//! Terminal interface, laid out like CrystalDiskInfo's window: one tab per disk (coloured by
//! health), the health and temperature boxes, the disk information and the S.M.A.R.T. table.
//!
//! The palette and the key layout follow animeav1-dl. Scans run on a background thread, so
//! a slow disk never freezes the interface.

use std::io::IsTerminal;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table, TableState, Wrap};
use ratatui::Frame;

use disklens::scan::{self, History, ScanOptions};
use disklens::smart::{DiskReport, Health, Protocol};
use disklens::units;

mod theme {
    use ratatui::style::{Color, Modifier, Style};

    pub const BG: Color = Color::Rgb(13, 17, 23);
    pub const BORDER: Color = Color::Rgb(48, 54, 61);
    pub const TEXT: Color = Color::Rgb(230, 237, 243);
    pub const DIM: Color = Color::Rgb(125, 133, 144);
    pub const ACCENT: Color = Color::Rgb(45, 212, 191);
    pub const GOOD: Color = Color::Rgb(56, 139, 253);
    pub const CAUTION: Color = Color::Rgb(210, 153, 34);
    pub const BAD: Color = Color::Rgb(248, 81, 73);

    pub fn base() -> Style {
        Style::default().fg(TEXT).bg(BG)
    }
    pub fn dim() -> Style {
        Style::default().fg(DIM)
    }
    pub fn title() -> Style {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    }
    pub fn border(active: bool) -> Style {
        Style::default().fg(if active { ACCENT } else { BORDER })
    }
}

/// CrystalDiskInfo's colours: blue good, yellow caution, red bad, grey unknown.
fn health_color(h: Health) -> Color {
    match h {
        Health::Good => theme::GOOD,
        Health::Caution => theme::CAUTION,
        Health::Bad => theme::BAD,
        Health::Unknown => theme::DIM,
    }
}

struct App {
    reports: Vec<DiskReport>,
    history: History,
    selected: usize,
    table: TableState,
    raw_decimal: bool,
    help: bool,
    scanning: bool,
    last_scan: Option<chrono::DateTime<chrono::Local>>,
    quit: bool,
    opts: ScanOptions,
}

pub fn run(opts: ScanOptions, interval: u64) -> Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("the terminal interface needs an interactive terminal; try `disklens list`");
    }
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, opts, interval.max(1));
    ratatui::restore();
    result
}

fn spawn_scan(opts: &ScanOptions, tx: &mpsc::Sender<Vec<DiskReport>>) {
    let (opts, tx) = (opts.clone(), tx.clone());
    std::thread::spawn(move || {
        let _ = tx.send(scan::scan(&opts));
    });
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    opts: ScanOptions,
    interval: u64,
) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut app = App {
        reports: Vec::new(),
        history: History::new(720),
        selected: 0,
        table: TableState::default().with_selected(Some(0)),
        raw_decimal: false,
        help: false,
        scanning: true,
        last_scan: None,
        quit: false,
        opts,
    };
    spawn_scan(&app.opts, &tx);
    let period = Duration::from_secs(interval);
    let mut next = Instant::now() + period;

    while !app.quit {
        terminal.draw(|f| draw(f, &mut app))?;
        if let Ok(reports) = rx.try_recv() {
            app.history.record(&reports);
            app.reports = reports;
            app.selected = app.selected.min(app.reports.len().saturating_sub(1));
            app.scanning = false;
            app.last_scan = Some(chrono::Local::now());
        }
        if Instant::now() >= next && !app.scanning {
            app.scanning = true;
            spawn_scan(&app.opts, &tx);
            next = Instant::now() + period;
        }
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    key(&mut app, k.code, k.modifiers, &tx);
                }
            }
        }
    }
    Ok(())
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers, tx: &mpsc::Sender<Vec<DiskReport>>) {
    let n = app.reports.len().max(1);
    let rows = app
        .reports
        .get(app.selected)
        .map_or(0, |r| r.attributes.len());
    if app.help {
        app.help = false;
        return;
    }
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.quit = true,
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
            app.selected = (app.selected + 1) % n;
            app.table.select(Some(0));
        }
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
            app.selected = (app.selected + n - 1) % n;
            app.table.select(Some(0));
        }
        KeyCode::Char(c @ '1'..='9') => {
            let i = c as usize - '1' as usize;
            if i < app.reports.len() {
                app.selected = i;
                app.table.select(Some(0));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let i = app.table.selected().unwrap_or(0);
            app.table.select(Some((i + 1).min(rows.saturating_sub(1))));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let i = app.table.selected().unwrap_or(0);
            app.table.select(Some(i.saturating_sub(1)));
        }
        KeyCode::PageDown => {
            let i = app.table.selected().unwrap_or(0);
            app.table.select(Some((i + 10).min(rows.saturating_sub(1))));
        }
        KeyCode::PageUp => {
            let i = app.table.selected().unwrap_or(0);
            app.table.select(Some(i.saturating_sub(10)));
        }
        KeyCode::Char('r') | KeyCode::F(5) if !app.scanning => {
            app.scanning = true;
            spawn_scan(&app.opts, tx);
        }
        KeyCode::Char('d') => app.raw_decimal = !app.raw_decimal,
        KeyCode::Char('?') => app.help = true,
        _ => {}
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Block::default().style(theme::base()), f.area());
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(f.area());
    header(f, zones[0], app);
    tabs(f, zones[1], app);
    if app.help {
        help(f, zones[2]);
    } else if let Some(r) = app.reports.get(app.selected).cloned() {
        body(f, zones[2], app, &r);
    } else {
        let msg = if app.scanning {
            "Reading disks..."
        } else {
            "No disks found."
        };
        f.render_widget(Paragraph::new(msg).style(theme::dim()), zones[2]);
    }
    footer(f, zones[3]);
}

fn header(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled(" DiskLens ", theme::title()),
        Span::styled(format!("v{} ", env!("CARGO_PKG_VERSION")), theme::dim()),
    ];
    let state = if app.scanning {
        "  refreshing...".to_string()
    } else {
        app.last_scan
            .map(|t| format!("  updated {}", t.format("%H:%M:%S")))
            .unwrap_or_default()
    };
    spans.push(Span::styled(state, theme::dim()));
    if !disklens::device::privileged() && app.opts.from.is_empty() {
        spans.push(Span::styled(
            "  not root: SMART data may be missing (run with sudo)",
            Style::default().fg(theme::CAUTION),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn tabs(f: &mut Frame, area: Rect, app: &App) {
    if app.reports.is_empty() {
        return;
    }
    let width = (area.width / app.reports.len() as u16).clamp(14, 30);
    let constraints: Vec<Constraint> = app
        .reports
        .iter()
        .map(|_| Constraint::Length(width))
        .collect();
    let cells = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (i, (r, cell)) in app.reports.iter().zip(cells.iter()).enumerate() {
        let c = health_color(r.health);
        let sel = i == app.selected;
        let lines = vec![Line::from(vec![
            Span::styled(
                r.health.label(),
                Style::default().fg(c).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                units::temperature(r.temperature_c),
                Style::default().fg(health_color(r.temperature_status)),
            ),
            Span::styled(format!(" {}", r.name), theme::dim()),
        ])];
        let block = Block::default().borders(Borders::ALL).border_style(if sel {
            Style::default().fg(c)
        } else {
            theme::border(false)
        });
        f.render_widget(Paragraph::new(lines).block(block), *cell);
    }
}

fn body(f: &mut Frame, area: Rect, app: &mut App, r: &DiskReport) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(13), Constraint::Min(5)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(40)])
        .split(rows[0]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Min(3),
        ])
        .split(top[0]);

    // Health box.
    let c = health_color(r.health);
    let life = r.life_percent.map(|l| format!("{l} %")).unwrap_or_default();
    let health = Paragraph::new(vec![
        Line::from(Span::styled(
            r.health.label().to_uppercase(),
            Style::default()
                .fg(Color::Black)
                .bg(c)
                .add_modifier(Modifier::BOLD),
        ))
        .centered(),
        Line::from(Span::styled(
            life,
            Style::default().fg(c).add_modifier(Modifier::BOLD),
        ))
        .centered(),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Health Status ")
            .border_style(Style::default().fg(c)),
    );
    f.render_widget(health, left[0]);

    let tc = health_color(r.temperature_status);
    let temp = Paragraph::new(Line::from(Span::styled(
        units::temperature(r.temperature_c),
        Style::default().fg(tc).add_modifier(Modifier::BOLD),
    )))
    .centered()
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Temperature ")
            .border_style(Style::default().fg(tc)),
    );
    f.render_widget(temp, left[1]);

    let samples: Vec<u64> = app
        .history
        .of(r)
        .iter()
        .map(|s| s.temperature_c.unwrap_or(0).max(0) as u64)
        .collect();
    let spark = Sparkline::default()
        .data(&samples)
        .style(Style::default().fg(tc))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" History ")
                .border_style(theme::border(false)),
        );
    f.render_widget(spark, left[2]);

    // Information panel.
    let kv = |k: &str, v: String| {
        Line::from(vec![
            Span::styled(format!("{k:>15} "), theme::dim()),
            Span::styled(v, Style::default().fg(theme::TEXT)),
        ])
    };
    let rotation = match (r.is_ssd, r.rotation_rate) {
        (true, _) => "---- (SSD)".into(),
        (false, Some(rpm)) => format!("{rpm} RPM"),
        _ => "----".into(),
    };
    let info = vec![
        kv("Firmware", r.firmware.clone()),
        kv("Serial Number", r.serial.clone()),
        kv("Interface", r.interface.clone()),
        kv("Transfer Mode", r.transfer_mode.clone()),
        kv("Standard", r.standard.clone()),
        kv("Features", r.features.join(", ")),
        kv("Mount Points", r.mounts.join(" ")),
        kv("Host Reads", units::gb(r.host_reads_gb)),
        kv("Host Writes", units::gb(r.host_writes_gb)),
        kv("Rotation Rate", rotation),
        kv("Power On Count", units::count(r.power_on_count)),
        kv("Power On Hours", units::hours(r.power_on_hours)),
    ];
    let title = format!(" {} : {} ", r.model, units::size(r.capacity_bytes));
    let info = Paragraph::new(info).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::title()))
            .border_style(theme::border(true)),
    );
    f.render_widget(info, top[1]);

    attributes(f, rows[1], app, r);
}

fn attributes(f: &mut Frame, area: Rect, app: &mut App, r: &DiskReport) {
    let nvme = r.protocol == Protocol::Nvme;
    if r.attributes.is_empty() {
        let mut lines = vec![Line::from(Span::styled(
            "No S.M.A.R.T. data.",
            theme::dim(),
        ))];
        lines.extend(
            r.notes
                .iter()
                .map(|n| Line::from(Span::styled(n.clone(), Style::default().fg(theme::CAUTION)))),
        );
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" S.M.A.R.T. ")
                    .border_style(theme::border(false)),
            ),
            area,
        );
        return;
    }
    let header = if nvme {
        Row::new(["", "ID", "Attribute Name", "Raw Values"])
    } else {
        Row::new([
            "",
            "ID",
            "Attribute Name",
            "Current",
            "Worst",
            "Threshold",
            "Raw Values",
        ])
    }
    .style(theme::dim());
    let rows: Vec<Row> = r
        .attributes
        .iter()
        .map(|a| {
            let dot = Cell::from("●").style(Style::default().fg(health_color(a.status)));
            let raw = if app.raw_decimal {
                a.raw.to_string()
            } else {
                a.raw_hex.clone()
            };
            if nvme {
                Row::new(vec![
                    dot,
                    Cell::from(format!("{:02X}", a.id)),
                    Cell::from(a.name.clone()),
                    Cell::from(raw),
                ])
            } else {
                let n = |v: Option<u8>| v.map_or("---".into(), |x| x.to_string());
                Row::new(vec![
                    dot,
                    Cell::from(format!("{:02X}", a.id)),
                    Cell::from(a.name.clone()),
                    Cell::from(n(a.current)),
                    Cell::from(n(a.worst)),
                    Cell::from(n(a.threshold)),
                    Cell::from(raw),
                ])
            }
        })
        .collect();
    let widths: Vec<Constraint> = if nvme {
        vec![
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(30),
            Constraint::Length(20),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(28),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(16),
        ]
    };
    let raw_mode = if app.raw_decimal { "decimal" } else { "hex" };
    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().fg(theme::BG).bg(theme::ACCENT))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" S.M.A.R.T. ({}, raw {raw_mode}) ", r.smart_table))
                .border_style(theme::border(false)),
        );
    f.render_stateful_widget(table, area, &mut app.table);
}

fn footer(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("←/→ tab  disk   1-9 jump   ↑/↓ attributes   d raw hex/dec   r refresh   ? help   q quit")
            .style(theme::dim()),
        area,
    );
}

fn help(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled("DiskLens", theme::title())),
        Line::from(""),
        Line::from("Health follows CrystalDiskInfo 9.9.3:"),
        Line::from("  Bad      an attribute is below its threshold, or an SSD's life is 0 %"),
        Line::from("  Caution  reallocated / pending / uncorrectable sectors (HDD), or life at or below 10 %"),
        Line::from("  Good     neither of the above"),
        Line::from("  Unknown  the disk returned no usable SMART data"),
        Line::from(""),
        Line::from("Thresholds: --life-caution, --sector-caution, --alarm-hdd, --alarm-ssd."),
        Line::from("Backends: --backend native | smartctl | auto (default)."),
        Line::from(""),
        Line::from(Span::styled("Any key to close.", theme::dim())),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::border(true)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn draws_a_captured_disk() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/captures");
        let opts = ScanOptions {
            from: vec![dir],
            ..Default::default()
        };
        let reports = scan::scan(&opts);
        assert!(!reports.is_empty());
        let mut app = App {
            history: History::new(10),
            reports,
            selected: 0,
            table: TableState::default().with_selected(Some(0)),
            raw_decimal: false,
            help: false,
            scanning: false,
            last_scan: None,
            quit: false,
            opts,
        };
        app.history.record(&app.reports);
        let mut term = ratatui::Terminal::new(TestBackend::new(140, 45)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let text: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        for needle in [
            "Health Status",
            "Temperature",
            "S.M.A.R.T.",
            "Power On Hours",
            "GOOD",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
        assert!(
            !text.contains("not root"),
            "no root banner when reading dumps"
        );
    }
}
