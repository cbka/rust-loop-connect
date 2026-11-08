// SPDX-License-Identifier: GPL-2.0-or-later
//! Exclusive, bounded HID sessions. Opening performs no output reports.
//! Protocol facts: pavelherr/ek-loop-connect and OpenRGB EKController.
//! Do not use concurrently with a kernel hwmon driver or unrelated RGB tools.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("EK Loop Desktop supports Linux and Windows only.");

use crate::protocol::{self, DeviceSnapshot, FanStatus, RgbSettings};
use anyhow::{Result, anyhow, ensure};
use fs2::FileExt;
use hidapi::{HidApi, HidDevice};
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
};

const VENDOR_ID: u16 = 0x0483;
const PRODUCT_ID: u16 = 0x5750;
const PRODUCT_NAME: &str = "EK Connect";
const READ_TIMEOUT_MS: i32 = 2000;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

pub trait Controller: Send {
    fn status(&mut self) -> Result<DeviceSnapshot>;
    fn set_pwm(&mut self, channel: usize, percent: u8) -> Result<FanStatus>;
    fn set_rgb(&mut self, settings: &RgbSettings) -> Result<()>;
}

fn matches_identity(vendor: u16, product: u16, name: Option<&str>, interface: i32) -> bool {
    vendor == VENDOR_ID
        && product == PRODUCT_ID
        && name.is_some_and(|s| s.trim() == PRODUCT_NAME)
        && interface == 0
}
fn matches_device(d: &hidapi::DeviceInfo) -> bool {
    matches_identity(
        d.vendor_id(),
        d.product_id(),
        d.product_string(),
        d.interface_number(),
    )
}

pub fn discover() -> Result<Vec<DeviceInfo>> {
    let api = HidApi::new().map_err(|_| anyhow!("USB-Geräteliste konnte nicht gelesen werden."))?;
    let mut paths: Vec<String> = api
        .device_list()
        .filter(|d| matches_device(d))
        .filter_map(|d| d.path().to_str().ok().map(str::to_owned))
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths
        .into_iter()
        .enumerate()
        .map(|(i, id)| DeviceInfo {
            id,
            name: format!("EK Loop Connect {}", i + 1),
        })
        .collect())
}

pub fn connect(info: &DeviceInfo) -> Result<Box<dyn Controller>> {
    let api = HidApi::new().map_err(|_| anyhow!("USB-Geräteliste konnte nicht gelesen werden."))?;
    let d = api
        .device_list()
        .find(|d| matches_device(d) && d.path().to_str().ok() == Some(info.id.as_str()))
        .ok_or_else(|| anyhow!("Der ausgewählte EK-Controller ist nicht mehr verfügbar."))?;
    // A serial is required rather than inventing an incompatible lock identity.
    let identity = d.serial_number().filter(|s| !s.is_empty()).ok_or_else(|| {
        anyhow!("Controller ohne Seriennummer: sichere gemeinsame Gerätesperre nicht möglich.")
    })?;
    guard_linux_driver(&info.id)?;
    let lock = lock_device(identity)?;
    guard_linux_driver(&info.id)?;
    let handle=api.open_path(d.path()).map_err(|_|anyhow!("Controller konnte nicht geöffnet werden. Zugriffsrechte und andere Steuerprogramme prüfen."))?;
    Ok(Box::new(Connection {
        session: Session {
            wire: UsbTransport { handle },
            poisoned: false,
        },
        _lock: lock,
    }))
}

fn identity_key(identity: &str) -> String {
    let digest = Sha256::digest(identity.as_bytes());
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}

fn lock_path(identity: &str) -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("Benutzerverzeichnis konnte nicht ermittelt werden."))?;
    Ok(base
        .home_dir()
        .join(".cache")
        .join("ek-loop-connect")
        .join(format!("{}.lock", identity_key(identity))))
}

