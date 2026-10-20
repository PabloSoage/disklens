"""Compare CrystalDiskInfo's DiskInfo.txt (from `DiskInfo64.exe /CopyExit`) with
`disklens show --json` for the same disks, captured at the same time.

Attribute names are not compared (CrystalDiskInfo writes them in the UI language); ids,
normalised values, thresholds, raw values, health, life, temperature, power-on hours/count
and host/NAND reads and writes are.

Usage: python -I tools/compare_cdi.py <DiskInfo.txt> <disklens.json>
Exit status 0 when everything matches.
"""
import json
import re
import sys

HEALTH = {
    "good": "good", "bueno": "good", "caution": "caution", "precaución": "caution",
    "precaucion": "caution", "bad": "bad", "malo": "bad", "unknown": "unknown",
    "desconocido": "unknown",
}


def first_int(s):
    m = re.search(r"-?\d+", s.replace(".", "").replace(",", ""))
    return int(m.group()) if m else None


def parse_cdi(text):
    disks = []
    blocks = re.split(r"-{70,}\n \(\d+\) .*\n-{70,}\n", text)[1:]
    for b in blocks:
        d = {"attrs": {}}
        for line in b.splitlines():
            m = re.match(r"\s*([A-Za-z ]+?) : (.*)$", line)
            if m and not line.startswith("--"):
                d.setdefault(m.group(1).strip(), m.group(2).strip())
        smart = b.split("-- S.M.A.R.T.", 1)
        if len(smart) == 2:
            for line in smart[1].splitlines()[2:]:
                if line.startswith("--") or not line.strip():
                    break
                f = line.split()
                if re.fullmatch(r"[0-9A-F]{2}", f[0]) and re.fullmatch(r"[0-9A-F]{12}", f[1]):
                    d["attrs"][int(f[0], 16)] = {"raw": f[1]}
                elif re.fullmatch(r"[0-9A-F]{2}", f[0]) and len(f) >= 5:
                    n = lambda v: None if v.startswith("-") else int(v.replace("_", "0") or 0)
                    d["attrs"][int(f[0], 16)] = {
                        "cur": n(f[1]), "wor": n(f[2]), "thr": n(f[3]), "raw": f[4]}
        if "Model" in d:
            disks.append(d)
    return disks


def main():
    cdi = parse_cdi(open(sys.argv[1], encoding="utf-8-sig").read())
    ours = {r["model"]: r for r in json.load(open(sys.argv[2], encoding="utf-8-sig"))}
    total = bad = 0

    def check(model, what, a, b):
        nonlocal total, bad
        total += 1
        if a != b:
            bad += 1
            print(f"  MISMATCH {model}: {what}: CDI={a!r} disklens={b!r}")

    for d in cdi:
        model = d["Model"]
        r = ours.get(model)
        if r is None:
            print(f"{model}: not in the disklens output")
            bad += 1
            continue
        print(f"{model}")
        hs = d.get("Health Status", "")
        word = hs.split()[0].lower() if hs else ""
        check(model, "health", HEALTH.get(word, word), r["health"])
        pct = re.search(r"\((\d+) %\)", hs)
        check(model, "life", int(pct.group(1)) if pct else None, r["life_percent"])
        check(model, "temperature", first_int(d.get("Temperature", "")), r["temperature_c"])
        check(model, "power-on hours", first_int(d.get("Power On Hours", "")), r["power_on_hours"])
        check(model, "power-on count", first_int(d.get("Power On Count", "")), r["power_on_count"])
        for key, field in [("Host Reads", "host_reads_gb"), ("Host Writes", "host_writes_gb"),
                           ("NAND Writes", "nand_writes_gb")]:
            if key in d:
                check(model, key, first_int(d[key]), r[field])
        rows = {a["id"]: a for a in r["attributes"]}
        check(model, "attribute ids", sorted(d["attrs"]), sorted(rows))
        for i, a in d["attrs"].items():
            o = rows.get(i)
            if not o:
                continue
            check(model, f"{i:02X} raw", a["raw"], o["raw_hex"])
            if "cur" in a:
                check(model, f"{i:02X} current", a["cur"], o["current"])
                check(model, f"{i:02X} worst", a["wor"], o["worst"])
                check(model, f"{i:02X} threshold", a["thr"], o["threshold"])
    print(f"\n{total - bad}/{total} checks match")
    sys.exit(1 if bad else 0)


main()
