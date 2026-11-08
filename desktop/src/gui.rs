// SPDX-License-Identifier: GPL-2.0-or-later
//! Native, deliberately applied controls. Device I/O belongs to the service worker.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    config::{self, ControlMode, CurvePoint, Profile},
    device::DeviceInfo,
    protocol::{DeviceSnapshot, RgbEffect},
    service::{Command, Event, Service},
};
use eframe::egui::{self, Color32, RichText};

const CYAN: Color32 = Color32::from_rgb(80, 211, 215);
const AMBER: Color32 = Color32::from_rgb(238, 184, 91);
const MUTED: Color32 = Color32::from_rgb(155, 168, 179);
const PANEL: Color32 = Color32::from_rgb(30, 38, 46);

pub struct GuiOptions {
    pub demo: bool,
    pub capture_path: Option<PathBuf>,
    pub capture_view: Option<String>,
    pub size: [f32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Fans,
    Lighting,
    Profiles,
}
impl Tab {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Overview, "Übersicht"),
        (Self::Fans, "Lüfter"),
        (Self::Lighting, "Beleuchtung"),
        (Self::Profiles, "Profile"),
    ];
}

type CaptureResult = Arc<Mutex<Option<Result<(), String>>>>;

pub fn run(options: GuiOptions) -> eframe::Result<()> {
    if options.capture_view.is_some() && (!options.demo || options.capture_path.is_none()) {
        return Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
            "Prüfansichten benötigen --demo und einen Screenshot-Pfad.",
        ))));
    }
    if options.capture_path.is_some() && !options.demo {
        return Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
            "Screenshot-Aufnahmen sind ausschließlich mit --demo erlaubt.",
        ))));
    }
    let capture_requested = options.capture_path.is_some();
    let capture_result: CaptureResult = Arc::new(Mutex::new(None));
    let result_copy = capture_result.clone();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(options.size)
            .with_min_inner_size([760.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        &format!("Rust Loop Connect {}", env!("CARGO_PKG_VERSION")),
        native,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(Desktop::new(options, result_copy)))
        }),
    )?;
    if capture_requested {
        let saved = capture_result.lock().map_err(|_| {
            eframe::Error::AppCreation(Box::new(std::io::Error::other(
                "Screenshot-Status nicht verfügbar",
            )))
        })?;
        match saved.as_ref() {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                return Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
                    error.clone(),
                ))));
            }
            None => {
                return Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
                    "Fenster vor erfolgreicher Screenshot-Aufnahme geschlossen",
                ))));
            }
        }
    }
    Ok(())
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(20, 27, 34);
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(14, 20, 26);
    style.visuals.override_text_color = Some(Color32::from_rgb(232, 238, 241));
    style.visuals.selection.bg_fill = Color32::from_rgb(35, 92, 102);
    style.visuals.selection.stroke.color = CYAN;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(39, 106, 112);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 65, 77);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.slider_width = 230.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    ctx.set_style(style);
}

struct Desktop {
    service: Service,
    demo: bool,
    devices: Vec<DeviceInfo>,
    selected_device: usize,
    connected: Option<String>,
    snapshot: Option<DeviceSnapshot>,
    updated: Option<Instant>,
    tab: Tab,
    selected_fan: usize,
    profile: Profile,
    profile_path: Option<PathBuf>,
    curves_active: bool,
    notice: String,
    error: Option<String>,
    capture_path: Option<PathBuf>,
    capture_requested: Option<Instant>,
    capture_result: CaptureResult,
    frames: usize,
    opened: Instant,
    auto_connected: bool,
    preview_state: Option<String>,
    show_about: bool,
}

