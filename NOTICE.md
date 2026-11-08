# Attribution and changes

Upstream: https://github.com/pavelherr/ek-loop-connect
Baseline commit: 5a6cf1f99b1b922ad21f4f6dff9d0ce99cc686cc

- `linux/ek-loop-connect.c` is derived from Pavel Herrmann's 2021 Linux hwmon driver (GPL-2.0-or-later). Its original copyright notice is retained.
- `docs/upstream-protocol.md` is copied from Pavel Herrmann's repository at that commit. It is retained as upstream evidence, including explicitly unknown fields; it is not a claim that all bytes are understood.
- Portable packet handling is based on that protocol and is distributed under GPL-2.0-or-later, with attribution in the source.
- Changes dated 2026-11-07 include driver robustness, unavailable-temperature handling, portable diagnostic/control tooling, tests, DKMS packaging, and installation/validation documentation. Consult source headers and git history for exact changes.

The upstream module README credits the Linux corsair-cpro module as inspiration. No EK proprietary software, firmware or binaries are included. The Rust desktop RGB protocol facts also reference OpenRGB's EKController implementation (GPL-2.0-or-later; Chris M / Dr_No). The new desktop implementation contains no proprietary vendor DLL, decompiled implementation or original UI assets.
