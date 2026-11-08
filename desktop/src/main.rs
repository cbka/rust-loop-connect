// SPDX-License-Identifier: GPL-2.0-or-later
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use anyhow::{Context, Result, bail};
use rust_loop_connect::{
    device,
    gui::{self, GuiOptions},
    service,
};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("Rust Loop Connect: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut demo = false;
    let mut list = false;
    let mut status = false;
    let mut index = None;
    let mut capture_path = None;
    let mut capture_view = None;
    let mut size = [1280.0, 900.0];
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--list" => list = true,
            "--status" => status = true,
            "--device" => {
                index = Some(
                    args.next()
                        .context("--device benötigt eine Nummer")?
                        .parse::<usize>()?,
                )
            }
            "--capture" => {
                capture_path = Some(PathBuf::from(
                    args.next().context("--capture benötigt einen Dateipfad")?,
                ))
            }
            "--capture-view" => {
                capture_view = Some(
                    args.next()
                        .context("--capture-view benötigt eine Ansicht")?,
                )
            }
            "--width" => {
                size[0] = args
                    .next()
                    .context("--width benötigt eine Zahl")?
                    .parse::<f32>()?
            }
            "--height" => {
                size[1] = args
                    .next()
                    .context("--height benötigt eine Zahl")?
                    .parse::<f32>()?
            }
            "--version" | "-V" => {
                println!("Rust Loop Connect {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "Rust Loop Connect\n\nOhne Argumente: Desktop-Anwendung\n--list: Geräte anzeigen\n--status [--device INDEX]: echte Messwerte lesen\n--demo: Oberfläche ohne Hardware ausprobieren\n--demo --capture DATEI.png [--width 960 --height 720]: GUI-Prüfaufnahme\n"
                );
                return Ok(());
            }
            other => bail!("Unbekanntes Argument: {other}"),
        }
    }
    if size
        .iter()
        .any(|v| !v.is_finite() || *v < 480.0 || *v > 4000.0)
    {
        bail!("Fenstergröße muss zwischen 480 und 4000 liegen");
    }
    if capture_path.is_some() && !demo {
        bail!("--capture ist nur zusammen mit --demo erlaubt");
    }
    if let Some(view) = capture_view.as_deref() {
        if !demo || capture_path.is_none() {
            bail!("--capture-view benötigt --demo und --capture");
        }
        if ![
            "fans",
            "curve",
            "lighting",
            "profiles",
            "empty",
            "error",
            "permission",
            "loading",
        ]
        .contains(&view)
        {
            bail!("Unbekannte Prüfungsansicht");
        }
    }
    if list && status {
        bail!("--list und --status getrennt verwenden");
    }
    if demo && (list || status) {
        if list {
            println!("0: Rust Loop Connect · Demo");
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&service::demo_snapshot())?
            );
        }
        return Ok(());
    }
    if list || status {
        let devices = device::discover()?;
        if list {
            for (i, item) in devices.iter().enumerate() {
                println!("{i}: {}", item.name);
            }
            if devices.is_empty() {
                println!("Kein unterstützter USB-Controller gefunden.");
            }
            return Ok(());
        }
        let selected = match index {
            Some(index) => index,
            None if devices.len() == 1 => 0,
            _ => bail!("Bitte mit --list suchen und bei mehreren Geräten --device INDEX wählen"),
        };
        let info = devices
            .get(selected)
            .context("Geräteindex nicht vorhanden")?;
        let mut controller = device::connect(info)?;
        println!("{}", serde_json::to_string_pretty(&controller.status()?)?);
        return Ok(());
    }
    gui::run(GuiOptions {
        demo,
        capture_path,
        capture_view,
        size,
    })
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
