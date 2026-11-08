# Desktop validation — 2026-11-08

Implemented for Linux and Windows. This record separates compilation, modeled behavior and real device evidence.

## Local software checks

Rust unit tests cover F6's dedicated A1E0 mapping, unavailable probes, malformed/oversized reports, real packet fixtures, ACK/readback, session poisoning and Python-compatible locking identity. Profile/curve tests cover bounds, nonfinite values, oversized/invalid files, atomic overwrite and interpolation/failsafe. Worker tests cover no implicit startup writes, explicit curve activation, missing-probe 100%, stop/disconnect behavior and prioritized shutdown discarding queued commands. GUI tests exercise four pages at960x720 and1280x900 and keyboard focus. Exact final results are recorded after integration.

The GUI's diagnostic screenshots use an explicitly labeled isolated demo controller. They verify rendered layouts and content states; they do not constitute live hardware control tests.

## Real Kubuntu device

The native Rust HID backend discovers and opens the physical EK Connect and successfully queries three probe fields and six fan states. At the initial Rust acceptance query, all temperature fields were unavailable, RPM values were zero, and PWM readbacks were24,0,0,0,0,38 percent. These values are a dated observation, not a fixed device state or a setting applied by this application. Earlier Python checks established T1–T3 channel/probe routing with actual attached probes.

No physical PWM changes, live curve regulation or RGB setting changes were performed by the Rust validation. Manual PWM and RGB paths have mocked transport tests; earlier independent USB scripts demonstrated global red, marquee and rainbow effects on the same controller. The Rust write path still needs a known attached fan and explicit physical acceptance.

## Windows

A local Windows-target cargo check is available. Native Windows CI builds/tests are tracked separately in GitHub Actions. Neither a cross-check nor a native Windows CI build proves actual HID device operation on Windows.

## Remaining target acceptance

- Attach a known fan to a confirmed port and verify selected PWM changes, RPM response and curve behavior without touching unidentified cooling hardware.
- Reattach a known probe to verify changing physical temperatures through the Rust application.
- Validate Windows HID discovery, access, unplug/replug, controlled PWM/RGB and GUI lifecycle on a Windows machine with the device.
- Verify flow/level physical sensors and calibration before displaying interpreted engineering values.

No installer/custom vendor DLL was run, no proprietary source or assets copied, and no service/autostart or Docker resources created.

## Integration checks completed

- 42 local Rust tests passed; formatting and all-target Clippy with warnings treated as errors passed. Native optimized Linux build passed.
- Read-only status queries with the native optimized Linux binary succeeded against the connected controller. No live PWM/RGB writes were sent by the Rust acceptance run.
- Rendered native demo windows were inspected at960x720 and1280x900; curve editor and lighting pages, plus empty/loading/error/permission presentation states were also captured. A keyboard Tab-focus test and four-page rendering tests passed. Simulated UI states are labeled demo and do not prove operating-system permission/hotplug behavior.
- Independent final review found no remaining critical blocker after the shutdown-queue regression and nonzero fan-metadata handling were corrected.
- [Initial native CI run 42432593897](https://github.com/cbka/rust-loop-connect/actions/runs/42432593897) passed Linux and Windows tests, Clippy and optimized builds for commit b9d9d99. Later changes replace two UI arrow labels with text so they render legibly with the Linux fonts.

The optional older installer was also checked: its EK.exe version1.0.0.9 is older than MSI application1.5.0.0. Both DLLs implement single-LED export names as immediate error stubs; no working per-pixel path was found in their actual UI call chains. This does not establish impossibility in unexamined device firmware.

## Version 0.0.1 release preparation (2026-11-08)

- Visible application branding is Rust Loop Connect; USB identity matching and the shared device lock remain unchanged.
- Version output reports `Rust Loop Connect 0.0.1`. All 42 local tests, formatting, all-target Clippy and the optimized Linux build pass after the rename.
- Three new 1280×900 demo captures (overview, curve editor, lighting) were visually inspected and embedded in the root README. Captures use defaults and simulated readings.
- Previous development profiles can be read as drafts; saving uses the new application directory. Demo profiles are separate. No startup control commands were introduced.
- Release acceptance requires the tag version to match Cargo, both native build/test jobs to pass, and exactly two raw binaries to be attached. Windows physical device acceptance remains pending.
