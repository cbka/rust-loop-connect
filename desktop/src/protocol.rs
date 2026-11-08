// SPDX-License-Identifier: GPL-2.0-or-later
// Fan/sensor protocol: Pavel Herrmann, https://github.com/pavelherr/ek-loop-connect
// RGB protocol: Chris M (Dr_No), OpenRGB EKController (GPL-2.0-or-later), and
// NiceAesth's ek-loop-py. This implementation uses public protocol documentation;
// no proprietary vendor implementation or UI assets are included.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const FAN_CHANNELS: [[u8; 2]; 6] = [
    [0xa0, 0xa0],
    [0xa0, 0xc0],
    [0xa0, 0xe0],
    [0xa1, 0x00],
    [0xa1, 0x20],
    [0xa1, 0xe0],
];
const REPORT_LEN: usize = 63;
const READ_SIGNATURE: [u8; 8] = [0x10, 0x12, 0x27, 0xaa, 1, 3, 0, 0x20];
const ACK_SIGNATURE: [u8; 11] = [0x10, 0x12, 6, 0, 0, 0, 0, 0xaa, 0xbb, 0x65, 0xed];

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct FanStatus {
    pub rpm: u16,
    pub pwm_percent: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub temperatures_c: [Option<f32>; 3],
    pub fans: [FanStatus; 6],
    pub flow_raw: u16,
    pub level_raw: u8,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RgbEffect {
    Static,
    Breathing,
    Fading,
    Marquee,
    CoveringMarquee,
    Pulse,
    Wave,
    Alternating,
    Candle,
}
impl RgbEffect {
    pub const ALL: [Self; 9] = [
        Self::Static,
        Self::Breathing,
        Self::Fading,
        Self::Marquee,
        Self::CoveringMarquee,
        Self::Pulse,
        Self::Wave,
        Self::Alternating,
        Self::Candle,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Self::Static => "Statisch",
            Self::Breathing => "Atmen",
            Self::Fading => "Farbwechsel",
            Self::Marquee => "Lauflicht",
            Self::CoveringMarquee => "Füllendes Lauflicht",
            Self::Pulse => "Pulsieren",
            Self::Wave => "Welle",
            Self::Alternating => "Abwechselnd",
            Self::Candle => "Kerzenlicht",
        }
    }
    fn wire_mode(self) -> u8 {
        match self {
            Self::Static => 1,
            Self::Breathing => 2,
            Self::Fading => 3,
            Self::Marquee => 4,
            Self::CoveringMarquee => 5,
            Self::Pulse => 6,
            Self::Wave => 7,
            Self::Alternating => 8,
            Self::Candle => 9,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RgbSettings {
    pub effect: RgbEffect,
    pub speed: u8,
    pub brightness: u8,
    pub color: [u8; 3],
}
impl Default for RgbSettings {
    fn default() -> Self {
        Self {
            effect: RgbEffect::Static,
            speed: 50,
            brightness: 100,
            color: [0, 180, 220],
        }
    }
}
fn channel_address(channel: usize) -> Result<[u8; 2]> {
    ensure!(
        (1..=6).contains(&channel),
        "Lüfterkanal muss zwischen 1 und 6 liegen."
    );
    Ok(FAN_CHANNELS[channel - 1])
}
pub fn read_request(channel: Option<usize>) -> Result<[u8; REPORT_LEN]> {
    let address = match channel {
        Some(c) => channel_address(c)?,
        None => [0xa2, 0x20],
    };
    let mut b = [0; REPORT_LEN];
    b[..6].copy_from_slice(&[0x10, 0x12, 8, 0xaa, 1, 3]);
    b[6..8].copy_from_slice(&address);
    b[8..14].copy_from_slice(&[0, 0x20, 0x66, 0xff, 0xff, 0xed]);
    if channel.is_none() {
        b[11..13].copy_from_slice(&[0x60, 0xfe]);
    }
    Ok(b)
}
pub fn pwm_request(channel: usize, percent: u8) -> Result<[u8; REPORT_LEN]> {
    let address = channel_address(channel)?;
    ensure!(
        percent <= 100,
        "PWM muss zwischen 0 und 100 Prozent liegen."
    );
    let mut b = [0; REPORT_LEN];
    b[..6].copy_from_slice(&[0x10, 0x12, 0x29, 0xaa, 1, 0x10]);
    b[6..8].copy_from_slice(&address);
    b[8..11].copy_from_slice(&[0, 0x10, 0x20]);
    b[24] = percent;
    b[25] = 0xff;
    b[45] = 0xff;
    b[46] = 0xed;
    Ok(b)
}
pub fn rgb_request(settings: &RgbSettings) -> Result<[u8; REPORT_LEN]> {
    ensure!(
        settings.speed <= 100 && settings.brightness <= 100,
        "RGB-Tempo und Helligkeit müssen zwischen 0 und 100 liegen."
    );
    let mut b = [0; REPORT_LEN];
    b[..12].copy_from_slice(&[
        0x10, 0x12, 0x29, 0xaa, 1, 0x10, 0xa2, 0x60, 0, 0x10, 0x20, 1,
    ]);
    b[12] = settings.effect.wire_mode();
    b[13] = if settings.effect == RgbEffect::Fading {
        0xff
    } else {
        0
    };
    b[14] = if settings.effect == RgbEffect::Static {
        0
    } else {
        settings.speed
    };
    b[15] = settings.brightness;
    b[16..19].copy_from_slice(&settings.color);
    b[47] = 0xff;
    b[48] = 0;
    Ok(b)
}
fn validate_status(b: &[u8]) -> Result<()> {
    ensure!(
        b.len() == REPORT_LEN,
        "Unvollständiger oder zu langer USB-Report."
    );
    ensure!(
        b[..8] == READ_SIGNATURE && b[40..42] == [0xaa, 0xbb] && b[43] == 0xed,
        "Unerwartete Statusantwort des Controllers."
    );
    Ok(())
}
pub fn parse_sensors(b: &[u8]) -> Result<([Option<f32>; 3], u16, u8)> {
    validate_status(b)?;
    ensure!(
        [8, 12, 16, 24].iter().all(|&o| b[o..o + 3] == [0, 1, 0]),
        "Antwort ist kein Sensorreport."
    );
    let temperatures = [11, 15, 19].map(|o| {
        if b[o] == 0xe7 {
            None
        } else {
            Some(f32::from(b[o]))
        }
    });
    Ok((temperatures, u16::from_be_bytes([b[22], b[23]]), b[27]))
}
pub fn parse_fan(b: &[u8]) -> Result<FanStatus> {
    validate_status(b)?;
    // Fan metadata (model/rated speed/power/load) can be nonzero. Only the
    // sensor marker combination identifies the other known response kind.
    ensure!(
        ![8, 12, 16, 24].iter().all(|&o| b[o..o + 3] == [0, 1, 0]),
        "Antwort ist kein Lüfterreport."
    );
    ensure!(b[21] <= 100, "Controller meldet einen ungültigen PWM-Wert.");
    Ok(FanStatus {
        rpm: u16::from_be_bytes([b[12], b[13]]),
        pwm_percent: b[21],
    })
}
pub fn validate_ack(b: &[u8]) -> Result<()> {
    ensure!(
        b.len() == REPORT_LEN && b.starts_with(&ACK_SIGNATURE),
        "Controller hat die Änderung nicht bestätigt."
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn fan_metadata_may_contain_rated_speed_power_and_load() {
        let mut b = fan_fixture(45);
        b[8..12].copy_from_slice(&[0, 3, 0x09, 0xc4]);
        b[14..21].copy_from_slice(&[0, 20, 0, 5, 0, 70, 0]);
        assert_eq!(parse_fan(&b).unwrap().pwm_percent, 45);
        assert!(parse_fan(&sensor_fixture()).is_err());
    }

    pub(crate) fn sensor_fixture() -> Vec<u8> {
        let mut b = vec![0; 63];
        b[..8].copy_from_slice(&[0x10, 0x12, 0x27, 0xaa, 1, 3, 0, 0x20]);
        for i in [8, 12, 16, 24] {
            b[i..i + 3].copy_from_slice(&[0, 1, 0]);
        }
        for i in [11, 15, 19] {
            b[i] = 0xe7;
        }
        b[40..44].copy_from_slice(&[0xaa, 0xbb, 0xec, 0xed]);
        b
    }
    pub(crate) fn fan_fixture(percent: u8) -> Vec<u8> {
        let mut b = vec![0; 63];
        b[..8].copy_from_slice(&[0x10, 0x12, 0x27, 0xaa, 1, 3, 0, 0x20]);
        b[9] = 3;
        b[12..14].copy_from_slice(&1234_u16.to_be_bytes());
        b[21] = percent;
        b[40..44].copy_from_slice(&[0xaa, 0xbb, 0x4e, 0xed]);
        b
    }
    pub(crate) fn ack_fixture() -> Vec<u8> {
        let mut b = vec![0; 63];
        b[..11].copy_from_slice(&[0x10, 0x12, 6, 0, 0, 0, 0, 0xaa, 0xbb, 0x65, 0xed]);
        b
    }
    #[test]
    fn f6_preserves_dedicated_pump_mapping() {
        assert_eq!(&read_request(Some(6)).unwrap()[6..8], &[0xa1, 0xe0]);
        assert_eq!(&pwm_request(6, 50).unwrap()[6..8], &[0xa1, 0xe0]);
    }
    #[test]
    fn frames_and_input_bounds() {
        assert_eq!(
            &read_request(None).unwrap()[..14],
            &[
                0x10, 0x12, 8, 0xaa, 1, 3, 0xa2, 0x20, 0, 0x20, 0x66, 0x60, 0xfe, 0xed
            ]
        );
        let b = pwm_request(1, 55).unwrap();
        assert_eq!(b[24], 55);
        assert_eq!(b[25], 255);
        assert_eq!(&b[45..47], &[255, 0xed]);
        for channel in [0, 7, usize::MAX] {
            assert!(pwm_request(channel, 50).is_err());
            assert!(read_request(Some(channel)).is_err());
        }
        assert!(pwm_request(1, 101).is_err());
    }
    #[test]
    fn missing_and_present_sensor_values_and_raw_flow() {
        let mut b = sensor_fixture();
        assert_eq!(parse_sensors(&b).unwrap().0, [None; 3]);
        b[11] = 20;
        b[15] = 30;
        b[19] = 40;
        b[22..24].copy_from_slice(&321_u16.to_be_bytes());
        b[27] = 100;
        assert_eq!(
            parse_sensors(&b).unwrap(),
            ([Some(20.), Some(30.), Some(40.)], 321, 100)
        );
    }
    #[test]
    fn fan_fixture_accepts_nonzero_model_field() {
        let s = parse_fan(&fan_fixture(24)).unwrap();
        assert_eq!(s.rpm, 1234);
        assert_eq!(s.pwm_percent, 24);
    }
    #[test]
    fn replies_reject_lengths_signatures_and_types() {
        let good = fan_fixture(24);
        for len in [0, 11, 62, 64, 65] {
            assert!(parse_fan(&vec![0; len]).is_err());
        }
        for offset in [0, 2, 3, 40, 41, 43] {
            let mut bad = good.clone();
            bad[offset] ^= 1;
            assert!(parse_fan(&bad).is_err());
        }
        assert!(parse_fan(&sensor_fixture()).is_err());
        assert!(parse_sensors(&good).is_err());
        assert!(parse_fan(&fan_fixture(101)).is_err());
        assert!(validate_ack(&ack_fixture()).is_ok());
        assert!(validate_ack(&good).is_err());
    }
    #[test]
    fn rgb_modes_global_address_and_fields() {
        for (i, effect) in RgbEffect::ALL.iter().enumerate() {
            let s = RgbSettings {
                effect: *effect,
                speed: 75,
                brightness: 42,
                color: [10, 20, 30],
            };
            let b = rgb_request(&s).unwrap();
            assert_eq!(&b[6..8], &[0xa2, 0x60]);
            assert_eq!(b[12], i as u8 + 1);
            assert_eq!(b[14], if *effect == RgbEffect::Static { 0 } else { 75 });
            assert_eq!(b[15], 42);
            assert_eq!(&b[16..19], &[10, 20, 30]);
            assert_eq!(b[47], 255);
            assert_eq!(b[48], 0);
        }
        let mut s = RgbSettings {
            brightness: 101,
            ..RgbSettings::default()
        };
        assert!(rgb_request(&s).is_err());
        s.brightness = 100;
        s.speed = 101;
        assert!(rgb_request(&s).is_err());
    }
}
