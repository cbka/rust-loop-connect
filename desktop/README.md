# EK Loop Desktop

Native open-source **Rust desktop application for Linux and Windows**, with a German interface. Kubuntu is the first hardware test system. This is a new controller application, not a binary port of EK's proprietary .NET software. It uses the OS HID driver directly, with no vendor DLL or installed Python required.

## Functions

- Three temperature probes with an explicit unavailable state; six RPM/PWM channels. F6 correctly uses the pump address `A1E0`, including when a fan is attached.
- Deliberate per-channel manual PWM changes, acknowledged and read back.
- Editable temperature curves from T1–T3, piecewise linear interpolation and local profiles. Selecting a curve or loading a profile does **not** start regulation.
- Nine global RGB effects, color, brightness and speed. L1–L6 and the controller lighting share this command; individual output/pixel control is not claimed.
- Raw flow/level values, without assuming sensor presence or calibration.
- A labeled demo mode that never opens HID hardware, plus read-only CLI diagnostics.

The proprietary CPUID SDK, CPU/GPU telemetry integration, firmware flashing, bleed-air routines, autostart/background services, custom per-pixel RGB and the unknown extension/debug interfaces are outside this release. No original UI assets or decompiled vendor implementation are included.

## Run

Start the `ek-loop-desktop` binary on Linux or `ek-loop-desktop.exe` on Windows. With a single available controller, the app connects and queries its readings. It does not program fan or RGB settings on startup. If several matching controllers are found, choose one explicitly.

```sh
# Developer build and start
cargo run --locked --release
# UI without hardware
cargo run --locked --release -- --demo
# Read-only diagnostics
cargo run --locked --release -- --list
cargo run --locked --release -- --status
```

Choose a fan channel in **Lüfter**, change the manual target and click **Übernehmen**. For a curve, select **Temperaturkurve**, a connected T1/T2/T3 probe and the curve points, then explicitly click **Kurven starten**. The **Beleuchtung** tab applies one shared setting to the LED outputs. **Profile** saves/loads drafts locally; existing malformed profiles are reported and not overwritten automatically.

## Control lifecycle

The application must remain open for temperature curves. Active curve channels receive 100% if their chosen temperature is unavailable or stale; the accepted sensor age is at most six seconds. Stopping curves, disconnecting or closing the app sets only previously curve-controlled channels to 100% while communication is healthy. Manually set outputs retain their setting. A close request overtakes pending user commands rather than applying queued old settings first.

A communication failure poisons the session: no additional commands are sent, the error is displayed, and physical USB reconnection is required before trying again. **A disconnected or crashed process cannot guarantee a new cooling value reaches the controller.** No hardware watchdog has been verified. Confirm attached fans/pumps, suitable minimum startup PWM and sensor placement before unattended control. Physical fan response and thermal calibration remain separate from software tests.

Use the app **instead of**, not concurrently with, this repository's kernel hwmon/fancontrol path, Python tool, original EK software or an RGB utility using the same controller. The app checks for a competing Linux driver and coordinates with the Python tool through a shared per-user file lock. Other programs/users need not honor that lock.

## Linux permissions

If read access is denied, install the provided product-specific desktop-session udev rule:

```sh
sudo install -m 0644 packaging/70-ek-loop-connect.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
```

Unplug/replug USB afterward. The current desktop session should gain access; do not run the entire GUI as root. The USB ID alone is insufficient because unrelated devices reuse it.

## Build and validation

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Rust 1.88 or newer is required. On Linux the eframe build uses X11/Wayland, OpenGL and system GUI libraries; Ubuntu build dependencies are recorded in the workflow. Linux HID uses the Rust native basic-udev backend. Windows uses native HID access; no custom USB replacement driver is installed. See [platform evidence](docs/PLATFORMS.md), [implementation contract](docs/CONTRACT.md) and [validation record](docs/VALIDATION.md).

Local profile storage follows each platform's standard user configuration directory. Files are size-limited, validated and saved with an atomic replacement; the app does not store credentials.

## Provenance

GPL-2.0-or-later, as the containing project. The protocol builds on [Pavel Herrmann's EK Loop Connect work](https://github.com/pavelherr/ek-loop-connect) and [OpenRGB's EK controller implementation](https://github.com/CalcProgrammer1/OpenRGB/tree/master/Controllers/EKController). Original EK binaries are used only for local static comparison and are not redistributed. Their complete application-to-protocol trace confirms the F6 pump-channel mapping. Export names alone do not establish a device capability.
