# Test fixtures

The `smartctl-*.json` files are **hand-written** in the shape of `smartctl -j -a` (smartmontools 7.4)
output, with plausible values. They test the JSON-to-pages conversion, not any real disk.

Real captures belong in `captures/` (one `disklens dump` per disk), with the model, the firmware
and the date in the file name, and the expected values checked against CrystalDiskInfo's
`DiskInfo.txt` for the same disk.
