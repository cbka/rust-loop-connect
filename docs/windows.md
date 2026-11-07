# Windows: initial open-source CLI

The current Windows deliverable is the Python CLI in `tools/ekloop.py`, including
protocol and mocked transport tests. **It has not been run on Windows or verified
with a real controller on Windows.** It is not yet a Fan Control plugin, a Windows
service, a fan-curve engine, or a graphical application. Linux `fancontrol` and
Rem0o's Windows **Fan Control** are separate programs.

## Use the CLI

Install Python 3 and the optional `hidapi` package in your chosen Python environment:

```powershell
py -m pip install hidapi
py tools/ekloop.py list
py tools/ekloop.py status
```

Discovery requires USB VID/PID `0483:5750`, product string `EK Connect` and HID
interface **0**. Other products sharing the same VID/PID are rejected. Interface 1
is not used. A HIDAPI backend reporting interface number `-1` instead of `0` will
be rejected; this conservative limitation needs Windows hardware testing. The
`list` command hides serial numbers and Windows device paths. Multiple controllers
require selection with `--device INDEX` before the subcommand:

```powershell
py tools/ekloop.py --device 0 status
```

`status` requests three temperatures and six RPM/PWM values without changing PWM.
An unconnected temperature input (`0xe7`) becomes JSON `null`. Flow and level are
reported as **raw values**, without unverified physical scaling.

To deliberately change one test fan to 50 percent:

```powershell
py tools/ekloop.py set-pwm --channel 1 --percent 50
```

Only integer channels 1–6 and percentages 0–100 are accepted. The command checks
both the acknowledgement and PWM readback. Changing PWM can reduce or stop cooling;
choose a suitable test channel and restore its previous setting afterwards. No
automatic fan sweep, calibration, or startup PWM change is performed.

## Ownership, errors and limitations

Only one program may communicate with the controller at a time. Stop the EK
application, other controller utilities, RGB applications using this controller,
and any competing service before running the CLI. The CLI uses a private per-user
lock file and a per-controller thread lock. These locks coordinate this CLI's own
processes; unrelated programs do not honor them. On Linux the CLI refuses to use a
device with a custom HID driver active, including this project's hwmon driver.
Use hwmon/fancontrol or the raw CLI separately.

Commands use bounded reads and check response sizes and signatures. After a timeout
or protocol error the controller/transport object rejects further operations. The
protocol does not echo the requested fan channel, so a late response cannot always
be assigned reliably. Reopening the device does not prove that pending data has
been cleared. Physically disconnect and reconnect the controller before a new
session after such an error. A successful USB disconnect does not necessarily
remove the controller's separate power supply or reset its stored PWM setting.

HID writes include report ID zero followed by the 63-byte protocol frame. Incoming
unnumbered reports are normally 63 bytes; the HIDAPI backend also accepts an exact
64-byte report with leading zero. Other lengths are rejected. Windows HID access
uses the OS HID class driver through HIDAPI; no custom Windows kernel driver is
included. Access rights and coexistence on the actual Windows system remain to be
verified.

The device's automatic behavior on host crash, sleep, or USB loss has not been
verified. The CLI does not supply continuous temperature-based regulation or a
hardware watchdog.

## Planned Windows Fan Control integration

An independent library and `FanControl.EkLoopConnect.dll` plugin are a future
milestone. Planned exposure is three temperature sensors, six RPM sensors, and six
PWM controls with stable IDs and paired RPM channels. The official plugin API
supports shared updates with `IPlugin2` and control/RPM pairing through
`IPluginControlSensor2`.

The plugin can be open source, but Fan Control's main application is closed source
and has separate license terms. Do not bundle its executable or
`FanControl.Plugins.dll` in this project's source or releases without appropriate
permission. Users/builders should obtain the host/API reference separately.

Acceptance requires a recorded supported Windows/Fan Control/Python/HIDAPI version,
actual USB discovery, sensor readouts, controlled single-channel PWM testing,
hotplug/suspend behavior, handling of absent sensors, and safe error handling.
Mocked tests on Ubuntu do not establish Windows hardware compatibility.

## Sources and license

- [Pavel Herrmann's protocol and GPL driver](https://github.com/pavelherr/ek-loop-connect)
- [Official Fan Control plugin API](https://github.com/Rem0o/FanControl.Releases/wiki/Plugins)
- [Fan Control source/status](https://github.com/Rem0o/FanControl.Releases)
- [Fan Control license](https://github.com/Rem0o/FanControl.Releases/blob/master/LICENSE)
- [Microsoft: user-mode HID collections](https://learn.microsoft.com/windows-hardware/drivers/hid/opening-hid-collections)
- [HIDAPI project](https://github.com/libusb/hidapi)

The CLI's protocol implementation derives from Pavel Herrmann's work and is
released under GPL-2.0-or-later, with provenance in its source header. Preserve the
project's license and attribution when redistributing it. Python's optional HIDAPI
binding and its underlying library retain their own licenses.
