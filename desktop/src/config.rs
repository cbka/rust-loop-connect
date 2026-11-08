// SPDX-License-Identifier: GPL-2.0-or-later
//! Local profiles are drafts; loading or saving never applies device settings.

use crate::protocol::RgbSettings;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_PROFILE_BYTES: usize = 64 * 1024;
pub const PROFILE_VERSION: u32 = 1;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ControlMode {
    Manual,
    Curve,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub temperature: f32,
    pub percent: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FanProfile {
    pub name: String,
    pub mode: ControlMode,
    pub manual_percent: u8,
    pub sensor: usize,
    pub points: Vec<CurvePoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub fans: [FanProfile; 6],
    pub rgb: RgbSettings,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            version: PROFILE_VERSION,
            fans: std::array::from_fn(|index| FanProfile {
                name: format!("F{}", index + 1),
                mode: ControlMode::Manual,
                manual_percent: 100,
                sensor: 0,
                points: vec![
                    CurvePoint {
                        temperature: 25.0,
                        percent: 30,
                    },
                    CurvePoint {
                        temperature: 35.0,
                        percent: 60,
                    },
                    CurvePoint {
                        temperature: 45.0,
                        percent: 100,
                    },
                ],
            }),
            rgb: RgbSettings::default(),
        }
    }
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == PROFILE_VERSION,
            "Unsupported profile version {}",
            self.version
        );
        for (index, fan) in self.fans.iter().enumerate() {
            ensure!(!fan.name.trim().is_empty(), "F{} needs a name", index + 1);
            ensure!(
                fan.name.chars().count() <= 64 && fan.name.len() <= 256,
                "F{} name is too long (maximum 64 characters)",
                index + 1
            );
            ensure!(
                !fan.name.chars().any(char::is_control),
                "F{} name contains a control character",
                index + 1
            );
            ensure!(
                fan.manual_percent <= 100,
                "F{} PWM must be 0..100%",
                index + 1
            );
            ensure!(fan.sensor < 3, "F{} sensor index must be 0..2", index + 1);
            validate_points(&fan.points)
                .with_context(|| format!("Invalid F{} curve", index + 1))?;
        }
        ensure!(self.rgb.speed <= 100, "RGB speed must be 0..100%");
        ensure!(self.rgb.brightness <= 100, "RGB brightness must be 0..100%");
        Ok(())
    }
}

pub(crate) fn validate_points(points: &[CurvePoint]) -> Result<()> {
    ensure!(
        (2..=16).contains(&points.len()),
        "A curve needs 2..16 points"
    );
    for (index, point) in points.iter().enumerate() {
        ensure!(
            point.temperature.is_finite() && (0.0..=120.0).contains(&point.temperature),
            "Curve temperature must be finite and within 0..120 degrees Celsius"
        );
        ensure!(point.percent <= 100, "Curve PWM must be 0..100%");
        if index > 0 {
            ensure!(
                point.temperature > points[index - 1].temperature,
                "Curve temperatures must be strictly increasing"
            );
        }
    }
    Ok(())
}

/// Resolve the native per-user configuration directory without creating it.
pub fn profile_path() -> Result<PathBuf> {
    let directories =
        directories::ProjectDirs::from("io.github.cbka", "RustLoopConnect", "Rust Loop Connect")
            .context("Cannot determine the current user's configuration directory")?;
    Ok(directories.config_dir().join("profile.json"))
}

