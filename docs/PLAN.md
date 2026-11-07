# EK Loop Connect: implementation and acceptance plan

Status: work in progress, 2026-11-07. License: GPL-2.0-or-later.
Baseline: https://github.com/pavelherr/ek-loop-connect commit 5a6cf1f99b1b922ad21f4f6dff9d0ce99cc686cc.

1. Identify USB interfaces and verify access. PASS: EK Connect product string, VID:PID 0483:5750, two HID interfaces, 63-byte reports; both nodes open for current user. No unsolicited reports over 10 seconds.
2. Query sensors and six fan statuses without changing settings. PASS requires validated full length/type responses from interface 0, recorded decoded values; distinguish unavailable sensors from measured zero. No concurrent HID clients.
3. Linux hwmon driver adaptation and build (independent of Windows research). PASS: build on target kernel 7.0.0-42-generic; report validation and errors reviewed; expose 6 fan RPM/PWM and 3 temperature inputs. Do not load kernel modules until code/build checks pass. Root authentication is an external installation dependency.
4. Windows architecture (parallel). Research official Fan Control plugin API, HID transport and licenses. PASS: documented open source integration path, honest platform test status. Hardware testing requires a Windows target. Never claim Windows runtime acceptance from Linux checks.
5. Integration and publication. Review GPL attribution and changes, run relevant tests, publish to authenticated cbka GitHub account. Public files must not include personal serials, absolute home paths, local credentials or raw host inventory. Intended name: ek-loop-connect, unless already occupied.
6. Target acceptance: Ubuntu fancontrol must read T1-T3, F1-F6 and control selected attached fan with verified RPM response. Requires hardware connection mapping from owner and root installation. Do not run pwmconfig blindly (it may stop fans). Temperature curves require user-confirmed sensor/fan assignment and thresholds. Provide installation steps and mark blocked criteria accurately.

No RGB, SW or EXT writes in the first milestone. No automatic device reset, firmware update or replacement USB driver. Rollback: unload module and restore hid-generic; remove only this project's installed configuration. Existing fancontrol configuration must be preserved.

## Review refinements

Queries use HID output reports with operation 0x03; they do not set PWM (operation 0x10). Treat disconnected temperature byte 0xe7 as unavailable. Fan replies do not include a channel ID; allow only one outstanding request and fail closed after timeout to avoid consuming a late reply as another channel. Reopening alone cannot prove wire resynchronization. Cooling settings persist; rebinding a driver does not restore previous PWM settings. Record baseline and an explicit restoration procedure before physical writes. Publish neither personal serial numbers nor private commit email addresses.