impl Desktop {
    fn new(options: GuiOptions, capture_result: CaptureResult) -> Self {
        let mut error = None;
        let profile_path = match config::profile_path() {
            Ok(path) => Some(if options.demo {
                path.with_file_name("demo-profile.json")
            } else {
                path
            }),
            Err(e) => {
                error = Some(format!("Profilpfad: {e:#}"));
                None
            }
        };
        let source_path = if options.capture_path.is_some() {
            None
        } else {
            profile_path
                .as_ref()
                .filter(|p| p.exists())
                .cloned()
                .or_else(|| {
                    if options.demo {
                        None
                    } else {
                        config::legacy_profile_path().filter(|p| p.exists())
                    }
                })
        };
        let migrated = source_path.is_some() && source_path.as_ref() != profile_path.as_ref();
        let profile = match source_path.as_ref() {
            Some(path) => match config::load(path) {
                Ok(profile) => profile,
                Err(e) => {
                    error = Some(format!(
                        "Profil wurde nicht geladen: {e:#}. Die Datei bleibt unverändert."
                    ));
                    Profile::default()
                }
            },
            None => Profile::default(),
        };
        let preview = options.capture_view.as_deref();
        let tab = match preview {
            Some("fans" | "curve") => Tab::Fans,
            Some("lighting") => Tab::Lighting,
            Some("profiles") => Tab::Profiles,
            _ => Tab::Overview,
        };
        let mut profile = profile;
        if preview == Some("curve") {
            profile.fans[0].mode = ControlMode::Curve;
        }
        let preview_state = options
            .capture_view
            .clone()
            .filter(|v| matches!(v.as_str(), "empty" | "error" | "permission" | "loading"));
        Self {
            service: Service::start(options.demo),
            demo: options.demo,
            devices: Vec::new(),
            selected_device: 0,
            connected: None,
            snapshot: None,
            updated: None,
            tab,
            selected_fan: 0,
            profile,
            profile_path,
            curves_active: false,
            notice: if migrated {
                "Vorhandenes Profil als Entwurf übernommen. Mit Speichern im neuen Profilordner ablegen.".into()
            } else {
                "Keine Regelung gestartet. Änderungen zunächst als Entwurf.".into()
            },
            error,
            capture_path: options.capture_path,
            capture_requested: None,
            capture_result,
            frames: 0,
            opened: Instant::now(),
            auto_connected: preview_state.is_some(),
            preview_state,
            show_about: false,
        }
    }

