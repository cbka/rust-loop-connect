# EK Loop Connect — Linux hwmon and portable control tools

Open-source support for the EK-Loop Connect fan controller. The first goal is six PWM/RPM channels and three external temperature inputs in Ubuntu's `hwmon` / `fancontrol`. A portable Python tool also provides a starting point for Windows support.

**Development snapshot, not a completed fan-control installation.** Hardware status queries work on Linux. Kernel installation, physical PWM response and Windows runtime acceptance remain pending. No automatic fan curve is enabled by this repository.

## Credits and license

This project builds on **[Pavel Herrmann (pavelherr)'s ek-loop-connect repository](https://github.com/pavelherr/ek-loop-connect)**, including his Linux driver and reverse-engineered protocol documentation. Upstream baseline: [`5a6cf1f99b1b922ad21f4f6dff9d0ce99cc686cc`](https://github.com/pavelherr/ek-loop-connect/commit/5a6cf1f99b1b922ad21f4f6dff9d0ce99cc686cc). Original copyright and license notices are retained. Thank you, Pavel, for making this work available.

Distributed under **GPL-2.0-or-later**; see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md). This is an independent community project, not an official EK product.

## Current support

| Capability | State |
| --- | --- |
| Identify EK Connect and HID interface 0 | Verified on Linux |
| Query all six fan channels | Verified; current test device reports 0 RPM on each |
| Read configured PWM targets | Verified on Linux |
| Identify disconnected T1–T3 (`0xe7`) | Verified; all three inputs currently disconnected |
| Read actual attached temperature probe | Not yet tested |
| Linux hwmon driver on kernel 7.0.0-42-generic | Build verification recorded in validation document |
| Physical fan speed changes | Not yet tested |
| Ubuntu fancontrol configuration | Requires installed driver, connected probes/fans and confirmed mapping |
| Windows Python/HIDAPI transport | Implemented experimental path; real Windows device test pending |
| Windows Fan Control plugin / GUI | Planned, not implemented |
| RGB, SW, EXT | Outside the first milestone |

## Quick start

Read [Linux installation and fancontrol notes](linux/README.md) for the kernel integration. Use the diagnostic tool first if the kernel driver is **not** bound to interface 0:

```sh
python3 tools/ekloop.py list
python3 tools/ekloop.py status
```

`status` sends only query commands. It does not change PWM targets. On Linux it uses `hidraw` and Python's standard library. [Windows setup and limitations](docs/windows.md) describe the optional HIDAPI transport.

The VID/PID `0483:5750` is shared by unrelated products. Always also verify the product name **EK Connect** and interface 0. Do not grant access to all devices with that USB ID indiscriminately.

## Cooling behavior

The controller is reported to retain the last PWM setting when control software stops. No verified hardware watchdog is available. Running a controller process is required for a software temperature curve; a crash must not be assumed to restore cooling automatically. Confirm connected fan/probe mapping and suitable minimum/startup PWM before enabling a curve. Do not run `pwmconfig` unattended: its probing can stop fans.

Use only one controlling client at a time: kernel driver + fancontrol **or** the portable HID tool. EK software, OpenRGB and other HID clients can interfere with response routing. Never run both against the same interface simultaneously. Read [validation and remaining acceptance work](docs/VALIDATION.md).

## Development

```sh
python3 -m unittest discover -s tests -v
make -C linux
```

Build artifacts are excluded from version control. Tests run without hardware; actual device checks are recorded separately. See [PLAN.md](docs/PLAN.md) for the remaining milestones and [upstream protocol](docs/upstream-protocol.md) for the original wire description.
