//! Conversions between UI-level settings (as stored in setting cards) and
//! device values. Each formula was measured against Options+ 2.8 driving an
//! MX Anywhere 3 (see docs/BEHAVIOR.md).

/// SmartShift sensitivity (0..=100, UI percent) to the 0x2110/0x2111
/// auto-disengage threshold. Measured: 0→50, 50→27, 70→18, 100→4.
pub fn smartshift_auto_disengage(sensitivity: u32) -> u8 {
    let s = sensitivity.min(100) as f64;
    (50.0 - 0.46 * s).round() as u8
}

/// Inverse of [`smartshift_auto_disengage`].
pub fn smartshift_sensitivity(auto_disengage: u8) -> u32 {
    (((50.0 - auto_disengage as f64) / 0.46).round().clamp(0.0, 100.0)) as u32
}

/// Pointer speed card value (-1.0..=1.0; UI shows `(v + 1) * 50` percent) to
/// a sensor DPI: geometric interpolation through (−1, min), (0, default),
/// (1, max), rounded to the sensor's step. Measured: 0%→200, 25%→450,
/// 50%→1000, 75%→2000, 100%→4000 for min 200 / default 1000 / max 4000.
pub fn pointer_speed_to_dpi(value: f64, min: u16, default: u16, max: u16, step: u16) -> u16 {
    let v = value.clamp(-1.0, 1.0);
    let (min, default, max) = (min as f64, default as f64, max as f64);
    let raw = if v < 0.0 { default * (min / default).powf(-v) } else { default * (max / default).powf(v) };
    let step = step.max(1) as f64;
    let snapped = min + ((raw - min) / step).round() * step;
    snapped.clamp(min, max) as u16
}

/// Vertical wheel event (1/120 detent units, positive = up) for a diverted wheel report of `delta`
/// (positive = up) in 1/`per_notch` notches. `natural` is the "Inverted" direction setting.
pub fn wheel_units(delta: i16, per_notch: u8, natural: bool) -> i32 {
    let units = 120 * delta as i32 / per_notch.max(1) as i32;
    if natural { -units } else { units }
}

/// Horizontal wheel event (positive = right) for a raw-wheel report while a horizontal-scroll button
/// is held: with the standard direction, wheel down scrolls right (measured against Options+).
pub fn hscroll_units(delta: i16, natural: bool) -> i32 {
    let units = 120 * delta as i32;
    if natural { units } else { -units }
}

/// 16-bit configuration cookie (FNV-1a folded) over `parts`. Avoids 0 (the device's reset value)
/// and 0x4825 (the value Options+ writes), so neither is mistaken for our configuration.
pub fn cookie<'a>(parts: impl IntoIterator<Item = &'a str>) -> u16 {
    let mut h: u32 = 0x811c_9dc5;
    for p in parts {
        for b in p.bytes().chain([0xff]) {
            h = (h ^ b as u32).wrapping_mul(0x0100_0193);
        }
    }
    match ((h >> 16) ^ h) as u16 {
        0 => 1,
        0x4825 => 0x4826,
        c => c,
    }
}

/// Inverse of [`pointer_speed_to_dpi`] (before step rounding).
pub fn dpi_to_pointer_speed(dpi: u16, min: u16, default: u16, max: u16) -> f64 {
    let (d, min, default, max) = (dpi as f64, min as f64, default as f64, max as f64);
    if d < default { -(d / default).ln() / (min / default).ln() } else { (d / default).ln() / (max / default).ln() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smartshift_matches_measurements() {
        for (s, a) in [(0, 50), (50, 27), (70, 18), (100, 4)] {
            assert_eq!(smartshift_auto_disengage(s), a, "sensitivity {s}");
            assert_eq!(smartshift_sensitivity(a), s, "auto-disengage {a}");
        }
    }

    #[test]
    fn pointer_speed_matches_measurements() {
        let points =
            [(0.0, 200), (5.0, 250), (12.0, 300), (15.0, 300), (25.0, 450), (50.0, 1000), (75.0, 2000), (100.0, 4000)];
        for (pct, dpi) in points {
            let v = pct / 50.0 - 1.0;
            assert_eq!(pointer_speed_to_dpi(v, 200, 1000, 4000, 50), dpi, "{pct}%");
        }
    }

    #[test]
    fn scroll_units_match_options_plus() {
        // Smooth scrolling, standard direction: -1 -> -120, -5 -> -600, -10 -> -1200.
        assert_eq!(wheel_units(-1, 1, false), -120);
        assert_eq!(wheel_units(-5, 1, false), -600);
        assert_eq!(wheel_units(-10, 1, false), -1200);
        assert_eq!(wheel_units(-1, 1, true), 120);
        assert_eq!(wheel_units(15, 15, false), 120);
        // Hold + wheel down (-1): standard -> +120 (right), inverted -> -120.
        assert_eq!(hscroll_units(-1, false), 120);
        assert_eq!(hscroll_units(-1, true), -120);
        assert_eq!(hscroll_units(1, false), -120);
    }

    #[test]
    fn cookie_depends_on_every_part() {
        let a = cookie(["x", "y"]);
        assert_eq!(a, cookie(["x", "y"]));
        assert_ne!(a, cookie(["xy"]));
        assert_ne!(a, cookie(["x", "z"]));
        assert_ne!(cookie([]), 0);
    }

    #[test]
    fn dpi_roundtrip() {
        for v in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let d = pointer_speed_to_dpi(v, 200, 1000, 4000, 1);
            assert!((dpi_to_pointer_speed(d, 200, 1000, 4000) - v).abs() < 0.01, "{v}");
        }
    }
}