fn lock_device(identity: &str) -> Result<File> {
    let path = lock_path(identity)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Ungültiger Sperrpfad."))?;
    let mut directory = std::fs::DirBuilder::new();
    directory.recursive(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory
        .create(parent)
        .map_err(|_| anyhow!("Gerätesperrverzeichnis konnte nicht erstellt werden."))?;
    ensure!(
        !std::fs::symlink_metadata(parent)?.file_type().is_symlink(),
        "Gerätesperrverzeichnis darf keine symbolische Verknüpfung sein."
    );
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Linux O_NOFOLLOW: do not follow a replacement symlink for the lockfile.
        options.mode(0o600).custom_flags(0x20000);
    }
    let file = options
        .open(path)
        .map_err(|_| anyhow!("Gerätesperre konnte nicht geöffnet werden."))?;
    ensure!(file.metadata()?.is_file(), "Ungültige Gerätesperrdatei.");
    FileExt::try_lock_exclusive(&file)
        .map_err(|_| anyhow!("Controller wird bereits von einem anderen Prozess verwendet."))?;
    // Linux flock matches Python fcntl.flock. Windows LockFileEx covers byte 0,
    // overlapping Python msvcrt.locking's first-byte range.
    Ok(file)
}

#[cfg(target_os = "linux")]
fn guard_linux_driver(id: &str) -> Result<()> {
    let path = Path::new(id);
    ensure!(
        path.parent() == Some(Path::new("/dev")),
        "USB-Gerätepfad konnte nicht sicher geprüft werden."
    );
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Ungültiger USB-Gerätepfad."))?;
    ensure!(
        name.strip_prefix("hidraw")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        "Unbekannter USB-Gerätetyp."
    );
    let hid = Path::new("/sys/class/hidraw")
        .join(name)
        .join("device")
        .canonicalize()
        .map_err(|_| anyhow!("Linux-HID-Treiber konnte nicht geprüft werden."))?;
    let driver = hid
        .join("driver")
        .canonicalize()
        .map_err(|_| anyhow!("Kein überprüfbarer HID-Treiber für den Controller."))?;
    ensure!(
        driver.file_name().is_some_and(|n| n == "hid-generic"),
        "Ein anderer HID-/hwmon-Treiber steuert das Gerät. Rohzugriff und fancontrol dürfen nicht gleichzeitig laufen."
    );
    Ok(())
}
#[cfg(target_os = "windows")]
fn guard_linux_driver(_id: &str) -> Result<()> {
    Ok(())
}

trait Transport: Send {
    fn exchange(&mut self, frame: &[u8; 63]) -> Result<Vec<u8>>;
}
struct UsbTransport {
    handle: HidDevice,
}
fn output_report(frame: &[u8; 63]) -> [u8; 64] {
    let mut output = [0; 64];
    output[1..].copy_from_slice(frame);
    output
}
fn input_report(input: &[u8]) -> Result<Vec<u8>> {
    match input {
        [0, payload @ ..] if input.len() == 64 => Ok(payload.to_vec()),
        payload if payload.len() == 63 => Ok(payload.to_vec()),
        _ => anyhow::bail!("Unvollständiger oder zu langer USB-Report."),
    }
}
impl Transport for UsbTransport {
    fn exchange(&mut self, frame: &[u8; 63]) -> Result<Vec<u8>> {
        let output = output_report(frame);
        let written = self
            .handle
            .write(&output)
            .map_err(|_| anyhow!("USB-Ausgabe fehlgeschlagen. Controller neu verbinden."))?;
        ensure!(
            written == output.len(),
            "Unvollständige USB-Ausgabe. Controller neu verbinden."
        );
        let mut input = [0; 65];
        let count = self
            .handle
            .read_timeout(&mut input, READ_TIMEOUT_MS)
            .map_err(|_| anyhow!("USB-Eingabe fehlgeschlagen. Controller neu verbinden."))?;
        ensure!(
            count > 0,
            "Controller antwortet nicht rechtzeitig. Controller neu verbinden."
        );
        input_report(&input[..count])
    }
}

