# Implementation contract — 2026-11-08

Native Rust application for Linux and Windows; Kubuntu first. Original UI assets and decompiled implementation are not copied. GPL attribution to pavelherr and OpenRGB for protocol facts. Primary workflows: real sensor dashboard, six PWM controls (F6 pump mapping A1E0), deliberate curve control from T1–T3, global RGB effects, local profiles. No firmware flashing, no inferred per-port RGB, no CPU/GPU vendor SDK dependencies. FLO/LVL raw readings until presence/calibration verified. No macOS deliverable.

## Ownership

Main owns Cargo.toml, Cargo.lock, lib.rs, main.rs, service.rs, workflows, integration and release docs.
Device agent owns src/protocol.rs and src/device.rs (+their internal tests).
Configuration agent owns src/config.rs and src/curve.rs (+their internal tests).
GUI agent owns src/gui.rs and docs/UI-CONTRACT.md; review this contract first, flag conflicts then implement. Do not edit other agents' files. All code uses anyhow::Result. No target device writes by agents. Main may perform read-only hardware acceptance after testing.

## Fixed Rust module interfaces

protocol:
- const FAN_CHANNELS: [[u8;2];6] = A0A0,A0C0,A0E0,A100,A120,A1E0.
- FanStatus { rpm:u16, pwm_percent:u8 }, Copy+Clone+Debug+Serialize+Deserialize.
- DeviceSnapshot { temperatures_c:[Option<f32>;3], fans:[FanStatus;6], flow_raw:u16, level_raw:u8 }, Clone+Debug+Serialize+Deserialize.
- RgbEffect enum (Static, Breathing, Fading, Marquee, CoveringMarquee, Pulse, Wave, Alternating, Candle), Copy+Clone+Debug+Eq+PartialEq+Serialize+Deserialize; fn label(&self)->&'static str; const ALL:[RgbEffect;9].
- RgbSettings { effect:RgbEffect, speed:u8, brightness:u8, color:[u8;3] }, Clone+Debug+Serialize+Deserialize+Default.
- read_request(channel:Option<usize>)->Result<[u8;63]> (None=sensors, Some1..6=fans); pwm_request(channel:usize,percent:u8)->Result<[u8;63]>; rgb_request(&RgbSettings)->Result<[u8;63]>; parse_sensors(&[u8])->Result<([Option<f32>;3],u16,u8)>; parse_fan(&[u8])->Result<FanStatus>; validate_ack(&[u8])->Result<()>.

device:
- DeviceInfo { id:String, name:String }, Clone+Debug. id is backend path, public UI shows name only.
- trait Controller:Send { fn status(&mut self)->Result<DeviceSnapshot>; fn set_pwm(&mut self,channel:usize,percent:u8)->Result<FanStatus>; fn set_rgb(&mut self,settings:&RgbSettings)->Result<()>; }
- discover()->Result<Vec<DeviceInfo>>; connect(&DeviceInfo)->Result<Box<dyn Controller>>.
- Strict VIDPID+EK Connect+interface0. Linux reject custom hwmon driver; common Python/Rust per-user hashed lock. Never open other hardware by shared VIDPID alone. Read timeout bounded; any malformed/timeout/transport failure poisons session and forbids further writes. No startup writes, no automatic retry after poisoned session. F6 retains A1E0. Read-back PWM after explicit write.

config:
- ControlMode enum (Manual,Curve), Copy+Clone+Debug+Eq+PartialEq+Serialize+Deserialize.
- CurvePoint { temperature:f32, percent:u8 }, Clone+Copy+Debug+Serialize+Deserialize.
- FanProfile { name:String, mode:ControlMode, manual_percent:u8, sensor:usize, points:Vec<CurvePoint> }, Clone+Debug+Serialize+Deserialize. sensor=0..2. Every field public.
- Profile { version:u32, fans:[FanProfile;6], rgb:RgbSettings }, Clone+Debug+Default+Serialize+Deserialize.
- Profile::validate()->Result<()>; profile_path()->Result<PathBuf>; load(&Path)->Result<Profile>; save(&Path,&Profile)->Result<()>.
- Reject malformed/oversized data, NaN, percentages>100, bad indices and non-increasing curve points; temp range0..120, at least2 and at most16 points, default curve [(25,30),(35,60),(45,100)]. Save atomically per platform; no silent overwriting malformed profile with defaults. Default all Manual, do not autoapply on app launch.