/// Parse one bounded regular file. Invalid profiles are never replaced by defaults.
pub fn load(path: &Path) -> Result<Profile> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Cannot inspect profile {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "Profile must be a regular file, not a link or directory"
    );
    ensure!(
        metadata.len() <= MAX_PROFILE_BYTES as u64,
        "Profile exceeds 64 KiB"
    );
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("Cannot open profile {}", path.display()))?
        .take((MAX_PROFILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .context("Cannot read profile")?;
    ensure!(bytes.len() <= MAX_PROFILE_BYTES, "Profile exceeds 64 KiB");
    let profile: Profile =
        serde_json::from_slice(&bytes).context("Profile is not valid JSON data")?;
    profile.validate().context("Profile validation failed")?;
    Ok(profile)
}

static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TemporaryProfile(PathBuf);

impl Drop for TemporaryProfile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Write beside the destination, flush and close it, then replace with one rename.
/// `std::fs::rename` replaces existing files on Unix and Windows. No remove-first
/// fallback is used: if replacement fails, the previous profile remains intact.
pub fn save(path: &Path, profile: &Profile) -> Result<()> {
    profile
        .validate()
        .context("Refusing to save an invalid profile")?;
    let mut bytes = serde_json::to_vec_pretty(profile).context("Cannot encode profile")?;
    bytes.push(b'\n');
    ensure!(
        bytes.len() <= MAX_PROFILE_BYTES,
        "Encoded profile exceeds 64 KiB"
    );
    let filename = path.file_name().context("Profile path must name a file")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("Cannot create {}", parent.display()))?;
    let mut created = None;
    for _ in 0..32 {
        let mut name = std::ffi::OsString::from(".");
        name.push(filename);
        name.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => {
                created = Some((TemporaryProfile(temporary), file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("Cannot create temporary profile"),
        }
    }
    let Some((temporary, mut file)) = created else {
        bail!("Cannot reserve a unique temporary profile file");
    };
    file.write_all(&bytes)
        .context("Cannot write temporary profile")?;
    file.sync_all().context("Cannot flush temporary profile")?;
    drop(file); // Close the temporary file before Windows replacement.
    fs::rename(&temporary.0, path)
        .context("Cannot atomically replace profile; previous file retained")?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .context("Profile was saved, but its directory could not be flushed")?;
    Ok(())
}

/// Read-only location for profiles from the earlier local development build.
pub fn legacy_profile_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("io.github.cbka", "EKLoopConnect", "EK Loop Connect")
        .map(|dirs| dirs.config_dir().join("profile.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let unique = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ek-loop-profile-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create isolated test directory");
            Self(path)
        }

        fn profile(&self) -> PathBuf {
            self.0.join("profile.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_are_valid_manual_drafts() {
        let profile = Profile::default();
        profile.validate().unwrap();
        assert_eq!(profile.version, 1);
        for fan in &profile.fans {
            assert_eq!(fan.mode, ControlMode::Manual);
            assert_eq!(fan.points.len(), 3);
            assert_eq!(fan.points[0].temperature, 25.0);
            assert_eq!(fan.points[0].percent, 30);
            assert_eq!(fan.points[2].percent, 100);
        }
    }

    #[test]
    fn rejects_invalid_version_channels_and_percentages() {
        let mut p = Profile {
            version: 2,
            ..Profile::default()
        };
        assert!(p.validate().is_err());
        p.version = 1;
        p.fans[0].sensor = 3;
        assert!(p.validate().is_err());
        p.fans[0].sensor = 0;
        p.fans[0].manual_percent = 101;
        assert!(p.validate().is_err());
        p.fans[0].manual_percent = 100;
        p.rgb.speed = 101;
        assert!(p.validate().is_err());
        p.rgb.speed = 100;
        p.rgb.brightness = 101;
        assert!(p.validate().is_err());
    }

    #[test]
    fn validates_curves_even_for_manual_profiles() {
        let mut p = Profile::default();
        for temperature in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 120.1] {
            p.fans[0].points[0].temperature = temperature;
            assert!(p.validate().is_err(), "temperature={temperature}");
        }
        p.fans[0].points[0].temperature = 35.0;
        assert!(p.validate().is_err(), "equal temperatures are ambiguous");
        p.fans[0].points[0].temperature = 40.0;
        assert!(p.validate().is_err(), "descending temperatures");
        p = Profile::default();
        p.fans[0].points[0].percent = 101;
        assert!(p.validate().is_err());
        p.fans[0].points = vec![CurvePoint {
            temperature: 25.0,
            percent: 30,
        }];
        assert!(p.validate().is_err());
        p.fans[0].points = (0..17)
            .map(|n| CurvePoint {
                temperature: n as f32,
                percent: 50,
            })
            .collect();
        assert!(p.validate().is_err());
    }

    #[test]
    fn rejects_empty_control_character_and_oversized_names() {
        let mut p = Profile::default();
        for name in [
            "".to_string(),
            "  ".to_string(),
            "F1\0pump".to_string(),
            "F1\nPump".to_string(),
            "x".repeat(65),
        ] {
            p.fans[0].name = name;
            assert!(p.validate().is_err());
        }
    }

    #[test]
    fn atomic_save_roundtrip_replaces_existing_and_leaves_no_temp_files() {
        let scratch = Scratch::new();
        let file = scratch.profile();
        let mut profile = Profile::default();
        profile.fans[1].name = "Gehäuselüfter".into();
        save(&file, &profile).unwrap();
        assert_eq!(load(&file).unwrap().fans[1].name, "Gehäuselüfter");
        profile.fans[1].mode = ControlMode::Curve;
        profile.fans[1].manual_percent = 43;
        save(&file, &profile).unwrap();
        let read = load(&file).unwrap();
        assert_eq!(read.fans[1].mode, ControlMode::Curve);
        assert_eq!(read.fans[1].manual_percent, 43);
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn invalid_save_does_not_replace_existing_profile() {
        let scratch = Scratch::new();
        let file = scratch.profile();
        let mut profile = Profile::default();
        save(&file, &profile).unwrap();
        let original = fs::read(&file).unwrap();
        profile.fans[0].points[0].temperature = f32::NAN;
        assert!(save(&file, &profile).is_err());
        assert_eq!(fs::read(file).unwrap(), original);
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
    }

    #[test]
    fn malformed_unknown_fields_and_oversized_profiles_are_errors_not_defaults() {
        let scratch = Scratch::new();
        let file = scratch.profile();
        for bytes in [
            b"{broken".as_slice(),
            b"{\"version\":1,\"fans\":[],\"rgb\":{}}",
            b"{\"version\":NaN}",
            b"null",
        ] {
            fs::write(&file, bytes).unwrap();
            assert!(load(&file).is_err());
            assert_eq!(fs::read(&file).unwrap(), bytes);
        }
        let mut json = serde_json::to_value(Profile::default()).unwrap();
        json["execute"] = "arbitrary-untrusted-command".into();
        fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(load(&file).is_err());
        fs::write(&file, vec![b' '; MAX_PROFILE_BYTES + 1]).unwrap();
        assert!(load(&file).is_err());
        assert!(load(&scratch.0).is_err(), "directories are not profiles");
    }

    #[test]
    fn failed_rename_preserves_existing_target_and_cleans_temp_file() {
        let scratch = Scratch::new();
        let file = scratch.profile();
        fs::create_dir(&file).unwrap();
        assert!(save(&file, &Profile::default()).is_err());
        assert!(file.is_dir());
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
    }

    #[test]
    fn malicious_nested_fields_and_huge_numbers_are_rejected() {
        let scratch = Scratch::new();
        let file = scratch.profile();
        for replacement in [
            serde_json::json!(-1),
            serde_json::json!(1e100),
            serde_json::json!("NaN"),
        ] {
            let mut json = serde_json::to_value(Profile::default()).unwrap();
            json["fans"][0]["sensor"] = replacement;
            fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
            assert!(load(&file).is_err());
        }
        let mut json = serde_json::to_value(Profile::default()).unwrap();
        json["fans"][0]["points"][0]["command"] = "do not execute".into();
        fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(load(&file).is_err());
    }
    #[test]
    fn native_profile_location_names_a_json_file() {
        let path = profile_path().unwrap();
        assert!(path.is_absolute());
        assert_eq!(path.file_name().unwrap(), "profile.json");
    }

    #[test]
    fn temperature_and_point_count_boundaries_are_inclusive() {
        let mut p = Profile::default();
        p.fans[0].points = vec![
            CurvePoint {
                temperature: 0.0,
                percent: 0,
            },
            CurvePoint {
                temperature: 120.0,
                percent: 100,
            },
        ];
        p.validate().unwrap();
        p.fans[0].points = (0..16)
            .map(|n| CurvePoint {
                temperature: n as f32 * 8.0,
                percent: 100,
            })
            .collect();
        p.validate().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn load_rejects_links_without_reading_external_target() {
        use std::os::unix::fs::symlink;
        let scratch = Scratch::new();
        let external = scratch.0.join("external.json");
        save(&external, &Profile::default()).unwrap();
        let original = fs::read(&external).unwrap();
        symlink(&external, scratch.profile()).unwrap();
        assert!(load(&scratch.profile()).is_err());
        assert_eq!(fs::read(external).unwrap(), original);
    }
}
