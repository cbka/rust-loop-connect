#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""List verified EK interfaces; optionally rebind exactly one to the hwmon driver."""
import argparse
import os
from pathlib import Path


def find_controllers():
    for entry in sorted(Path('/sys/bus/hid/devices').glob('0003:0483:5750.*')):
        interface = entry.resolve().parent
        usb = interface.parent
        try:
            attrs = {name: (usb / name).read_text().strip()
                     for name in ('idVendor', 'idProduct', 'product', 'serial')}
            if ((interface / 'bInterfaceNumber').read_text().strip() == '00'
                    and attrs['idVendor'] == '0483'
                    and attrs['idProduct'] == '5750'
                    and attrs['product'] == 'EK Connect'):
                yield entry, attrs['serial']
        except (OSError, UnicodeError):
            continue


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', help='Select a controller by serial number')
    parser.add_argument('--apply', action='store_true', help='Perform the rebind (root required)')
    args = parser.parse_args()
    matches = [(p, serial) for p, serial in find_controllers()
               if args.serial is None or serial == args.serial]
    if not matches:
        parser.exit(1, 'No matching EK Connect interface 0 found.\n')
    for path, serial in matches:
        driver = (path / 'driver').resolve().name if (path / 'driver').exists() else 'unbound'
        print(f'{path.name}: serial={serial}, driver={driver}')
    if not args.apply:
        return
    if len(matches) != 1:
        parser.error('Multiple devices found; select exactly one with --serial')
    if os.geteuid() != 0:
        parser.error('--apply requires root')
    target = Path('/sys/bus/hid/drivers/ek-loop-connect')
    if not (target / 'bind').exists():
        parser.error('Load the ek-loop-connect module first')
    path, _ = matches[0]
    old_driver = (path / 'driver').resolve() if (path / 'driver').exists() else None
    if old_driver == target:
        print('Already bound to ek-loop-connect')
        return
    if old_driver and old_driver.name != 'hid-generic':
        parser.error(f'Refusing to replace unexpected driver {old_driver.name}')
    if old_driver:
        (old_driver / 'unbind').write_text(path.name)
    try:
        (target / 'bind').write_text(path.name)
    except OSError:
        if old_driver:
            try:
                (old_driver / 'bind').write_text(path.name)
            except OSError as rollback_error:
                print(f'Rollback failed: {rollback_error}; unplug/replug the controller')
        raise
    print('Bound to ek-loop-connect; inspect /sys/class/hwmon/*/name')


if __name__ == '__main__':
    main()
