# Linux and Windows build/acceptance status

Checked 2026-11-08. The desktop application supports Linux and Windows only. Its
Rust modules use the same documented 63-byte EK Loop Connect protocol. F6 keeps
the dedicated pump-compatible address `A1E0`.

## Build automation

`.github/workflows/desktop.yml` runs independently on Ubuntu 24.04 x86_64 and a
native Windows x86_64 runner. Each job installs stable Rust with rustfmt/clippy,
checks formatting, runs all unit tests, runs Clippy with warnings treated as
errors, and builds a native release using the committed `Cargo.lock` (`--locked`).
No CI job opens a USB controller or exercises physical outputs.

Successful jobs upload these binaries with `actions/upload-artifact@v4`:

| Artifact | Contents |
|---|---|
| `ek-loop-desktop-linux-x86_64` | `ek-loop-desktop` |
| `ek-loop-desktop-windows-x86_64` | `ek-loop-desktop.exe` |

Artifacts are retained for 14 days. They are CI binaries, not an installer or a
claim of tested hardware support. The workflow has read-only repository contents
permission. It runs for changes to `desktop/**` or its workflow on pushes and pull
requests, and also supports manual dispatch. Compiler version information is
recorded in each run; release assets are not published by this workflow.

Linux build prerequisites installed by CI:

```sh
sudo apt-get install build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
```

The Linux HID backend is `hidapi` 2.6.7 with `linux-native-basic-udev`; Windows uses
the same crate with `windows-native`. No separate vendor kernel driver is bundled
for Windows. GUI builds use eframe's Glow backend with Linux X11/Wayland support.

## What is verified locally

| Check | Result | Meaning |
|---|---|---|
| Protocol/session unit tests on Linux | Passed, 14 device/protocol tests | Packet fixtures, bounds, F6 mapping, missing probes, acknowledgement/readback, poisoned sessions and strict identification are covered without hardware. |
| `cargo clippy --lib -- -D warnings` | Passed | The library passed the local static checks. |
| `cargo check --locked --target x86_64-pc-windows-gnu` | Passed | Windows-specific code and dependencies type-check from Linux. This did not link or execute a Windows binary. |
| Workflow YAML parsing | Passed | YAML parsed; two runners and read-only contents permission checked. This is not an executed GitHub Actions run. |
| Native Windows build and tests | Passed in initial CI run 42432593897 | Native Windows tests, Clippy and optimized build succeeded for b9d9d99; this is not hardware acceptance. |
| Real Windows EK controller | Not tested | No Windows HID discovery, sensor, PWM or RGB hardware acceptance is established. |

Linux desktop rendering, full-project checks and read-only hardware acceptance are
tracked separately in the main project reports. A passing build, mocked test or
demo screenshot cannot establish physical fan/RGB behavior.

## Windows hardware acceptance still required

1. Record Windows version, binary commit and attached controller revision.
2. Confirm enumeration matches VID/PID `0483:5750`, trimmed product `EK Connect`
   and HID interface **0**. Some Windows HIDAPI enumeration can report an unknown
   interface (`-1`); this application deliberately rejects it rather than opening
   an unverified collection. Its behavior with the real device remains unverified.
3. Confirm three temperature inputs, six RPM readings and existing PWM settings
   against a known physical setup. Missing probes must remain unavailable.
4. With a suitable test fan attached, explicitly apply individual safe PWM values
   and verify physical response/readback, including physical F6. No calibration
   sweep is run automatically.
5. Explicitly test global RGB mode/color/speed/brightness changes and their
   acknowledgements; no independent L1–L6 or per-pixel control is promised.
6. Check unplug/replug, sleep/resume, process errors and shutdown. A failed session
   must not retry writes. The software cannot guarantee a hardware failsafe after
   USB loss or a host crash.

The application and Python CLI share a per-user hashed lock based on the serial
number. Close the official EK utility, OpenRGB and any other controller utility;
those programs do not honor this application's lock. On Linux the desktop refuses
raw HID access while a custom kernel HID/hwmon driver owns the device. Use the
native desktop controller or hwmon/fancontrol separately.

Connecting and polling issue read requests only. Manual PWM and RGB changes
require explicit actions; temperature curves start only on an explicit user
command. When healthy communication permits, stopping curves or exiting restores
only channels controlled by the active curves to 100 percent. This does not create
a background service or autostart entry.
