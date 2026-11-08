// SPDX-License-Identifier: GPL-2.0-or-later
//! One worker owns the HID connection. Opening, polling and saving do not set PWM.
use crate::{
    config::{ControlMode, Profile},
    curve,
    device::{self, Controller, DeviceInfo},
    protocol::{DeviceSnapshot, FanStatus, RgbSettings},
};
use anyhow::{Result, bail};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_SENSOR_AGE: Duration = Duration::from_secs(6);

#[derive(Debug)]
pub enum Command {
    Discover,
    Connect(usize),
    Disconnect,
    Refresh,
    SetPwm { channel: usize, percent: u8 },
    SetRgb(RgbSettings),
    StartCurves(Box<Profile>),
    StopCurves,
    Shutdown,
}
#[derive(Debug)]
pub enum Event {
    Devices(Vec<DeviceInfo>),
    Connected(String),
    Snapshot(DeviceSnapshot),
    Disconnected,
    Curves(bool),
    Notice(String),
    Error(String),
}

pub struct Service {
    commands: SyncSender<Command>,
    events: Receiver<Event>,
    join: Option<JoinHandle<()>>,
    stopping: Arc<AtomicBool>,
}
impl Service {
    pub fn start(demo: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(16);
        let (events_tx, events_rx) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = Arc::clone(&stopping);
        let join = thread::spawn(move || {
            let mut worker = Worker::new(demo, events_tx);
            worker.discover();
            run_worker(worker, rx, worker_stopping);
        });
        Self {
            commands: tx,
            events: events_rx,
            join: Some(join),
            stopping,
        }
    }
    pub fn send(&self, command: Command) -> Result<()> {
        if self.stopping.load(Ordering::Acquire) {
            bail!("Anwendung wird bereits beendet");
        }
        self.commands
            .try_send(command)
            .map_err(|e| anyhow::anyhow!("Befehl konnte nicht gesendet werden: {e}"))
    }
    pub fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
    pub fn shutdown(&mut self) {
        if let Some(join) = self.join.take() {
            self.stopping.store(true, Ordering::Release);
            // Wake an idle receiver, but never wait behind a full command queue.
            let _ = self.commands.try_send(Command::Shutdown);
            let _ = join.join();
        }
    }
}

