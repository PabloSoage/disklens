"""Extract the Smart* sections of a CrystalDiskInfo .lang file into a UTF-8 INI.

Usage: python lang2ini.py <English.lang> <out.ini>
"""
import sys

src, dst = sys.argv[1], sys.argv[2]
text = open(src, encoding="utf-16").read()
out = [
    "; S.M.A.R.T. attribute names, one section per vendor table.",
    "; Extracted from CrystalDiskInfo 9.9.3 (CdiResource/language/English.lang).",
    "; (C) 2008-2025 hiyohiyo, (C) 2025 CrystalMark Inc. MIT License, see THIRD_PARTY.md.",
    "",
]
keep = False
for line in text.splitlines():
    s = line.strip()
    if s.startswith("["):
        name = s[1:s.index("]")]
        keep = name.startswith("Smart")
        if keep:
            out.append("")
            out.append(f"[{name}]")
        continue
    if keep and "=" in s and not s.startswith(";"):
        k, v = s.split("=", 1)
        out.append(f"{k.strip()}={v.strip()}")
open(dst, "w", encoding="utf-8", newline="\n").write("\n".join(out).lstrip("\n") + "\n")
