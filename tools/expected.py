"""Build tests/captures/expected.json: the values each capture must produce, from a source
that does not share code with disklens.

- With a `smartctl -j -a` file for the same disk (taken at the same time), the values come
  from smartmontools.
- Without one (NVMe captured on Windows), the SMART / Health log is decoded here from the
  NVMe 1.4 specification (figure 194), independently of src/smart/nvme.rs.

Usage: python -I tools/expected.py <out.json> <capture.json>[=<smartctl.json>] ...
"""
import json
import sys


def le(b, off, n):
    return int.from_bytes(b[off:off + n], "little")


def from_nvme_log(hexlog):
    b = bytes.fromhex(hexlog)
    used = b[5]
    units_written = le(b, 48, 16)
    units_read = le(b, 32, 16)
    return {
        "temperature_c": le(b, 1, 2) - 273,
        "life_percent": max(0, 100 - used),
        "power_on_hours": le(b, 128, 16),
        "power_on_count": le(b, 112, 16),
        # A data unit is 1000 x 512 bytes; CrystalDiskInfo shows GiB (2^30).
        "host_writes_gb": units_written * 512000 // 2**30,
        "host_reads_gb": units_read * 512000 // 2**30,
        "source": "NVMe 1.4 log 0x02 decoded by tools/expected.py",
    }


def from_smartctl(s):
    out = {"source": "smartctl %s" % ".".join(map(str, s["smartctl"]["version"]))}
    if "temperature" in s:
        out["temperature_c"] = s["temperature"]["current"]
    if "power_on_time" in s:
        out["power_on_hours"] = s["power_on_time"]["hours"]
    if "power_cycle_count" in s:
        out["power_on_count"] = s["power_cycle_count"]
    h = s.get("nvme_smart_health_information_log")
    if h:
        out["life_percent"] = max(0, 100 - h["percentage_used"])
        out["host_writes_gb"] = h["data_units_written"] * 512000 // 2**30
        out["host_reads_gb"] = h["data_units_read"] * 512000 // 2**30
    out["smart_passed"] = s.get("smart_status", {}).get("passed")
    return out


def main():
    out_path = sys.argv[1]
    result = {}
    for arg in sys.argv[2:]:
        cap, _, ref = arg.partition("=")
        d = json.load(open(cap, encoding="utf-8-sig"))
        if ref:
            exp = from_smartctl(json.load(open(ref, encoding="utf-8-sig")))
        elif d.get("nvme_smart_log"):
            exp = from_nvme_log(d["nvme_smart_log"])
        else:
            raise SystemExit(f"{cap}: no reference for an ATA capture")
        result[cap.replace("\\", "/").rsplit("/", 1)[-1]] = exp
    json.dump(result, open(out_path, "w", encoding="utf-8", newline="\n"), indent=2, sort_keys=True)
    open(out_path, "a", encoding="utf-8", newline="\n").write("\n")


main()
