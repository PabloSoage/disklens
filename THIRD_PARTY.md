# Third-party material

## CrystalDiskInfo

DiskLens ports the disk evaluation of CrystalDiskInfo 9.9.3 to Rust and ships its English
attribute names:

- `src/smart/vendor.rs`: `CAtaSmart::CheckSsdSupport` and the `IsSsd*` vendor detection.
- `src/smart/derive.rs`: `FillSmartData`, `FillSmartThreshold`, the life/host-writes loop of
  `CheckSsdSupport`, `GetTimeUnitType`, `GetPowerOnHours`.
- `src/smart/health.rs`: `CheckDiskStatus`, the attribute icons of `UpdateListCtrl` and
  `GetTemperatureClass`.
- `src/smart/ata.rs`: parts of `AddDisk`, `GetTransferMode`, `GetAtaMajorVersion`.
- `src/smart/nvme.rs`: `AddDiskNVMe` and `NVMeInterpreter.cpp` (by Minkyu Kim, MIT).
- `data/smart_attributes.ini`: the `[Smart*]` sections of `CdiResource/language/English.lang`,
  extracted with `tools/lang2ini.py`.

```
MIT License

(C) 2008-2025 hiyohiyo
(C) 2025 CrystalMark Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

DiskLens is not affiliated with or endorsed by CrystalMark Inc. "CrystalDiskInfo" is named
only to credit the source of the logic.
