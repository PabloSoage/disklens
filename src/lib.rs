//! DiskLens: disk health for Linux with the CrystalDiskInfo evaluation.
//!
//! - [`smart`]: the analysis (vendor detection, derived values, health), OS-independent.
//! - [`device`]: discovery and raw reads on Linux (SG_IO ATA pass-through, NVMe admin
//!   ioctls), plus a `smartctl` fallback.
//! - [`scan`]: puts both together and keeps a temperature history.

pub mod device;
pub mod dump;
pub mod scan;
pub mod smart;
pub mod units;