    fn send(&mut self, command: Command) {
        match self.service.send(command) {
            Ok(()) => {
                self.notice = "Anfrage wird bearbeitet …".into();
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Anfrage fehlgeschlagen: {e:#}")),
        }
    }

    fn receive(&mut self) {
        for event in self.service.drain() {
            match event {
                Event::Devices(devices) => {
                    self.devices = devices;
                    self.selected_device = self
                        .selected_device
                        .min(self.devices.len().saturating_sub(1));
                    if self.devices.len() == 1 && !self.auto_connected && self.connected.is_none() {
                        self.auto_connected = true;
                        self.send(Command::Connect(0));
                    }
                }
                Event::Connected(name) => {
                    self.connected = Some(name);
                    self.notice = "Verbunden. Messwerte werden alle 2 Sekunden abgefragt.".into();
                }
                Event::Snapshot(snapshot) => {
                    self.snapshot = Some(snapshot);
                    self.updated = Some(Instant::now());
                }
                Event::Disconnected => {
                    self.connected = None;
                    self.snapshot = None;
                    self.updated = None;
                    self.curves_active = false;
                    self.notice = "Verbindung getrennt.".into();
                }
                Event::Curves(active) => {
                    self.curves_active = active;
                }
                Event::Notice(text) => self.notice = text,
                Event::Error(text) => self.error = Some(text),
            }
        }
    }

    fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            egui::Frame::new().inner_margin(18).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("RUST LOOP CONNECT").strong().size(22.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Info").clicked() {
                            self.show_about = true;
                        }
                        if self.demo {
                            ui.label(
                                RichText::new("DEMO · simulierte Daten")
                                    .color(AMBER)
                                    .strong(),
                            );
                        } else {
                            ui.label(
                                RichText::new(if self.connected.is_some() {
                                    "● USB verbunden"
                                } else {
                                    "○ Nicht verbunden"
                                })
                                .color(
                                    if self.connected.is_some() {
                                        CYAN
                                    } else {
                                        MUTED
                                    },
                                ),
                            );
                        }
                    });
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    for (tab, title) in Tab::ALL {
                        if ui.selectable_label(self.tab == tab, title).clicked() {
                            self.tab = tab;
                        }
                    }
                });
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(18, 10))
                .show(ui, |ui| {
                    if let Some(error) = &self.error {
                        ui.colored_label(AMBER, format!("Hinweis: {error}"));
                    }
                    ui.label(RichText::new(&self.notice).small().color(MUTED));
                });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("page-scroll")
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("controller")
                            .selected_text(
                                self.devices
                                    .get(self.selected_device)
                                    .map(|d| d.name.as_str())
                                    .unwrap_or("Kein Controller gefunden"),
                            )
                            .show_ui(ui, |ui| {
                                for (i, device) in self.devices.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_device, i, &device.name);
                                }
                            });
                        if ui
                            .add_enabled(
                                !self.devices.is_empty() && self.connected.is_none(),
                                egui::Button::new("Verbinden"),
                            )
                            .clicked()
                        {
                            self.send(Command::Connect(self.selected_device));
                        }
                        if ui
                            .add_enabled(self.connected.is_some(), egui::Button::new("Trennen"))
                            .clicked()
                        {
                            self.send(Command::Disconnect);
                        }
                        if ui.button("Geräte suchen").clicked() {
                            self.auto_connected = true;
                            self.send(Command::Discover);
                        }
                        if ui
                            .add_enabled(
                                self.connected.is_some(),
                                egui::Button::new("Aktualisieren"),
                            )
                            .clicked()
                        {
                            self.send(Command::Refresh);
                        }
                    });
                    ui.add_space(16.0);
                    match self.tab {
                        Tab::Overview => self.overview(ui),
                        Tab::Fans => self.fans(ui),
                        Tab::Lighting => self.lighting(ui),
                        Tab::Profiles => self.profiles(ui),
                    }
                    ui.add_space(12.0);
                });
        });
        let profile_path = self.profile_path.clone();
        egui::Window::new("Über Rust Loop Connect")
            .open(&mut self.show_about)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading(format!("Rust Loop Connect {}", env!("CARGO_PKG_VERSION")));
                ui.label("Unabhängige Open-Source-Anwendung für Linux und Windows.");
                ui.label("Lüfter, Temperaturfühler und gemeinsame RGB-Effekte steuern.");
                ui.separator();
                ui.label("Protokollgrundlagen: Pavel Herrmann und OpenRGB.");
                ui.label("Lizenz: GPL-2.0-or-later.");
                ui.label("github.com/cbka/rust-loop-connect");
                if ui
                    .add_enabled(
                        profile_path.is_some(),
                        egui::Button::new("Lizenzhinweise speichern"),
                    )
                    .clicked()
                    && let Some(parent) = profile_path.as_ref().and_then(|p| p.parent())
                {
                    let path = parent.join("LICENSE-NOTICES.txt");
                    let notices = concat!(
                        include_str!("../../LICENSE"),
                        "\n\n",
                        include_str!("../licenses/THIRD-PARTY-NOTICES.txt")
                    );
                    match std::fs::create_dir_all(parent)
                        .and_then(|()| std::fs::write(&path, notices))
                    {
                        Ok(()) => {
                            self.notice = format!("Lizenzhinweise gespeichert: {}", path.display())
                        }
                        Err(e) => {
                            self.error = Some(format!(
                                "Lizenzhinweise konnten nicht gespeichert werden: {e}"
                            ))
                        }
                    }
                }
            });
    }

    fn overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Kühlkreislauf");
        let freshness = self.updated.map(|t| t.elapsed().as_secs());
        ui.label(
            RichText::new(match freshness {
                Some(age) if age <= 6 => format!(
                    "Messwerte vor {age} s aktualisiert · keine automatische Kurve beim Start"
                ),
                Some(age) => {
                    format!("Messwerte veraltet ({age} s). Verbindung und Kühlung prüfen.")
                }
                None => "Noch keine Messwerte. Verbinden, um den Controller auszulesen.".into(),
            })
            .color(if freshness.is_some_and(|s| s > 6) {
                AMBER
            } else {
                MUTED
            }),
        );
        ui.add_space(12.0);
        ui.columns(3, |cols| {
            for (i, col) in cols.iter_mut().enumerate() {
                card(col, |ui| {
                    ui.label(
                        RichText::new(format!("T{} / TEMPERATUR", i + 1))
                            .small()
                            .color(MUTED),
                    );
                    let value = self.snapshot.as_ref().and_then(|s| s.temperatures_c[i]);
                    ui.label(
                        RichText::new(temperature_text(value))
                            .size(29.0)
                            .color(if value.is_some() { CYAN } else { AMBER }),
                    );
                    ui.label(
                        RichText::new(if value.is_some() {
                            "Externer Temperaturfühler"
                        } else if self.snapshot.is_some() {
                            "Fühler nicht verfügbar"
                        } else {
                            "Warte auf Messwerte"
                        })
                        .small()
                        .color(MUTED),
                    );
                });
            }
        });
        ui.add_space(16.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Lüfter & Pumpe");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(
                        if self.curves_active { CYAN } else { MUTED },
                        if self.curves_active {
                            "● Kurven aktiv"
                        } else {
                            "Kurven nicht gestartet"
                        },
                    );
                });
            });
            ui.add_space(10.0);
            egui::Grid::new("live-fans")
                .num_columns(4)
                .spacing([26.0, 13.0])
                .min_col_width(120.0)
                .show(ui, |ui| {
                    for heading in ["KANAL", "DREHZAHL", "PWM-ZIEL", "ANZEIGE"] {
                        ui.label(RichText::new(heading).small().color(MUTED));
                    }
                    ui.end_row();
                    for i in 0..6 {
                        ui.label(fan_label(i));
                        let fan = self.snapshot.as_ref().map(|s| s.fans[i]);
                        ui.label(
                            RichText::new(
                                fan.map(|f| format!("{} RPM", f.rpm))
                                    .unwrap_or_else(|| "— RPM".into()),
                            )
                            .strong(),
                        );
                        ui.label(
                            fan.map(|f| format!("{} %", f.pwm_percent))
                                .unwrap_or_else(|| "— %".into()),
                        );
                        ui.add(
                            egui::ProgressBar::new(
                                fan.map(|f| f.pwm_percent as f32 / 100.0).unwrap_or(0.0),
                            )
                            .desired_width(170.0)
                            .fill(if fan.is_some() {
                                CYAN
                            } else {
                                MUTED
                            }),
                        );
                        ui.end_row();
                    }
                });
        });
        ui.add_space(14.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "FLO · {}",
                    self.snapshot
                        .as_ref()
                        .map(|s| format!("{} roh", s.flow_raw))
                        .unwrap_or_else(|| "—".into())
                ))
                .color(MUTED),
            );
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "LVL · {}",
                    self.snapshot
                        .as_ref()
                        .map(|s| format!("{} roh", s.level_raw))
                        .unwrap_or_else(|| "—".into())
                ))
                .color(MUTED),
            );
            ui.label(
                RichText::new("Sensorbelegung und Kalibrierung unbestätigt")
                    .small()
                    .color(AMBER),
            );
        });
    }

    fn fans(&mut self, ui: &mut egui::Ui) {
        ui.heading("Lüfterregelung");
        ui.label(
            RichText::new(
                "Änderungen sind Entwürfe. Erst Übernehmen oder Kurven starten steuert das Gerät.",
            )
            .color(MUTED),
        );
        ui.horizontal_wrapped(|ui| {
            for i in 0..6 {
                if ui
                    .selectable_label(self.selected_fan == i, fan_label(i))
                    .clicked()
                {
                    self.selected_fan = i;
                }
            }
        });
        ui.add_space(8.0);
        let index = self.selected_fan;
        let mut apply = false;
        card(ui, |ui| {
            let fan = &mut self.profile.fans[index];
            ui.horizontal(|ui| {
                let label = ui.label("Name");
                ui.text_edit_singleline(&mut fan.name).labelled_by(label.id);
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut fan.mode, ControlMode::Manual, "Manuell");
                ui.radio_value(&mut fan.mode, ControlMode::Curve, "Temperaturkurve");
            });
            match fan.mode {
                ControlMode::Manual => {
                    ui.add(
                        egui::Slider::new(&mut fan.manual_percent, 0..=100).text("PWM-Ziel (%)"),
                    );
                    apply = ui
                        .add_enabled(
                            self.connected.is_some() && !self.curves_active,
                            egui::Button::new(format!("{} übernehmen", fan_label(index))),
                        )
                        .clicked();
                    if self.curves_active {
                        ui.colored_label(
                            AMBER,
                            "Vor manuellen Änderungen die laufenden Kurven stoppen.",
                        );
                    }
                }
                ControlMode::Curve => {
                    egui::ComboBox::from_label("Temperaturquelle")
                        .selected_text(format!("T{}", fan.sensor + 1))
                        .show_ui(ui, |ui| {
                            for sensor in 0..3 {
                                ui.selectable_value(
                                    &mut fan.sensor,
                                    sensor,
                                    format!("T{}", sensor + 1),
                                );
                            }
                        });
                    ui.label(
                        RichText::new("Fehlender oder ungültiger Fühler: 100 % für aktive Kurven.")
                            .color(AMBER)
                            .small(),
                    );
                    curve_chart(ui, &fan.points);
                    let mut remove = None;
                    egui::Grid::new("curve-points")
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.label("Temperatur (°C)");
                            ui.label("PWM (%)");
                            ui.end_row();
                            let count = fan.points.len();
                            for (i, point) in fan.points.iter_mut().enumerate() {
                                ui.add(
                                    egui::DragValue::new(&mut point.temperature)
                                        .range(0.0..=120.0)
                                        .speed(0.5)
                                        .suffix(" °C"),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut point.percent)
                                        .range(0..=100)
                                        .suffix(" %"),
                                );
                                if ui
                                    .add_enabled(
                                        count > 2,
                                        egui::Button::new(format!("Punkt {} entfernen", i + 1)),
                                    )
                                    .clicked()
                                {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(i) = remove {
                        fan.points.remove(i);
                    }
                    if ui
                        .add_enabled(
                            fan.points.len() < 16
                                && fan.points.last().is_none_or(|p| p.temperature < 120.0),
                            egui::Button::new("Kurvenpunkt hinzufügen"),
                        )
                        .clicked()
                    {
                        let temperature = fan
                            .points
                            .last()
                            .map(|p| (p.temperature + 5.0).min(120.0))
                            .unwrap_or(25.0);
                        fan.points.push(CurvePoint {
                            temperature,
                            percent: 100,
                        });
                    }
                }
            }
        });
        if apply {
            self.send(Command::SetPwm {
                channel: index + 1,
                percent: self.profile.fans[index].manual_percent,
            });
        }
        ui.add_space(12.0);
        let count = self
            .profile
            .fans
            .iter()
            .filter(|f| f.mode == ControlMode::Curve)
            .count();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.connected.is_some() && count > 0 && !self.curves_active,
                    egui::Button::new(format!("Kurven starten ({count} Kanäle)")),
                )
                .clicked()
            {
                match self.profile.validate() {
                    Ok(()) => self.send(Command::StartCurves(Box::new(self.profile.clone()))),
                    Err(e) => self.error = Some(format!("Kurve ungültig: {e:#}")),
                }
            }
            if ui
                .add_enabled(
                    self.curves_active,
                    egui::Button::new("Kurven stoppen (100 %)"),
                )
                .clicked()
            {
                self.send(Command::StopCurves);
            }
        });
        ui.label(RichText::new("Stoppen setzt nur aktiv geregelte Kanäle auf 100 %, sofern die Verbindung funktioniert. Kein bestätigter Hardware-Watchdog.").small().color(AMBER));
    }

    fn lighting(&mut self, ui: &mut egui::Ui) {
        ui.heading("Beleuchtung");
        ui.label(
            RichText::new(
                "Ein globaler Effekt für den Controller. Keine belegte Einzelsteuerung von L1–L6.",
            )
            .color(MUTED),
        );
        let mut apply = false;
        card(ui, |ui| {
            let rgb = &mut self.profile.rgb;
            egui::ComboBox::from_label("Effekt")
                .selected_text(rgb.effect.label())
                .show_ui(ui, |ui| {
                    for effect in RgbEffect::ALL {
                        ui.selectable_value(&mut rgb.effect, effect, effect.label());
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Farbe");
                ui.color_edit_button_srgb(&mut rgb.color);
            });
            ui.add(egui::Slider::new(&mut rgb.speed, 0..=100).text("Geschwindigkeit"));
            ui.add(egui::Slider::new(&mut rgb.brightness, 0..=100).text("Helligkeit"));
            apply = ui
                .add_enabled(
                    self.connected.is_some(),
                    egui::Button::new("Beleuchtung übernehmen"),
                )
                .clicked();
        });
        if apply {
            self.send(Command::SetRgb(self.profile.rgb.clone()));
        }
    }

    fn profiles(&mut self, ui: &mut egui::Ui) {
        ui.heading("Lokales Profil");
        ui.label(RichText::new("Speichern und Laden ändern nur den Entwurf. Sie starten keine Regelung und senden keine Gerätebefehle.").color(MUTED));
        card(ui, |ui| {
            ui.label("Profildatei");
            ui.label(
                RichText::new(
                    self.profile_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "Nicht verfügbar".into()),
                )
                .monospace()
                .small(),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.profile_path.is_some(), egui::Button::new("Entwurf speichern")).clicked()
                    && let Some(path) = &self.profile_path { match config::save(path, &self.profile) {
                        Ok(()) => { self.notice = "Profil gespeichert. Gerätewerte unverändert.".into(); self.error = None; }
                        Err(e) => self.error = Some(format!("Speichern fehlgeschlagen: {e:#}")),
                    }
                }
                if ui.add_enabled(self.profile_path.as_ref().is_some_and(|p| p.exists()), egui::Button::new("Profil laden")).clicked()
                    && let Some(path) = &self.profile_path { match config::load(path) {
                        Ok(profile) => { self.profile = profile; self.notice = "Profil als Entwurf geladen. Laufende Kurven bleiben bis zum Stoppen unverändert.".into(); self.error = None; }
                        Err(e) => self.error = Some(format!("Laden fehlgeschlagen: {e:#}")),
                    }
                }
            });
            ui.add_space(8.0);
            for (i, fan) in self.profile.fans.iter().enumerate() {
                ui.label(format!(
                    "{} · {} · {}",
                    fan_label(i),
                    fan.name,
                    match fan.mode {
                        ControlMode::Manual => format!("Manuell {} %", fan.manual_percent),
                        ControlMode::Curve => format!("Kurve über T{}", fan.sensor + 1),
                    }
                ));
            }
        });
    }

    fn capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.capture_path.clone() else {
            return;
        };
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let result = image::save_buffer(
                &path,
                &rgba,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .map_err(|e| format!("Screenshot konnte nicht gespeichert werden: {e}"));
            if let Err(e) = &result {
                self.error = Some(e.clone());
                eprintln!("{e}");
            }
            if let Ok(mut output) = self.capture_result.lock() {
                *output = Some(result);
            }
            self.capture_path = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self
            .capture_requested
            .is_some_and(|t| t.elapsed() > Duration::from_secs(15))
        {
            let error = "Screenshot-Aufnahme nach 15 Sekunden ohne Renderer-Antwort abgebrochen"
                .to_string();
            eprintln!("{error}");
            self.error = Some(error.clone());
            if let Ok(mut output) = self.capture_result.lock() {
                *output = Some(Err(error));
            }
            self.capture_path = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.opened.elapsed() > Duration::from_secs(20)
            && self.capture_requested.is_none()
        {
            let error =
                "Screenshot-Aufnahme: keine stabilen Demo-Messwerte nach 20 Sekunden".to_string();
            eprintln!("{error}");
            self.error = Some(error.clone());
            if let Ok(mut output) = self.capture_result.lock() {
                *output = Some(Err(error));
            }
            self.capture_path = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.frames >= 8
            && (self.snapshot.is_some() || self.preview_state.is_some())
            && self.capture_requested.is_none()
        {
            self.capture_requested = Some(Instant::now());
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive();
        if let Some(state) = &self.preview_state {
            self.connected = None;
            self.snapshot = None;
            self.updated = None;
            match state.as_str() {
                "empty" => {
                    self.devices.clear();
                    self.notice =
                        "Kein Controller gefunden. USB-Verbindung prüfen und erneut suchen.".into();
                }
                "error" => {
                    self.error = Some("Verbindung zum Controller unterbrochen. USB neu verbinden und erneut verbinden.".into());
                }
                "permission" => {
                    self.error = Some("Zugriff auf den Controller verweigert. USB-Geräteregel prüfen und Gerät neu verbinden.".into());
                }
                "loading" => {
                    self.devices.clear();
                    self.notice = "USB-Geräte werden gesucht …".into();
                }
                _ => {}
            }
        }
        self.draw(ctx);
        self.frames += 1;
        self.capture(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.service.shutdown();
    }
}

fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            body(ui);
        });
}