struct Session<T: Transport> {
    wire: T,
    poisoned: bool,
}
impl<T: Transport> Session<T> {
    fn perform<R>(&mut self, operation: impl FnOnce(&mut T) -> Result<R>) -> Result<R> {
        ensure!(
            !self.poisoned,
            "Verbindung nach Kommunikationsfehler gesperrt. Controller physisch neu verbinden; erneutes Öffnen allein bestätigt keine Synchronisation."
        );
        let result = operation(&mut self.wire);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}
impl<T: Transport> Controller for Session<T> {
    fn status(&mut self) -> Result<DeviceSnapshot> {
        self.perform(|wire| {
            let sensors = wire.exchange(&protocol::read_request(None)?)?;
            let (temperatures_c, flow_raw, level_raw) = protocol::parse_sensors(&sensors)?;
            let mut fans = [FanStatus {
                rpm: 0,
                pwm_percent: 0,
            }; 6];
            for (i, status) in fans.iter_mut().enumerate() {
                *status =
                    protocol::parse_fan(&wire.exchange(&protocol::read_request(Some(i + 1))?)?)?;
            }
            Ok(DeviceSnapshot {
                temperatures_c,
                fans,
                flow_raw,
                level_raw,
            })
        })
    }
    fn set_pwm(&mut self, channel: usize, percent: u8) -> Result<FanStatus> {
        let frame = protocol::pwm_request(channel, percent)?;
        self.perform(|wire| {
            protocol::validate_ack(&wire.exchange(&frame)?)?;
            let status =
                protocol::parse_fan(&wire.exchange(&protocol::read_request(Some(channel))?)?)?;
            ensure!(
                status.pwm_percent == percent,
                "PWM-Rückmeldung stimmt nicht mit dem gesetzten Wert überein."
            );
            Ok(status)
        })
    }
    fn set_rgb(&mut self, settings: &RgbSettings) -> Result<()> {
        let frame = protocol::rgb_request(settings)?;
        self.perform(|wire| protocol::validate_ack(&wire.exchange(&frame)?))
    }
}
struct Connection {
    session: Session<UsbTransport>,
    _lock: File,
}
impl Controller for Connection {
    fn status(&mut self) -> Result<DeviceSnapshot> {
        self.session.status()
    }
    fn set_pwm(&mut self, channel: usize, percent: u8) -> Result<FanStatus> {
        self.session.set_pwm(channel, percent)
    }
    fn set_rgb(&mut self, settings: &RgbSettings) -> Result<()> {
        self.session.set_rgb(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::tests::{ack_fixture, fan_fixture, sensor_fixture};
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    type Calls = Arc<Mutex<Vec<[u8; 63]>>>;
    struct MockWire {
        replies: VecDeque<Result<Vec<u8>>>,
        calls: Calls,
    }
    impl Transport for MockWire {
        fn exchange(&mut self, b: &[u8; 63]) -> Result<Vec<u8>> {
            self.calls.lock().unwrap().push(*b);
            self.replies.pop_front().expect("unexpected I/O")
        }
    }
    fn session(replies: Vec<Result<Vec<u8>>>) -> (Session<MockWire>, Calls) {
        let calls = Arc::new(Mutex::new(vec![]));
        (
            Session {
                wire: MockWire {
                    replies: replies.into(),
                    calls: calls.clone(),
                },
                poisoned: false,
            },
            calls,
        )
    }
    #[test]
    fn no_implicit_startup_writes_and_status_is_read_only() {
        let mut replies = vec![Ok(sensor_fixture())];
        replies.extend((1..=6).map(|_| Ok(fan_fixture(24))));
        let (mut s, calls) = session(replies);
        assert!(calls.lock().unwrap().is_empty());
        let snapshot = s.status().unwrap();
        assert_eq!(snapshot.fans[5].pwm_percent, 24);
        assert_eq!(calls.lock().unwrap().len(), 7);
        assert!(calls.lock().unwrap().iter().all(|b| b[5] == 3));
    }
    #[test]
    fn pwm_requires_ack_and_matching_readback() {
        let (mut s, calls) = session(vec![Ok(ack_fixture()), Ok(fan_fixture(55))]);
        assert_eq!(s.set_pwm(6, 55).unwrap().pwm_percent, 55);
        assert_eq!(&calls.lock().unwrap()[0][6..8], &[0xa1, 0xe0]);
        assert_eq!(calls.lock().unwrap().len(), 2);
    }
    #[test]
    fn malformed_reply_timeout_or_wrong_readback_poison_without_retry() {
        for replies in [
            vec![Ok(vec![0; 63])],
            vec![Err(anyhow!("timeout"))],
            vec![Ok(ack_fixture()), Ok(fan_fixture(54))],
        ] {
            let (mut s, calls) = session(replies);
            assert!(s.set_pwm(1, 55).is_err());
            let count = calls.lock().unwrap().len();
            assert!(s.status().is_err());
            assert!(s.set_rgb(&RgbSettings::default()).is_err());
            assert!(s.set_pwm(1, 55).is_err());
            assert_eq!(calls.lock().unwrap().len(), count);
        }
    }
    #[test]
    fn malformed_status_poisons_session() {
        let (mut s, calls) = session(vec![Ok(vec![0; 62])]);
        assert!(s.status().is_err());
        assert!(s.set_pwm(1, 50).is_err());
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
    #[test]
    fn invalid_user_input_does_not_send_or_poison() {
        let (mut s, calls) = session(vec![
            Ok(sensor_fixture()),
            Ok(fan_fixture(24)),
            Ok(fan_fixture(24)),
            Ok(fan_fixture(24)),
            Ok(fan_fixture(24)),
            Ok(fan_fixture(24)),
            Ok(fan_fixture(24)),
        ]);
        assert!(s.set_pwm(7, 50).is_err());
        assert!(s.set_pwm(1, 101).is_err());
        assert!(calls.lock().unwrap().is_empty());
        assert!(s.status().is_ok());
    }
    #[test]
    fn strict_identity_rejects_shared_vidpid_and_wrong_interface() {
        assert!(matches_identity(
            VENDOR_ID,
            PRODUCT_ID,
            Some("EK Connect "),
            0
        ));
        assert!(!matches_identity(
            VENDOR_ID,
            PRODUCT_ID,
            Some("LED badge"),
            0
        ));
        assert!(!matches_identity(
            VENDOR_ID,
            PRODUCT_ID,
            Some("EK Connect"),
            1
        ));
        assert!(!matches_identity(
            VENDOR_ID,
            PRODUCT_ID,
            Some("EK Connect"),
            -1
        ));
        assert!(!matches_identity(1, PRODUCT_ID, Some("EK Connect"), 0));
        assert!(!matches_identity(VENDOR_ID, PRODUCT_ID, None, 0));
    }
    #[test]
    fn report_id_prefix_and_input_sizes_match_hidapi_contract() {
        let frame = protocol::read_request(Some(6)).unwrap();
        let report = output_report(&frame);
        assert_eq!(report[0], 0);
        assert_eq!(&report[1..], &frame);
        let fan = fan_fixture(24);
        assert_eq!(input_report(&fan).unwrap(), fan);
        let mut prefixed = vec![0];
        prefixed.extend_from_slice(&fan);
        assert_eq!(input_report(&prefixed).unwrap(), fan);
        prefixed[0] = 1;
        assert!(input_report(&prefixed).is_err());
        for count in [0, 62, 65] {
            assert!(input_report(&vec![0; count]).is_err());
        }
    }
    #[test]
    fn shared_identity_hash_matches_python_contract() {
        assert_eq!(identity_key("test"), "9f86d081884c7d659a2feaa0");
    }
}
