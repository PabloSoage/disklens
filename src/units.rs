//! Formatting shared by the front ends.

/// Decimal size as disk vendors count it (1 GB = 10^9 bytes), like CrystalDiskInfo.
pub fn size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e12 {
        format!("{:.1} TB", b / 1e12)
    } else if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else {
        format!("{bytes} B")
    }
}

/// Host reads/writes are kept in GiB-like "GB" units, as CrystalDiskInfo shows them.
pub fn gb(v: Option<i64>) -> String {
    match v {
        None => "----".into(),
        Some(g) if g >= 10_000 => format!("{:.2} TB", g as f64 / 1024.0),
        Some(g) => format!("{g} GB"),
    }
}

pub fn hours(v: Option<i64>) -> String {
    match v {
        None => "----".into(),
        Some(h) => {
            let days = h / 24;
            if days > 0 {
                format!("{h} h ({days} d)")
            } else {
                format!("{h} h")
            }
        }
    }
}

pub fn count(v: Option<i64>) -> String {
    v.map_or("----".into(), |c| format!("{c}"))
}

pub fn temperature(v: Option<i32>) -> String {
    v.map_or("-- °C".into(), |t| format!("{t} °C"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn sizes() {
        assert_eq!(super::size(1_000_204_886_016), "1.0 TB");
        assert_eq!(super::size(500_107_862_016), "500.1 GB");
        assert_eq!(super::gb(Some(20480)), "20.00 TB");
    }
}
