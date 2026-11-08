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
