// SPDX-License-Identifier: GPL-2.0-or-later
//! Pure curve evaluation; this module never touches a controller.

use crate::config::{CurvePoint, validate_points};
use anyhow::Result;

pub fn target(points: &[CurvePoint], temperature: Option<f32>) -> Result<u8> {
    validate_points(points)?;
    let Some(temperature) = temperature.filter(|t| t.is_finite() && (0.0..=120.0).contains(t))
    else {
        return Ok(100);
    };
    if temperature <= points[0].temperature {
        return Ok(points[0].percent);
    }
    for pair in points.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        if temperature <= right.temperature {
            let fraction = (f64::from(temperature) - f64::from(left.temperature))
                / (f64::from(right.temperature) - f64::from(left.temperature));
            let interpolated = f64::from(left.percent)
                + fraction * (f64::from(right.percent) - f64::from(left.percent));
            return Ok(interpolated.ceil().clamp(0.0, 100.0) as u8);
        }
    }
    Ok(points[points.len() - 1].percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points() -> [CurvePoint; 3] {
        [
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
        ]
    }

    #[test]
    fn clamps_and_interpolates_including_exact_points() {
        let curve = points();
        for (temperature, percent) in [
            (0.0, 30),
            (25.0, 30),
            (30.0, 45),
            (35.0, 60),
            (40.0, 80),
            (45.0, 100),
            (120.0, 100),
        ] {
            assert_eq!(target(&curve, Some(temperature)).unwrap(), percent);
        }
        assert_eq!(target(&curve, Some(25.1)).unwrap(), 31, "round up duty");
    }

    #[test]
    fn missing_and_invalid_temperatures_fail_safe() {
        for temperature in [
            None,
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(f32::NEG_INFINITY),
            Some(-1.0),
            Some(120.1),
        ] {
            assert_eq!(target(&points(), temperature).unwrap(), 100);
        }
    }

    #[test]
    fn invalid_curves_are_rejected_before_missing_temperature_failsafe() {
        assert!(target(&[], None).is_err());
        assert!(target(&points()[..1], None).is_err());
        let mut curve = points();
        curve[1].temperature = curve[0].temperature;
        assert!(target(&curve, None).is_err());
        curve[1].temperature = f32::NAN;
        assert!(target(&curve, Some(25.0)).is_err());
        curve = points();
        curve[1].percent = 101;
        assert!(target(&curve, None).is_err());
    }

    #[test]
    fn decreasing_duty_is_supported_without_underflow() {
        let curve = [
            CurvePoint {
                temperature: 20.0,
                percent: 80,
            },
            CurvePoint {
                temperature: 40.0,
                percent: 30,
            },
        ];
        assert_eq!(target(&curve, Some(30.0)).unwrap(), 55);
        assert_eq!(target(&curve, Some(30.1)).unwrap(), 55);
    }
}
