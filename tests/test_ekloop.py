# SPDX-License-Identifier: GPL-2.0-or-later
"""Protocol/transport tests; no hardware access or optional packages required."""
import importlib.util
import io
from pathlib import Path
import types
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("ekloop", Path(__file__).resolve().parents[1] / "tools" / "ekloop.py")
e = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(e)
SENSOR_HEX = "101227aa01030020000100e7000100e7000100e70000000000010000000000000000000000000000aabbeced00000000000000000000000000000000000000"
FAN_HEX = "101227aa010300200003000000000000000000000018000000000000000000000000000000000000aabb4eed00000000000000000000000000000000000000"


def fan_reply(percent=24, rpm=1234):
    reply = bytearray.fromhex(FAN_HEX)
    reply[12:14] = rpm.to_bytes(2, "big")
    reply[21] = percent
    return bytes(reply)


class FakeTransport:
    def __init__(self, replies):
        self.replies, self.requests = list(replies), []

    def exchange(self, frame):
        self.requests.append(frame)
        return self.replies.pop(0)


class ProtocolTests(unittest.TestCase):
    def test_real_sensor_fixture_marks_disconnected_as_null(self):
        self.assertEqual(e.parse_sensors(bytes.fromhex(SENSOR_HEX)),
                         {"temperatures_c": [None] * 3, "flow_raw": 0, "level_raw": 0})

    def test_real_fan_fixture_with_nonzero_field_at_nine(self):
        self.assertEqual(e.parse_fan(bytes.fromhex(FAN_HEX)), {"rpm": 0, "pwm_percent": 24})

    def test_temperature_flow_and_level_offsets(self):
        reply = bytearray.fromhex(SENSOR_HEX)
        reply[11], reply[15], reply[19] = 20, 30, 40
        reply[22:24] = b"\x01\x23"
        reply[27] = 100
        self.assertEqual(e.parse_sensors(reply),
                         {"temperatures_c": [20, 30, 40], "flow_raw": 291, "level_raw": 100})

    def test_read_request_exact_offsets(self):
        sensor = e.read_request()
        self.assertEqual(len(sensor), 63)
        self.assertEqual(sensor[:14], bytes.fromhex("10 12 08 aa 01 03 a2 20 00 20 66 60 fe ed"))
        self.assertEqual(sensor[14:], bytes(49))
        for index, pair in enumerate(e.CHANNELS, 1):
            request = e.read_request(index)
            self.assertEqual(len(request), 63)
            self.assertEqual(request[6:8], bytes(pair))
            self.assertEqual(request[10:14], bytes.fromhex("66 ff ff ed"))

    def test_set_request_keeps_other_fields_zero(self):
        for percent in (0, 24, 100):
            frame = e.pwm_request(6, percent)
            expected = bytearray(63)
            expected[:11] = bytes.fromhex("10 12 29 aa 01 10 a1 e0 00 10 20")
            expected[24], expected[25], expected[45], expected[46] = percent, 255, 255, 237
            self.assertEqual(frame, bytes(expected))

    def test_invalid_bounds_before_io(self):
        for channel in (0, 7, -1, True, 1.5, "1"):
            with self.subTest(channel=channel), self.assertRaises(ValueError):
                e.pwm_request(channel, 50)
        for value in (-1, 101, 25.5, True, "50"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                e.pwm_request(1, value)

    def test_truncated_oversized_wrong_header_and_footer(self):
        original = bytes.fromhex(FAN_HEX)
        for bad in (original[:-1], original + b"\x00", b"\x00" + original[1:],
                    original[:40] + b"\x00\x00" + original[42:], original[:43] + b"\x00" + original[44:]):
            with self.subTest(bad=bad.hex()), self.assertRaises(e.ProtocolError):
                e.parse_fan(bad)

    def test_fan_sensor_response_type_not_interchangeable(self):
        with self.assertRaises(e.ProtocolError):
            e.parse_fan(bytes.fromhex(SENSOR_HEX))
        with self.assertRaises(e.ProtocolError):
            e.parse_sensors(bytes.fromhex(FAN_HEX))

    def test_fan_pwm_out_of_range_rejected(self):
        with self.assertRaises(e.ProtocolError):
            e.parse_fan(fan_reply(101))

    def test_status_reads_seven_reports_without_pwm_write(self):
        fake = FakeTransport([bytes.fromhex(SENSOR_HEX)] + [fan_reply(i, i * 100) for i in range(1, 7)])
        result = e.Controller(fake).status()
        self.assertEqual(fake.requests, [e.read_request()] + [e.read_request(i) for i in range(1, 7)])
        self.assertEqual(result["fans"][5], {"channel": 6, "pwm_percent": 6, "rpm": 600})

    def test_protocol_failure_poisons_controller_no_more_io(self):
        fake = FakeTransport([bytes(63), bytes.fromhex(SENSOR_HEX)])
        controller = e.Controller(fake)
        with self.assertRaises(e.ProtocolError):
            controller.status()
        with self.assertRaises(e.ProtocolError):
            controller.status()
        self.assertEqual(len(fake.requests), 1)

    def test_timeout_poisons_controller_no_retry(self):
        class TimeoutTransport:
            calls = 0
            def exchange(self, frame):
                self.calls += 1
                raise TimeoutError("test timeout")
        fake = TimeoutTransport()
        controller = e.Controller(fake)
        with self.assertRaises(TimeoutError):
            controller.status()
        with self.assertRaises(e.ProtocolError):
            controller.set_pwm(1, 50)
        self.assertEqual(fake.calls, 1)

    def test_set_requires_acknowledgement_and_verified_readback(self):
        ack = e.ACK_HEADER + bytes(63 - len(e.ACK_HEADER))
        fake = FakeTransport([ack, fan_reply(55)])
        self.assertEqual(e.Controller(fake).set_pwm(2, 55)["pwm_percent"], 55)
        self.assertEqual(fake.requests, [e.pwm_request(2, 55), e.read_request(2)])
        for replies in ([bytes.fromhex(FAN_HEX)], [ack, fan_reply(54)]):
            with self.assertRaises(e.ProtocolError):
                e.Controller(FakeTransport(replies)).set_pwm(2, 55)


class HidApiTests(unittest.TestCase):
    def setUp(self):
        platform = patch.object(e.sys, "platform", "win32")
        platform.start()
        self.addCleanup(platform.stop)

    def test_hidapi_linux_cannot_bypass_kernel_driver_guard(self):
        device = {"path": b"device", "_identity": "test"}
        linux_device = dict(device, _driver="ek-loop-connect")
        with patch.object(e.sys, "platform", "linux"), \
             patch.object(e, "enumerate_linux", return_value=[linux_device]), \
             patch.object(e, "_hidapi") as hid_api, self.assertRaises(RuntimeError):
            e.HidApiTransport(device)
        hid_api.assert_not_called()

    def test_discovery_rejects_wrong_product_and_interface(self):
        valid = dict(vendor_id=e.VID, product_id=e.PID, product_string="EK Connect ",
                     interface_number=0, path=b"test", serial_number="private-serial")
        fake = types.SimpleNamespace(enumerate=lambda *args: [valid, dict(valid, interface_number=1),
                                                               dict(valid, product_string="LED badge"),
                                                               dict(valid, vendor_id=1)])
        with patch.object(e, "_hidapi", return_value=fake):
            self.assertEqual(len(e.enumerate_hidapi()), 1)
            with patch("sys.stdout", new_callable=io.StringIO) as output:
                self.assertEqual(e.main(["--backend", "hidapi", "list"]), 0)
                self.assertNotIn("private-serial", output.getvalue())
                self.assertNotIn("test", output.getvalue())

    def test_hidapi_write_includes_zero_report_id_and_read_normalization(self):
        calls = []
        fake_device = types.SimpleNamespace(open_path=lambda path: None, close=lambda: None,
                    write=lambda frame: calls.append(frame) or len(frame),
                    read=lambda *args: list(b"\x00" + bytes.fromhex(FAN_HEX)))
        with patch.object(e, "_hidapi", return_value=types.SimpleNamespace(device=lambda: fake_device)):
            transport = e.HidApiTransport({"path": b"device"})
            reply = transport.exchange(e.read_request(1))
            self.assertEqual(calls, [b"\x00" + e.read_request(1)])
            self.assertEqual(reply, bytes.fromhex(FAN_HEX))

    def test_short_write_and_timeout_are_errors(self):
        fake = types.SimpleNamespace(open_path=lambda path: None, close=lambda: None,
                                     write=lambda frame: 2, read=lambda *args: [])
        with patch.object(e, "_hidapi", return_value=types.SimpleNamespace(device=lambda: fake)):
            transport = e.HidApiTransport({"path": b"device"})
            with self.assertRaises(e.ProtocolError):
                transport.exchange(e.read_request(1))
            with self.assertRaises(e.ProtocolError):
                transport.exchange(e.read_request(1))
            fake.write = len
            transport = e.HidApiTransport({"path": b"device"})
            with self.assertRaises(TimeoutError):
                transport.exchange(e.read_request(1))
            fake.read = lambda *args: list(bytes.fromhex(FAN_HEX))
            with self.assertRaises(e.ProtocolError):
                transport.exchange(e.read_request(1))

    def test_linux_refuses_custom_driver_before_open(self):
        with patch.object(e.os, "open") as opening, self.assertRaises(RuntimeError):
            e.LinuxTransport({"_driver": "ek-loop-connect", "path": "/dev/not-opened"})
        opening.assert_not_called()


if __name__ == "__main__":
    unittest.main()
