"""Replace the serial number in a `disklens dump` file, everywhere it appears.

The serial is overwritten in `device.serial`, in the ATA IDENTIFY words 10-19 and in the NVMe
Identify Controller bytes 4-23, with a placeholder of the same length, so the pages keep their
layout (and an IDENTIFY checksum, if present, is fixed up).

Usage: python -I tools/redact_dump.py <in.json> <out.json> <placeholder>
"""
import json
import sys

src, dst, tag = sys.argv[1], sys.argv[2], sys.argv[3]
d = json.load(open(src, encoding="utf-8-sig"))


def fill(n):
    return (tag * (n // len(tag) + 1))[:n].encode()


d["device"]["serial"] = tag

ident = d.get("ata_identify")
if ident:
    b = bytearray.fromhex(ident)
    s = fill(20)
    for k in range(10):  # two characters per word, high byte first
        b[20 + 2 * k] = s[2 * k + 1]
        b[20 + 2 * k + 1] = s[2 * k]
    if b[510] == 0xA5:
        b[511] = (-sum(b[:511])) & 0xFF
    d["ata_identify"] = b.hex()

ctrl = d.get("nvme_identify_controller")
if ctrl:
    b = bytearray.fromhex(ctrl)
    b[4:24] = fill(20)
    d["nvme_identify_controller"] = b.hex()

json.dump(d, open(dst, "w", encoding="utf-8", newline="\n"), indent=2)
open(dst, "a", encoding="utf-8", newline="\n").write("\n")
