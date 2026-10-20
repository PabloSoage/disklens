use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use disklens::device::Backend;
use disklens::scan::{self, ScanOptions};
use disklens::smart::Thresholds;

mod cli;
#[cfg(feature = "gui")]
mod gui;
mod tui;
#[cfg(feature = "web")]
mod web;

#[derive(Parser)]
#[command(
    name = "disklens",
    version,
    about = "Disk health (S.M.A.R.T.) for Linux, with the CrystalDiskInfo evaluation",
    long_about = "Disk health (S.M.A.R.T.) for Linux, with the CrystalDiskInfo evaluation.\n\n\
        Without a subcommand it opens the terminal interface (or prints the summary when the \
        output is not a terminal). Reading SMART data needs root."
)]
struct Args {
    /// Analyse `disklens dump` files (or directories of them) instead of this machine's disks.
    #[arg(long, global = true, value_name = "PATH")]
    from: Vec<PathBuf>,

    /// How to read SMART data.
    #[arg(long, value_enum, default_value_t = Backend::Auto, global = true)]
    backend: Backend,

    /// Remaining life (%) at or below which an SSD is "Caution".
    #[arg(long, default_value_t = 10, global = true)]
    life_caution: i32,

    /// Reallocated / pending / uncorrectable sectors that make an HDD "Caution".
    #[arg(long, default_value_t = 1, global = true)]
    sector_caution: u16,

    /// Alarm temperature for HDDs, in °C.
    #[arg(long, default_value_t = 50, global = true)]
    alarm_hdd: i32,

    /// Alarm temperature for SSDs, in °C.
    #[arg(long, default_value_t = 60, global = true)]
    alarm_ssd: i32,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// One line per disk: health, temperature, life, model.
    List {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// The full report of some disks (all when none is given), like CrystalDiskInfo's window.
    Show {
        /// Kernel names or paths: `sda`, `/dev/nvme0n1`.
        disks: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Exit status for scripts and monitoring: 0 good, 1 caution, 2 bad, 3 unknown.
    Check {
        disks: Vec<String>,
        /// Print nothing, only set the exit status.
        #[arg(short, long)]
        quiet: bool,
    },
    /// Interactive terminal interface.
    Tui {
        /// Seconds between refreshes.
        #[arg(long, default_value_t = 10)]
        interval: u64,
    },
    /// Desktop window.
    #[cfg(feature = "gui")]
    Gui {
        #[arg(long, default_value_t = 10)]
        interval: u64,
    },
    /// Web interface and JSON API, for headless servers.
    #[cfg(feature = "web")]
    Web {
        /// Address to listen on. Use 0.0.0.0:8086 to reach it from other machines.
        #[arg(long, default_value = "127.0.0.1:8086")]
        bind: String,
        /// Seconds between refreshes.
        #[arg(long, default_value_t = 60)]
        interval: u64,
        /// Require `Authorization: Bearer <token>` (or `?token=`); also read from
        /// DISKLENS_TOKEN.
        #[arg(long, env = "DISKLENS_TOKEN")]
        token: Option<String>,
        /// Samples of temperature history kept per disk.
        #[arg(long, default_value_t = 1440)]
        history: usize,
    },
    /// Print one JSON scan per line, forever (the privileged helper of the GUI).
    #[command(hide = true)]
    Stream {
        #[arg(long, default_value_t = 10)]
        interval: u64,
    },
    /// Save the raw pages (IDENTIFY, SMART data, thresholds, NVMe logs) of every disk, to
    /// report a misdetected disk or to add a test case.
    Dump {
        /// Directory for one JSON file per disk.
        #[arg(long, default_value = ".")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let opts = ScanOptions {
        backend: args.backend,
        thresholds: Thresholds {
            reallocated: args.sector_caution,
            pending: args.sector_caution,
            uncorrectable: args.sector_caution,
            life: args.life_caution,
            alarm_hdd: args.alarm_hdd,
            alarm_ssd: args.alarm_ssd,
        },
        only: Vec::new(),
        from: args.from,
    };

    match args.cmd {
        None if std::io::stdout().is_terminal() => tui::run(opts, 10),
        None => cli::list(&scan::scan(&opts), false),
        Some(Cmd::List { json }) => cli::list(&scan::scan(&opts), json),
        Some(Cmd::Show { disks, json }) => {
            let opts = ScanOptions {
                only: disks,
                ..opts
            };
            cli::show(&scan::scan(&opts), json)
        }
        Some(Cmd::Check { disks, quiet }) => {
            let opts = ScanOptions {
                only: disks,
                ..opts
            };
            let code = cli::check(&scan::scan(&opts), quiet);
            std::process::exit(code);
        }
        Some(Cmd::Tui { interval }) => tui::run(opts, interval),
        #[cfg(feature = "gui")]
        Some(Cmd::Gui { interval }) => gui::run(opts, interval),
        #[cfg(feature = "web")]
        Some(Cmd::Web {
            bind,
            interval,
            token,
            history,
        }) => web::run(opts, &bind, interval, token, history),
        Some(Cmd::Stream { interval }) => cli::stream(&opts, interval),
        Some(Cmd::Dump { out }) => cli::dump(&opts, &out),
    }
}