fn run_worker(mut worker: Worker, commands: Receiver<Command>, stopping: Arc<AtomicBool>) {
    let mut next_poll = Instant::now() + POLL_INTERVAL;
    loop {
        if stopping.load(Ordering::Acquire) {
            worker.disconnect();
            break;
        }
        let wait = next_poll.saturating_duration_since(Instant::now());
        match commands.recv_timeout(wait) {
            Ok(command) => {
                // A close request overtakes all queued user commands.
                if stopping.load(Ordering::Acquire) || matches!(command, Command::Shutdown) {
                    worker.disconnect();
                    break;
                }
                worker.handle(command);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                worker.disconnect();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if !stopping.load(Ordering::Acquire) && Instant::now() >= next_poll {
            worker.poll();
            next_poll = Instant::now() + POLL_INTERVAL;
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Worker {
    demo: bool,
    devices: Vec<DeviceInfo>,
    controller: Option<Box<dyn Controller>>,
    profile: Option<Profile>,
    events: mpsc::Sender<Event>,
}
impl Worker {
    fn new(demo: bool, events: mpsc::Sender<Event>) -> Self {
        Self {
            demo,
            devices: Vec::new(),
            controller: None,
            profile: None,
            events,
        }
    }
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }
    fn discover(&mut self) {
        let found = if self.demo {
            Ok(vec![DeviceInfo {
                id: "demo".into(),
                name: "EK Loop Connect · Demo".into(),
            }])
        } else {
            device::discover()
        };
        match found {
            Ok(devices) => {
                self.devices = devices;
                self.emit(Event::Devices(self.devices.clone()));
            }
            Err(error) => self.emit(Event::Error(format!("Gerätesuche: {error:#}"))),
        }
    }
    fn handle(&mut self, command: Command) {
        match command {
            Command::Discover => self.discover(),
            Command::Connect(index) => self.connect(index),
            Command::Disconnect => self.disconnect(),
            Command::Refresh => {
                if self.controller.is_some() {
                    self.poll();
                } else {
                    self.emit(Event::Error("Kein Controller verbunden.".into()));
                }
            }
            Command::SetPwm { channel, percent } => self.manual(channel, percent),
            Command::SetRgb(settings) => self.rgb(settings),
            Command::StartCurves(profile) => self.start_curves(*profile),
            Command::StopCurves => {
                if let Err(e) = self.stop_curves() {
                    self.fault(e);
                }
            }
            Command::Shutdown => self.disconnect(),
        }
    }
    fn connect(&mut self, index: usize) {
        let Some(info) = self.devices.get(index).cloned() else {
            self.emit(Event::Error(
                "Bitte ein vorhandenes Gerät auswählen.".into(),
            ));
            return;
        };
        self.disconnect();
        let result: Result<Box<dyn Controller>> = if self.demo {
            Ok(Box::new(DemoController {
                snapshot: demo_snapshot(),
            }))
        } else {
            device::connect(&info)
        };
        match result {
            Ok(controller) => {
                self.controller = Some(controller);
                self.emit(Event::Connected(info.name));
                self.poll();
            }
            Err(error) => self.emit(Event::Error(format!("Verbindung: {error:#}"))),
        }
    }
    fn fault(&mut self, error: anyhow::Error) {
        self.profile = None;
        self.controller = None;
        self.emit(Event::Curves(false));
        self.emit(Event::Disconnected);
        self.emit(Event::Error(format!("Gerätekommunikation beendet: {error:#}. Nach einem Übertragungsfehler USB neu verbinden. Ein Sicherheitswert konnte bei unterbrochener Kommunikation nicht garantiert werden.")));
    }
    fn disconnect(&mut self) {
        if let Err(error) = self.stop_curves() {
            self.fault(error);
        }
        self.controller = None;
        self.emit(Event::Disconnected);
    }
    fn poll(&mut self) {
        let started = Instant::now();
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut snapshot = match controller.status() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.fault(error);
                return;
            }
        };
        if let Some(profile) = self.profile.as_ref() {
            for (index, fan) in profile.fans.iter().enumerate() {
                if fan.mode != ControlMode::Curve {
                    continue;
                }
                let temperature = if started.elapsed() <= MAX_SENSOR_AGE {
                    snapshot.temperatures_c[fan.sensor]
                } else {
                    None
                };
                let target = match curve::target(&fan.points, temperature) {
                    Ok(target) => target,
                    Err(error) => {
                        self.fault(error);
                        return;
                    }
                };
                if snapshot.fans[index].pwm_percent != target {
                    match controller.set_pwm(index + 1, target) {
                        Ok(actual) => snapshot.fans[index] = actual,
                        Err(error) => {
                            self.fault(error);
                            return;
                        }
                    }
                }
            }
        }
        if started.elapsed() > MAX_SENSOR_AGE {
            snapshot.temperatures_c = [None; 3];
            self.emit(Event::Notice("Messwertabfrage zu langsam; Temperaturwerte gelten als veraltet. Aktive Kurven verwenden 100 %.".into()));
        }
        self.emit(Event::Snapshot(snapshot));
    }
    fn manual(&mut self, channel: usize, percent: u8) {
        if !(1..=6).contains(&channel) || percent > 100 {
            self.emit(Event::Error("Kanal muss 1–6 und PWM 0–100 % sein.".into()));
            return;
        }
        if self
            .profile
            .as_ref()
            .is_some_and(|p| p.fans[channel - 1].mode == ControlMode::Curve)
        {
            self.emit(Event::Error(
                "Für manuelle Werte zuerst die laufende Kurvenregelung stoppen.".into(),
            ));
            return;
        }
        let Some(controller) = self.controller.as_mut() else {
            self.emit(Event::Error("Kein Controller verbunden.".into()));
            return;
        };
        match controller.set_pwm(channel, percent) {
            Ok(actual) => {
                self.emit(Event::Notice(format!(
                    "F{channel}: {} % bestätigt",
                    actual.pwm_percent
                )));
                self.poll();
            }
            Err(error) => self.fault(error),
        }
    }
    fn rgb(&mut self, settings: RgbSettings) {
        if settings.speed > 100 || settings.brightness > 100 {
            self.emit(Event::Error(
                "Geschwindigkeit und Helligkeit müssen 0–100 sein.".into(),
            ));
            return;
        }
        let Some(controller) = self.controller.as_mut() else {
            self.emit(Event::Error("Kein Controller verbunden.".into()));
            return;
        };
        match controller.set_rgb(&settings) {
            Ok(()) => self.emit(Event::Notice(
                "Gemeinsame RGB-Einstellung bestätigt.".into(),
            )),
            Err(error) => self.fault(error),
        }
    }
    fn start_curves(&mut self, profile: Profile) {
        if let Err(error) = profile.validate() {
            self.emit(Event::Error(format!("Ungültiges Profil: {error:#}")));
            return;
        }
        if !profile
            .fans
            .iter()
            .any(|fan| fan.mode == ControlMode::Curve)
        {
            self.emit(Event::Error(
                "Mindestens einen Kanal auf Temperaturkurve stellen.".into(),
            ));
            return;
        }
        if self.controller.is_none() {
            self.emit(Event::Error("Kein Controller verbunden.".into()));
            return;
        }
        // Stop an earlier curve profile first, including channels no longer selected.
        if let Err(error) = self.stop_curves() {
            self.fault(error);
            return;
        }
        self.profile = Some(profile);
        self.emit(Event::Curves(true));
        self.emit(Event::Notice("Kurvenregelung aktiv. Ohne gültigen Fühler gilt 100 %. Beim Stoppen/Beenden werden diese Kanäle auf 100 % gesetzt.".into()));
        self.poll();
    }
    fn stop_curves(&mut self) -> Result<()> {
        let Some(profile) = self.profile.take() else {
            self.emit(Event::Curves(false));
            return Ok(());
        };
        self.emit(Event::Curves(false));
        let Some(controller) = self.controller.as_mut() else {
            bail!("Keine Verbindung zum Setzen der Sicherheitswerte");
        };
        for (index, fan) in profile.fans.iter().enumerate() {
            if fan.mode == ControlMode::Curve {
                controller.set_pwm(index + 1, 100)?;
            }
        }
        self.emit(Event::Notice(
            "Kurven gestoppt; zuvor geregelte Kanäle auf 100 % gesetzt.".into(),
        ));
        Ok(())
    }
}

pub fn demo_snapshot() -> DeviceSnapshot {
    DeviceSnapshot {
        temperatures_c: [Some(28.0), Some(31.0), None],
        fans: [
            FanStatus {
                rpm: 1000,
                pwm_percent: 40,
            },
            FanStatus {
                rpm: 1250,
                pwm_percent: 50,
            },
            FanStatus {
                rpm: 1100,
                pwm_percent: 44,
            },
            FanStatus {
                rpm: 0,
                pwm_percent: 0,
            },
            FanStatus {
                rpm: 950,
                pwm_percent: 38,
            },
            FanStatus {
                rpm: 1800,
                pwm_percent: 72,
            },
        ],
        flow_raw: 0,
        level_raw: 0,
    }
}
struct DemoController {
    snapshot: DeviceSnapshot,
}
impl Controller for DemoController {
    fn status(&mut self) -> Result<DeviceSnapshot> {
        Ok(self.snapshot.clone())
    }
    fn set_pwm(&mut self, channel: usize, percent: u8) -> Result<FanStatus> {
        if !(1..=6).contains(&channel) || percent > 100 {
            bail!("Ungültiger Demowert");
        }
        let fan = FanStatus {
            rpm: u16::from(percent) * 25,
            pwm_percent: percent,
        };
        self.snapshot.fans[channel - 1] = fan;
        Ok(fan)
    }
    fn set_rgb(&mut self, _: &RgbSettings) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Calls {
        writes: Vec<(usize, u8)>,
        fail_status: bool,
    }
    struct Fake {
        state: Arc<Mutex<Calls>>,
        snapshot: DeviceSnapshot,
    }
    impl Controller for Fake {
        fn status(&mut self) -> Result<DeviceSnapshot> {
            if self.state.lock().unwrap().fail_status {
                bail!("unplugged");
            }
            Ok(self.snapshot.clone())
        }
        fn set_pwm(&mut self, channel: usize, percent: u8) -> Result<FanStatus> {
            self.state.lock().unwrap().writes.push((channel, percent));
            self.snapshot.fans[channel - 1].pwm_percent = percent;
            Ok(self.snapshot.fans[channel - 1])
        }
        fn set_rgb(&mut self, _: &RgbSettings) -> Result<()> {
            Ok(())
        }
    }
    fn worker() -> (Worker, Arc<Mutex<Calls>>, Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Calls::default()));
        let mut w = Worker::new(false, tx);
        w.controller = Some(Box::new(Fake {
            state: state.clone(),
            snapshot: demo_snapshot(),
        }));
        (w, state, rx)
    }
    #[test]
    fn shutdown_discards_queued_controls_and_only_restores_active_curves() {
        let (mut w, calls, _) = worker();
        let mut p = Profile::default();
        p.fans[0].mode = ControlMode::Curve;
        w.start_curves(p);
        calls.lock().unwrap().writes.clear();
        let (tx, rx) = mpsc::sync_channel(16);
        tx.send(Command::SetPwm {
            channel: 2,
            percent: 0,
        })
        .unwrap();
        tx.send(Command::StartCurves(Box::default())).unwrap();
        let stopping = Arc::new(AtomicBool::new(true));
        run_worker(w, rx, stopping);
        assert_eq!(calls.lock().unwrap().writes, [(1, 100)]);
    }

    #[test]
    fn polling_and_profile_defaults_never_write() {
        let (mut w, calls, _) = worker();
        w.poll();
        w.poll();
        assert!(calls.lock().unwrap().writes.is_empty());
        assert!(w.profile.is_none());
    }
    #[test]
    fn missing_selected_probe_sets_only_selected_fan_to_full() {
        let (mut w, calls, _) = worker();
        let mut p = Profile::default();
        p.fans[0].mode = ControlMode::Curve;
        p.fans[0].sensor = 2;
        w.start_curves(p);
        w.poll();
        assert_eq!(calls.lock().unwrap().writes, [(1, 100)]);
        w.stop_curves().unwrap();
        assert_eq!(calls.lock().unwrap().writes, [(1, 100), (1, 100)]);
    }
    #[test]
    fn transport_failure_clears_curve_and_does_not_attempt_fallback_writes() {
        let (mut w, calls, events) = worker();
        let mut p = Profile::default();
        p.fans[5].mode = ControlMode::Curve;
        calls.lock().unwrap().fail_status = true;
        w.start_curves(p);
        w.poll();
        assert!(w.controller.is_none());
        assert!(w.profile.is_none());
        assert!(calls.lock().unwrap().writes.is_empty());
        assert!(events.try_iter().any(|e| matches!(e, Event::Disconnected)));
    }
    #[test]
    fn manual_does_not_override_active_curve() {
        let (mut w, calls, _) = worker();
        let mut p = Profile::default();
        p.fans[0].mode = ControlMode::Curve;
        w.start_curves(p);
        let count = calls.lock().unwrap().writes.len();
        w.manual(1, 0);
        assert_eq!(calls.lock().unwrap().writes.len(), count);
    }
    #[test]
    fn invalid_profile_is_rejected_before_hardware_io() {
        let (mut w, calls, _) = worker();
        let mut p = Profile::default();
        p.fans[0].mode = ControlMode::Curve;
        p.fans[0].sensor = 9;
        w.start_curves(p);
        assert!(w.profile.is_none());
        assert!(calls.lock().unwrap().writes.is_empty());
    }
    #[test]
    fn disconnect_restores_only_curve_channels_and_drops_connection() {
        let (mut w, calls, _) = worker();
        let mut p = Profile::default();
        p.fans[5].mode = ControlMode::Curve;
        w.start_curves(p);
        calls.lock().unwrap().writes.clear();
        w.disconnect();
        assert_eq!(calls.lock().unwrap().writes, [(6, 100)]);
        assert!(w.controller.is_none());
    }
    #[test]
    fn demo_backend_lifecycle_has_no_external_device_dependency() {
        let mut service = Service::start(true);
        service.send(Command::Connect(0)).unwrap();
        loop {
            if matches!(
                service.events.recv_timeout(Duration::from_secs(2)).unwrap(),
                Event::Snapshot(_)
            ) {
                break;
            }
        }
        service.shutdown();
    }
}
