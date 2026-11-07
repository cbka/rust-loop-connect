#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
# Protocol derived from Pavel Herrmann's GPL EK Loop Connect driver/protocol:
# https://github.com/pavelherr/ek-loop-connect
"""Small EK Loop Connect diagnostic/control CLI. Windows hardware unverified."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import select
import sys
import threading

VID, PID = 0x0483, 0x5750
PRODUCT = "EK Connect"
SIZE = 63
CHANNELS = ((0xA0, 0xA0), (0xA0, 0xC0), (0xA0, 0xE0),
            (0xA1, 0x00), (0xA1, 0x20), (0xA1, 0xE0))
READ_HEADER = bytes.fromhex("10 12 27 aa 01 03 00 20")
ACK_HEADER = bytes.fromhex("10 12 06 00 00 00 00 aa bb 65 ed")


class ProtocolError(Exception):
    pass


def channel_index(channel):
    if isinstance(channel, bool) or not isinstance(channel, int) or not 1 <= channel <= 6:
        raise ValueError("channel must be an integer from 1 to 6")
    return channel - 1


def read_request(channel=None):
    frame = bytearray(SIZE)
    frame[:6] = bytes.fromhex("10 12 08 aa 01 03")
    frame[6:8] = bytes((0xA2, 0x20) if channel is None else CHANNELS[channel_index(channel)])
    frame[8:14] = bytes.fromhex("00 20 66 ff ff ed")
    if channel is None:
        frame[11:13] = bytes.fromhex("60 fe")
    return bytes(frame)


def pwm_request(channel, percent):
    idx = channel_index(channel)
    if isinstance(percent, bool) or not isinstance(percent, int) or not 0 <= percent <= 100:
        raise ValueError("percent must be an integer from 0 to 100")
    frame = bytearray(SIZE)
    frame[:6] = bytes.fromhex("10 12 29 aa 01 10")
    frame[6:8] = bytes(CHANNELS[idx])
    frame[8:11] = bytes.fromhex("00 10 20")
    frame[24], frame[25], frame[45], frame[46] = percent, 0xFF, 0xFF, 0xED
    return bytes(frame)


def validate_reply(reply, acknowledgement=False):
    reply = bytes(reply)
    if len(reply) != SIZE:
        raise ProtocolError(f"expected {SIZE} reply bytes, received {len(reply)}")
    if acknowledgement:
        if not reply.startswith(ACK_HEADER):
            raise ProtocolError("unexpected PWM acknowledgement")
    elif not (reply.startswith(READ_HEADER) and reply[40:42] == b"\xaa\xbb" and reply[43] == 0xED):
        raise ProtocolError("unexpected status response signature")
    return reply


def parse_sensors(reply):
    reply = validate_reply(reply)
    # Sensor-read and fan-read share a header; these fixed field markers differ.
    if any(reply[o:o + 3] != b"\x00\x01\x00" for o in (8, 12, 16, 24)):
        raise ProtocolError("response is not a sensor report")
    return {"temperatures_c": [None if reply[o] == 0xE7 else reply[o] for o in (11, 15, 19)],
            "flow_raw": int.from_bytes(reply[22:24], "big"),
            "level_raw": reply[27]}


def parse_fan(reply):
    reply = validate_reply(reply)
    if reply[8] != 0 or reply[10:12] != bytes(2) or reply[14:21] != bytes(7):
        raise ProtocolError("response is not a fan report")
    if reply[21] > 100:
        raise ProtocolError("reported PWM exceeds 100 percent")
    return {"rpm": int.from_bytes(reply[12:14], "big"), "pwm_percent": reply[21]}


class Controller:
    def __init__(self, transport):
        self.transport = transport
        self._lock = threading.Lock()
        self._failed = False

    @contextlib.contextmanager
    def _operation(self):
        with self._lock:
            if self._failed:
                raise ProtocolError("session failed; reconnect the controller before starting a new session")
            try:
                yield
            except Exception:
                self._failed = True
                raise

    def status(self):
        with self._operation():
            result = parse_sensors(self.transport.exchange(read_request()))
            result["fans"] = [dict(channel=i, **parse_fan(self.transport.exchange(read_request(i))))
                              for i in range(1, 7)]
            return result

    def set_pwm(self, channel, percent):
        request = pwm_request(channel, percent)
        with self._operation():
            validate_reply(self.transport.exchange(request), acknowledgement=True)
            actual = parse_fan(self.transport.exchange(read_request(channel)))
            if actual["pwm_percent"] != percent:
                raise ProtocolError("PWM readback does not match the requested value")
            return dict(channel=channel, **actual)


def _text(path):
    try:
        return path.read_text().strip()
    except OSError:
        return ""


def enumerate_linux():
    devices = []
    for entry in sorted(Path("/sys/class/hidraw").glob("hidraw*")):
        hid = (entry / "device").resolve()
        interface = next((p for p in hid.parents if (p / "bInterfaceNumber").exists()), None)
        usb = next((p for p in hid.parents if (p / "idVendor").exists()), None)
        if usb is None or interface is None:
            continue
        if (_text(usb / "idVendor"), _text(usb / "idProduct"), _text(usb / "product"),
                _text(interface / "bInterfaceNumber")) != ("0483", "5750", PRODUCT, "00"):
            continue
        driver = (hid / "driver").resolve().name if (hid / "driver").exists() else ""
        devices.append({"product": PRODUCT, "interface": 0, "path": "/dev/" + entry.name,
                        "_identity": _text(usb / "serial") or usb.name, "_driver": driver})
    return devices


def _hidapi():
    try:
        import hid
    except ImportError as exc:
        raise RuntimeError("hidapi backend requires the optional 'hidapi' Python package") from exc
    return hid


def enumerate_hidapi():
    return [{"product": PRODUCT, "interface": 0, "path": d["path"],
             "_identity": d.get("serial_number") or repr(d["path"])}
            for d in _hidapi().enumerate(VID, PID)
            if d.get("vendor_id") == VID and d.get("product_id") == PID
            and (d.get("product_string") or "").strip() == PRODUCT and d.get("interface_number") == 0]


@contextlib.contextmanager
def device_lock(identity):
    # Private per-user directory; hash avoids exposing a serial in filenames.
    base = Path.home() / ".cache" / "ek-loop-connect"
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    key = hashlib.sha256(str(identity).encode()).hexdigest()[:24]
    flags = os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(base / (key + ".lock"), flags, 0o600)
    try:
        if os.name == "nt":
            import msvcrt
            os.write(fd, b"0")
            os.lseek(fd, 0, os.SEEK_SET)
            try:
                msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
            except OSError as exc:
                raise RuntimeError("controller already in use by another CLI process") from exc
        else:
            import fcntl
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise RuntimeError("controller already in use by another CLI process") from exc
        yield
    finally:
        os.close(fd)


class CheckedTransport:
    """A failed USB transaction cannot safely be retried in the same session."""
    def exchange(self, frame):
        if getattr(self, "_failed", False):
            raise ProtocolError("transport failed; reconnect the controller before starting a new session")
        try:
            return self._exchange(frame)
        except Exception:
            self._failed = True
            raise


class LinuxTransport(CheckedTransport):
    def __init__(self, device, timeout=2.0):
        if device.get("_driver") not in ("", "hid-generic"):
            raise RuntimeError("custom HID driver is active; stop hwmon control and unbind it before CLI use")
        self.fd = os.open(device["path"], os.O_RDWR | os.O_NONBLOCK)
        self.timeout = timeout

    def _exchange(self, frame):
        if len(frame) != SIZE:
            raise ValueError("invalid output frame length")
        # hidraw write requires report ID 0; unnumbered input reports omit it.
        if os.write(self.fd, b"\x00" + frame) != SIZE + 1:
            raise ProtocolError("short USB write")
        ready, _, _ = select.select([self.fd], [], [], self.timeout)
        if not ready:
            raise TimeoutError("controller did not respond")
        return os.read(self.fd, SIZE + 2)

    def close(self):
        os.close(self.fd)


class HidApiTransport(CheckedTransport):
    def __init__(self, device, timeout=2.0):
        if sys.platform.startswith("linux"):
            matches = [d for d in enumerate_linux()
                       if d["_identity"] == device.get("_identity")]
            if not matches or any(d.get("_driver") not in ("", "hid-generic") for d in matches):
                raise RuntimeError("Linux raw HIDAPI access requires a verified device with hid-generic; custom driver may be active")
        self.device = _hidapi().device()
        try:
            self.device.open_path(device["path"])
        except Exception:
            self.device.close()
            raise
        self.timeout_ms = int(timeout * 1000)

    def _exchange(self, frame):
        if len(frame) != SIZE:
            raise ValueError("invalid output frame length")
        if self.device.write(b"\x00" + frame) != SIZE + 1:
            raise ProtocolError("short USB write")
        data = bytes(self.device.read(SIZE + 2, self.timeout_ms))
        if not data:
            raise TimeoutError("controller did not respond")
        # Unnumbered HIDAPI input normally has no report ID. Accommodate a
        # zero-prefixed backend only when total length is exactly 64 bytes.
        if len(data) == SIZE + 1 and data[0] == 0:
            data = data[1:]
        return data

    def close(self):
        self.device.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=("auto", "linux", "hidapi"), default="auto")
    parser.add_argument("--device", type=int, help="index from list (required when multiple controllers are present)")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="list matching controllers without serial numbers")
    sub.add_parser("status", help="request all sensor/fan values; does not change PWM")
    pwm = sub.add_parser("set-pwm", help="explicitly change one fan channel")
    pwm.add_argument("--channel", type=int, required=True, choices=range(1, 7))
    pwm.add_argument("--percent", type=int, required=True, choices=range(0, 101))
    args = parser.parse_args(argv)
    backend = args.backend if args.backend != "auto" else ("linux" if sys.platform.startswith("linux") else "hidapi")
    try:
        devices = enumerate_linux() if backend == "linux" else enumerate_hidapi()
        if args.command == "list":
            public = [{"index": i, "product": d["product"], "interface": d["interface"],
                       **({"path": d["path"]} if backend == "linux" else {})} for i, d in enumerate(devices)]
            print(json.dumps(public, indent=2))
            return 0
        if not devices:
            raise RuntimeError("no matching EK Connect interface 0 found")
        if args.device is None and len(devices) != 1:
            raise RuntimeError("multiple controllers found; select --device INDEX from list")
        index = 0 if args.device is None else args.device
        if not 0 <= index < len(devices):
            raise ValueError("device index out of range")
        device = devices[index]
        with device_lock(device["_identity"]):
            transport = LinuxTransport(device) if backend == "linux" else HidApiTransport(device)
            try:
                controller = Controller(transport)
                result = controller.status() if args.command == "status" else controller.set_pwm(args.channel, args.percent)
                print(json.dumps(result, indent=2))
            finally:
                transport.close()
        return 0
    except (OSError, RuntimeError, ValueError, ProtocolError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
