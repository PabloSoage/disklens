//! Regression tests on real disks: every file in `tests/captures/` must give the values in
//! `tests/captures/expected.json`, which come from smartctl or from an independent decoder
//! (`tools/expected.py`), never from disklens itself.

use std::path::Path;

use disklens::dump::Dump;
use disklens::smart::{Health, Thresholds};
use serde_json::Value;

#[test]
fn captures_match_independent_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/captures");
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    let expected = expected.as_object().unwrap();
    assert!(expected.len() >= 7, "expected.json lost entries");

    let mut checked = 0;
    for (file, exp) in expected {
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        let dump: Dump = serde_json::from_str(&text).unwrap();
        let r = dump
            .analyze(&Thresholds::default())
            .unwrap_or_else(|| panic!("{file}: no report"));

        let int = |k: &str| exp.get(k).and_then(Value::as_i64);
        if let Some(t) = int("temperature_c") {
            assert_eq!(r.temperature_c, Some(t as i32), "{file}: temperature");
        }
        if let Some(h) = int("power_on_hours") {
            assert_eq!(r.power_on_hours, Some(h), "{file}: power-on hours");
        }
        if let Some(c) = int("power_on_count") {
            assert_eq!(r.power_on_count, Some(c), "{file}: power-on count");
        }
        if let Some(l) = int("life_percent") {
            assert_eq!(r.life_percent, Some(l as i32), "{file}: life");
        }
        if let Some(w) = int("host_writes_gb") {
            assert_eq!(r.host_writes_gb, Some(w), "{file}: host writes");
        }
        if let Some(w) = int("host_reads_gb") {
            assert_eq!(r.host_reads_gb, Some(w), "{file}: host reads");
        }
        // All captured disks are healthy; smartctl agrees where it was run.
        assert_eq!(r.health, Health::Good, "{file}: health");
        assert!(
            r.serial.starts_with("SERIAL"),
            "{file}: serial not redacted"
        );
        assert!(!r.attributes.is_empty(), "{file}: no attributes");
        checked += 1;
    }
    assert_eq!(checked, expected.len());
}
