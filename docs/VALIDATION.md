# Validation and target acceptance

Date: 2026-11-07. Development snapshot. Target: Ubuntu, kernel 7.0.0-42-generic; physical EK Connect over USB HID. No USB serial numbers or host inventories are published.

## Hardware queries completed

- Product descriptor identifies EK Connect; vendor/product 0483:5750.
- Two HID interfaces expose 63-byte input/output reports.
- Linux interface 0 can be opened by an unprivileged user after a device-specific access rule was installed.
- Passive capture over 10 seconds returned no unsolicited reports.
- Seven sequential query requests (one sensor request and F1–F6) returned full 63-byte responses with the documented header/trailer.
- All three temperature bytes were 0xe7, consistent with unconnected probes in the upstream protocol. These are unavailable values, not 231 degrees Celsius.
- All six fan RPM responses were zero. Target PWM percentages: F1 24, F2–F5 0, F6 38. These are observed settings, not settings applied by this project.
- Fan response byte 9 was 0x03 on some channels and 0x00 on another, despite upstream documentation marking bytes 8–11 as zero. Parsers must not insist this byte is zero.
- Flow raw reading and level raw byte were zero. Physical sensor presence, calibration and correctness are unverified.
- No PWM set, RGB, firmware, SW or EXT commands were sent during the initial hardware queries.

## Required acceptance still pending

1. Install and bind the reviewed kernel module on the actual target with administrator authentication; read hwmon values through sysfs and sensors.
2. Connect a known fan and temperature probe; confirm physical channel mapping and real readings. Six addressable channels are not equivalent to six physically tested fan outputs.
3. Record original PWM settings; test safe selected PWM values on the confirmed fan, observe RPM changes and restore the intended baseline. Do not lower an unidentified pump or fan.
4. Configure fancontrol with confirmed temperature/fan mapping and agreed thermal thresholds. Preserve existing system configuration. Verify process error behavior and recovery; no hardware watchdog claim.
5. Run the Windows transport on a Windows host with the actual controller and verify disconnect/reconnect behavior. A Linux test or Windows CI unit test is not Windows hardware acceptance.

No continuously running services, containers or Docker volumes are created by this project.

## Local checks completed

- Kernel module build: PASS against 7.0.0-42-generic headers, W=1. Compiler binary-name and unavailable pahole warnings were emitted; no compilation errors. No module loaded.
- C response validator: PASS with actual F1 capture, F6 variant, sensor and malformed/ACK test cases.
- Portable CLI unit suite: PASS, 18 tests; no hardware writes in tests.
- Portable CLI list/status: PASS against physical Linux HID interface 0; same readings as the initial queries.
- Python syntax checks: PASS.
- Independent source review: response bounds, shared VID/PID identity check, callback/request synchronization, disconnected probes and truthful runtime limitations checked.
- Publication scan: no local USB serial, private key, token or workstation path in tracked source.

Remote CI results are recorded by GitHub Actions; local checks do not substitute for executing Windows hardware checks.
