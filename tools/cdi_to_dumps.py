"""Turn the raw pages inside CrystalDiskInfo's DiskInfo.txt into disklens dump files, so
both programs analyse exactly the same bytes (H-001).

DiskInfo.txt prints IDENTIFY_DEVICE as 256 little-endian 16-bit words and the SMART pages
(SMART_READ_DATA, SMART_READ_THRESHOLD, SMART_NVME) as bytes. For NVMe only the first 512
bytes of Identify Controller are printed; the rest is zero-filled (it only holds the
optional-command bits that feed the "Features" line).

Usage: python -I tools/cdi_to_dumps.py <DiskInfo.txt> <outdir> [--redact]
"""
import json
import os
import re
import sys


def section(block, name):
    m = re.search(r"-- " + name + r" -+\n(.*?)(?:\n\n|\n-- |\Z)", block, re.S)
    return m.group(1) if m else None


def words(text):
    out = bytearray()
    for line in text.splitlines():
        m = re.match(r"\d{3}: (.*)", line)
        if m:
            for w in m.group(1).split():
                v = int(w, 16)
                out += bytes([v & 0xFF, v >> 8])
    return bytes(out)


def bytes_dump(text):
    out = bytearray()
    for line in text.splitlines():
        m = re.match(r"[0-9A-F]{3}: (.*)", line)
        if m:
            out += bytes(int(x, 16) for x in m.group(1).split())
    return bytes(out)


def main():
    src, outdir = sys.argv[1], sys.argv[2]
    redact = "--redact" in sys.argv
    text = open(src, encoding="utf-8-sig").read()
    os.makedirs(outdir, exist_ok=True)
    parts = re.split(r"-{70,}\n \((\d+)\) (.*)\n-{70,}\n", text)
    for i in range(1, len(parts), 3):
        idx, model, block = parts[i], parts[i + 1].strip(), parts[i + 2]
        ident = section(block, "IDENTIFY_DEVICE")
        if not ident:
            continue
        ident = words(ident)
        nvme = section(block, "SMART_NVME")
        iface = re.search(r"Interface : (.*)", block)
        serial = re.search(r"Serial Number : (.*)", block).group(1).strip()
        dump = {
            "format": 1,
            "tool": "tools/cdi_to_dumps.py (pages printed by CrystalDiskInfo 9.9.3)",
            "captured_at": "",
            "device": {
                "name": f"cdi{idx}", "path": f"cdi{idx}", "model": model,
                "serial": "SERIAL" if redact else serial, "firmware": "", "size_bytes": 0,
                "rotational": None, "removable": False,
                "transport": "nvme" if nvme else "sata", "link": "", "mounts": [],
            },
            "source": "crystaldiskinfo",
            "errors": [],
        }
        if nvme:
            ctrl = bytearray(ident + bytes(4096 - len(ident)))
            if redact:
                ctrl[4:24] = b"SERIAL".ljust(20)
            dump["nvme_identify_controller"] = bytes(ctrl).hex()
            dump["nvme_smart_log"] = bytes_dump(nvme).hex()
        else:
            ident = bytearray(ident)
            if redact:
                s = b"SERIAL".ljust(20)
                for k in range(10):
                    ident[20 + 2 * k] = s[2 * k + 1]
                    ident[20 + 2 * k + 1] = s[2 * k]
                if ident[510] == 0xA5:
                    ident[511] = (-sum(ident[:511])) & 0xFF
            data = section(block, "SMART_READ_DATA")
            thr = section(block, "SMART_READ_THRESHOLD")
            dump["ata_identify"] = bytes(ident).hex()
            if data:
                dump["ata_smart"] = bytes_dump(data).hex()
                dump["ata_smart_check"] = dump["ata_smart"]
            if thr:
                dump["ata_thresholds"] = bytes_dump(thr).hex()
        safe = re.sub(r"[^A-Za-z0-9._-]+", "-", model)
        with open(os.path.join(outdir, f"cdi{idx}-{safe}.json"), "w", encoding="utf-8", newline="\n") as f:
            json.dump(dump, f, indent=2)
            f.write("\n")
        print(f"cdi{idx}: {model} ({'NVMe' if nvme else 'ATA'})")


main()
