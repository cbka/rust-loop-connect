# Linux hwmon driver (experimental)

This is a modified version of [Pavel Herrmann's EK Loop Connect driver](https://github.com/pavelherr/ek-loop-connect), upstream commit `5a6cf1f99b1b922ad21f4f6dff9d0ce99cc686cc`. Original copyright and GPL-2.0-or-later notices are retained. See the repository LICENSE and attribution notes.

## Status

The module builds against Ubuntu kernel `7.0.0-42-generic`. Build verification does **not** establish successful binding, accurate sensor readings, PWM operation, or fancontrol integration on hardware. Check the main project verification record for target results.

The driver exposes six fan RPM readings and six writable `pwmN` attributes (0–255), and three temperatures in millidegrees Celsius. The unconnected thermistor marker `0xe7` returns `ENODATA` instead of 231 °C. No automatic firmware control mode is known, so no `pwmN_enable` auto mode is invented. Generic software requiring this attribute may need adaptation; actual Ubuntu fancontrol integration remains to be verified.

The inherited `fan7_input` is flow in litres/hour, **not** RPM, and `humidity1_alarm` is a coolant-level alarm, **not** relative humidity. These compatibility mappings and their physical interpretation are unverified here. RGB, SW and EXT are outside this driver.

## Build

Install the headers matching the running kernel, compiler, make, and (for persistent installation) DKMS using your Ubuntu package manager.

```sh
cd linux
make
make check
```

For a different kernel use `make KVER=... KDIR=/lib/modules/.../build`.

## Initial load and bind

Stop other programs accessing this controller, including raw HID tools, the official software, and OpenRGB. All requests share interface 0 and replies contain no transaction IDs; two clients cannot safely coordinate. Loading or binding the driver does not set PWM values, but reading hwmon files sends status requests. Each write to `pwmN` changes a fan output.

```sh
sudo insmod ./ek-loop-connect.ko
python3 ./bind-controller.py
sudo python3 ./bind-controller.py --apply
```

The helper lists devices by default. With `--apply` it selects exactly one interface 0 after checking USB VID/PID **and product name**, and refuses to displace anything other than `hid-generic`. With multiple controllers, specify `--serial YOUR_SERIAL`. It attempts rollback to the previous driver if binding fails. Never unbind devices based on `0483:5750` alone: that identifier is shared by unrelated STM32 products. Interface 1 is left to its existing driver.

Secure Boot may require signing the module with a trusted key; do not disable Secure Boot merely to load this driver.

Find the new hwmon instance by its name, rather than assuming a fixed hwmon number:

```sh
for name in /sys/class/hwmon/hwmon*/name; do
    if [ "$(cat "$name")" = ekloopconnect ]; then
        printf '%s\n' "${name%/name}"
    fi
done
```

Read `temp1_input` through `temp3_input`, `fan1_input` through `fan6_input`, and `pwm1` through `pwm6` from that directory. Record current PWM values before any controlled write test. Confirm which fans, pumps, and thermistors are physically attached before changing outputs. No automatic calibration is performed by this project.

## Persistent DKMS installation

From the repository root, after successful initial testing:

```sh
sudo install -d /usr/src/ek-loop-connect-0.1.0
sudo install -m 0644 linux/ek-loop-connect.c linux/response-validation.h linux/Makefile linux/dkms.conf /usr/src/ek-loop-connect-0.1.0/
sudo dkms add -m ek-loop-connect -v 0.1.0
sudo dkms build -m ek-loop-connect -v 0.1.0
sudo dkms install -m ek-loop-connect -v 0.1.0
```

DKMS installs the module for kernel updates. Its HID module alias supports automatic loading when the device is attached; current binding order and reboot behavior still require hardware verification. For an already bound controller, use the explicit helper. Do not blacklist `hid-generic` globally. To remove this installation, run `sudo dkms remove -m ek-loop-connect -v 0.1.0 --all` after stopping clients and unloading the module.

## fancontrol acceptance

Install Ubuntu's `fancontrol` package. Before enabling its service, verify all of the following on the intended hardware:

1. Three temperature attributes exist, and attached probes produce plausible readings; disconnected probes produce errors.
2. Six RPM and PWM attributes exist. A monitored PWM adjustment affects only its intended channel, with readback and RPM response checked.
3. A fancontrol configuration uses the actual hwmon paths, `DEVPATH` and `DEVNAME`, connected thermistors, and measured fan start/stop limits.
4. Start, stop, sensor failure, USB removal, and reboot behaviors have been tested. A suitable cooling fallback must be demonstrated before unattended operation.

Do not copy guessed thermistor mappings or minimum PWM values into `/etc/fancontrol`. The commonly used `pwmconfig` program may stop fans while calibrating; inspect the actual cooling connections before using it. This package does not launch it.

## Error handling and rollback

Responses are checked for exact report length, expected message kind, known framing markers, and basic payload bounds. Outgoing transport errors are returned instead of silently ignored. Request and response buffers are separate and protected against callback races. Only one request is in flight.

Following a timeout or failed transfer, further requests return `EIO` until the device is rebound. This deliberately avoids accepting a delayed response as the next channel's measurement. The protocol does not carry an echoed channel identifier, so full attribution against another concurrent client is impossible. Unknown checksum bytes remain undocumented and are not validated.

Stopping this driver or its user-space controller does **not** program a fallback fan speed: the controller may retain its last PWM setting. Unloading alone must not be treated as a cooling failsafe. Restore recorded safe PWM values while communication still works before ending a write test. For a rebind after a communication failure, stop clients, unload the module, then reload and use the helper; reconnecting USB is another recovery path.
