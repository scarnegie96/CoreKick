//! Virtual shifting for single-speed trainers (e.g. KICKR CORE with a Zwift Cog).
//!
//! With one physical cog the rider can't change gear, so in SIM-type riding we
//! compute the speed the rider *would* be doing in the selected virtual gear
//! (cadence × ratio × wheel circumference) and set the trainer's resistance in
//! ERG mode to exactly the power that speed needs on the current gradient.
//! Shifting up raises the target at the same cadence – just like a real gear.

use crate::physics::{power_for_speed, PhysicsParams};

/// 24 gear ratios (chainring ÷ sprocket) spanning a compact 34×34 bail-out to a
/// 52×11 sprint gear, spaced so each shift is a similar ~8 % step.
pub const VIRTUAL_GEARS: [f64; 24] = [
    0.75, 0.87, 0.99, 1.11, 1.23, 1.38, 1.53, 1.68, 1.86, 2.04, 2.22, 2.40, 2.61, 2.82, 3.03, 3.24, 3.49, 3.74,
    3.99, 4.24, 4.54, 4.84, 5.14, 5.49,
];

pub fn gear_count() -> usize {
    VIRTUAL_GEARS.len()
}

/// Gear index (0-based) clamped into range from a 1-based gear number.
pub fn index_from_gear(gear: u32) -> usize {
    (gear.max(1) as usize - 1).min(VIRTUAL_GEARS.len() - 1)
}

/// Road speed (m/s) for a cadence in a gear.
pub fn speed_for_cadence(cadence_rpm: f64, gear_idx: usize, wheel_circumference_m: f64) -> f64 {
    cadence_rpm.max(0.0) / 60.0 * VIRTUAL_GEARS[gear_idx.min(VIRTUAL_GEARS.len() - 1)] * wheel_circumference_m
}

/// Resistance target (W) for the trainer: the power needed to ride at the
/// cadence-implied speed on `grade` (fraction). Never negative; capped at 2 kW.
pub fn target_power(cadence_rpm: f64, gear_idx: usize, wheel_circumference_m: f64, grade: f64, ele_m: f64, p: &PhysicsParams) -> f64 {
    if cadence_rpm < 15.0 {
        return 0.0;
    }
    let v = speed_for_cadence(cadence_rpm, gear_idx, wheel_circumference_m);
    power_for_speed(v, grade, ele_m, p).clamp(0.0, 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gears_increase_monotonically() {
        for w in VIRTUAL_GEARS.windows(2) {
            assert!(w[1] > w[0]);
        }
        assert_eq!(index_from_gear(0), 0);
        assert_eq!(index_from_gear(99), 23);
    }

    #[test]
    fn realistic_targets() {
        let p = PhysicsParams { mass_kg: 83.0, ..Default::default() };
        // 90 rpm in gear 14 (2.82) on the flat ≈ 32 km/h ≈ 170–200 W
        let w = target_power(90.0, index_from_gear(14), 2.105, 0.0, 0.0, &p);
        assert!((150.0..230.0).contains(&w), "{w}");
        // easy gear on an 8 % climb at 80 rpm ≈ 9 km/h ≈ 170–230 W
        let w = target_power(80.0, index_from_gear(3), 2.105, 0.08, 0.0, &p);
        assert!((150.0..260.0).contains(&w), "{w}");
        // shifting up at the same cadence costs more
        assert!(target_power(90.0, 15, 2.105, 0.0, 0.0, &p) > target_power(90.0, 14, 2.105, 0.0, 0.0, &p));
        // coasting / descending: no resistance
        assert_eq!(target_power(5.0, 10, 2.105, 0.0, 0.0, &p), 0.0);
        assert_eq!(target_power(90.0, 0, 2.105, -0.08, 0.0, &p), 0.0);
    }
}