curve:
- target(points:&[CurvePoint],temperature:Option<f32>)->Result<u8>: validated piecewise linear interpolation/clamp, ceil to avoid rounding below intended duty, missing/invalid temperature returns100 (failsafe). Validate points even if temp missing. Never write device here.

service (main owns):
- enum Command { Discover,Connect(usize),Disconnect,Refresh,SetPwm{channel:usize,percent:u8},SetRgb(RgbSettings),StartCurves(Profile),StopCurves,Shutdown }.
- enum Event { Devices(Vec<DeviceInfo>),Connected(String),Snapshot(DeviceSnapshot),Disconnected,Curves(bool),Notice(String),Error(String) }.
- struct Service; Service::start(demo:bool)->Self; send(&self,Command)->Result<()> (bounded try_send); drain(&self)->Vec<Event>; shutdown(&mut self). Drop shuts down and joins.
- Single worker exclusively owns controller. Initial discover only (GUI can connect if exactlyone, read queries only), polling every2s. Actual curves run ONLY after explicit StartCurves, use last fresh successful sensor snapshot, changed percentages only. Missing sensor =>100 for active curve. StopCurves/Disconnect/Shutdown set only active curve-controlled channels to100 before stopping when communication healthy. Poisoned connection cannot promise fallback; do not send further commands, report need reconnect. Every attempted user operation produces clear Event result. Demo is isolated in-memory controller, prominently labeled; no HID API called. No placeholder data in real mode.

GUI:
- pub struct GuiOptions { pub demo:bool, pub capture_path:Option<PathBuf>, pub size:[f32;2] }; pub fn run(options:GuiOptions)->eframe::Result<()>.
- Creates Service::start(options.demo), profile_path/load with error visible, graceful shutdown; missing config uses defaults. Shows status, tabs Übersicht/Lüfter/Beleuchtung/Profile, device select+connect+refresh; sensor values with unavailable marker, RPM/PWM six lines, flow/lvl raw. Manual slider changes draft only, explicit apply one channel; curves per fan editor with sensor select, enable mode, points and chart, global start/stop. RGB nine effects/color/speed/brightness global apply. Profiles save/load explicit, loaded config never autoapplies. No per-port RGB controls. Clear F6 label pump-compatible.
- Calm industrial dashboard: charcoal panels, warm white text, cyan live accent, amber partial/unavailable. Quiet typography and compact controls, no marketing content. Scannable actual app firstscreen. Target960x720 and1280x900; scroll on narrow screens; keyboard reachable labeled controls.
- --demo with optional screenshot path: handle egui Screenshot event after first stable frames, save image to requested path, close only after screenshot saved. Non-demo default must never synthesize sensor values. A capture failure must be visible and propagate process failure if practical. Main CLI adds --status and --list.

## Acceptance plan

1. Tests FIRST for protocol malformed replies, F6 mapping, missing sensor, ACK/readback; curve boundaries/failsafe; invalid profiles and roundtrips; worker behavior/polling/no implicit startup writes.
2. cargo fmt --check, cargo test, cargo clippy --all-targets -- -D warnings; local debug/release Linux builds.
3. CLI list/status query actual EK on Kubuntu; no writes until attached fan is confirmed. Record actual readings, do not claim physical PWM acceptance from a mock.
4. Native demo screenshots at960x720 and1280x900 plus error/partial states; inspect images, labels, overflow and keyboard focus. Evidence gate truthful, pending states do not become fabricated PASS.
5. CI native Linux/Windows builds+tests with artifacts. Source public in cbka/ek-loop-connect, no original vendor binaries/decompilation. Package Linux binary with desktop launcher; Windows artifact build evidence separate from hardware acceptance.
6. User-facing report distinguishes finished implementation/builds from pending Windows controller test and physical fan/curve calibration. No unrequested background service/autostart. Stop created QA processes; retain only explicitly delivered app.