fn fan_label(index: usize) -> String {
    if index == 5 {
        "F6 · Lüfter/Pumpe".into()
    } else {
        format!("F{}", index + 1)
    }
}

fn temperature_text(value: Option<f32>) -> String {
    value
        .filter(|v| v.is_finite())
        .map(|v| format!("{v:.1} °C"))
        .unwrap_or_else(|| "— °C".into())
}

fn curve_chart(ui: &mut egui::Ui, points: &[CurvePoint]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(680.0), 150.0),
        egui::Sense::hover(),
    );
    let plot = rect.shrink2(egui::vec2(36.0, 22.0));
    let painter = ui.painter_at(rect);
    for percent in [0.0, 50.0, 100.0] {
        let y = plot.bottom() - plot.height() * percent / 100.0;
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, Color32::from_rgb(53, 65, 75)),
        );
        painter.text(
            egui::pos2(rect.left(), y),
            egui::Align2::LEFT_CENTER,
            format!("{percent:.0}%"),
            egui::FontId::proportional(11.0),
            MUTED,
        );
    }
    for temperature in [0.0, 40.0, 80.0, 120.0] {
        let x = plot.left() + plot.width() * temperature / 120.0;
        painter.text(
            egui::pos2(x, plot.bottom() + 9.0),
            egui::Align2::CENTER_TOP,
            format!("{temperature:.0}°C"),
            egui::FontId::proportional(11.0),
            MUTED,
        );
    }
    let positions: Vec<_> = points
        .iter()
        .filter(|p| p.temperature.is_finite())
        .map(|p| {
            egui::pos2(
                plot.left() + plot.width() * p.temperature / 120.0,
                plot.bottom() - plot.height() * p.percent as f32 / 100.0,
            )
        })
        .collect();
    for segment in positions.windows(2) {
        painter.line_segment([segment[0], segment[1]], egui::Stroke::new(2.0_f32, CYAN));
    }
    for position in positions {
        painter.circle_filled(position, 3.5, CYAN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_preview_cannot_synthesize_values_in_real_mode() {
        assert!(
            run(GuiOptions {
                demo: false,
                capture_path: None,
                capture_view: Some("error".into()),
                size: [960.0, 720.0],
            })
            .is_err()
        );
    }

    #[test]
    fn unavailable_temperature_is_not_a_synthesized_reading() {
        assert_eq!(temperature_text(None), "— °C");
        assert_eq!(temperature_text(Some(f32::NAN)), "— °C");
        assert_eq!(temperature_text(Some(31.5)), "31.5 °C");
    }

    #[test]
    fn every_page_renders_without_device_data_and_keyboard_focus_is_reachable() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut app = Desktop::new(
            GuiOptions {
                demo: true,
                capture_path: None,
                capture_view: None,
                size: [960.0, 720.0],
            },
            Arc::new(Mutex::new(None)),
        );
        for size in [[960.0, 720.0], [1280.0, 900.0]] {
            for (tab, _) in Tab::ALL {
                app.tab = tab;
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0], size[1]),
                    )),
                    ..Default::default()
                };
                let output = ctx.run(input, |ctx| app.draw(ctx));
                assert!(!output.shapes.is_empty());
            }
        }
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: Some(egui::Key::Tab),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        assert!(ctx.memory(|memory| memory.focused().is_some()));
    }
}
