//! Rider + bike physics for virtual speed.
//!
//! Forces on the rider at speed v on gradient s (rise/run):
//!   gravity  = m·g·sin(θ)
//!   rolling  = Crr·m·g·cos(θ)
//!   aero     = ½·ρ·CdA·(v + wind)²
//! Propulsion = η·P / v. We integrate F = m_eff·a so the rider carries momentum
//! over crests and accelerates realistically, rather than snapping to a steady speed.

use serde::{Deserialize, Serialize};

use crate::model::Profile;

pub const G: f64 = 9.80665;
/// Equivalent mass of rotating wheels/drivetrain (≈ I/r² for two road wheels).
pub const ROTATIONAL_MASS_KG: f64 = 1.4;
/// Speed below which propulsive force is capped (avoids infinite force at 0 m/s).
const MIN_PROPULSION_SPEED: f64 = 1.5;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PhysicsParams {
    pub mass_kg: f64,
    pub crr: f64,
    pub cda: f64,
    pub drivetrain_eff: f64,
    pub wind_mps: f64,
}

impl Default for PhysicsParams {
    fn default() -> Self {
        PhysicsParams { mass_kg: 83.0, crr: 0.004, cda: 0.32, drivetrain_eff: 0.976, wind_mps: 0.0 }
    }
}

impl PhysicsParams {
    pub fn from_profile(p: &Profile) -> Self {
        PhysicsParams {
            mass_kg: p.weight_kg + p.bike_weight_kg,
            crr: p.crr,
            cda: p.cda,
            drivetrain_eff: 0.976,
            wind_mps: 0.0,
        }
    }

    /// FTMS "wind resistance coefficient" Cw = ½·ρ·CdA (kg/m) at sea level.
    pub fn cw(&self) -> f64 {
        0.5 * air_density(0.0) * self.cda
    }
}

/// ISA barometric air density at an elevation (kg/m³), 15 °C sea level.
pub fn air_density(ele_m: f64) -> f64 {
    let e = ele_m.clamp(-400.0, 9000.0);
    1.225 * (1.0 - 2.25577e-5 * e).powf(4.2559)
}

/// Total resistive force (N) at speed v on gradient `grade` (fraction).
pub fn resistive_force(v: f64, grade: f64, ele_m: f64, p: &PhysicsParams) -> f64 {
    let theta = grade.atan();
    let va = v + p.wind_mps;
    p.mass_kg * G * theta.sin()
        + p.crr * p.mass_kg * G * theta.cos()
        + 0.5 * air_density(ele_m) * p.cda * va * va.abs()
}

/// Rider power (W) needed to hold speed v steadily.
pub fn power_for_speed(v: f64, grade: f64, ele_m: f64, p: &PhysicsParams) -> f64 {
    resistive_force(v, grade, ele_m, p) * v / p.drivetrain_eff
}

/// Steady-state speed (m/s) for a constant power. Solved by bisection.
pub fn steady_speed(power: f64, grade: f64, ele_m: f64, p: &PhysicsParams) -> f64 {
    let f = |v: f64| power * p.drivetrain_eff - resistive_force(v, grade, ele_m, p) * v;
    // Coasting uphill / zero power on a climb: stopped.
    if f(0.05) <= 0.0 && resistive_force(0.0, grade, ele_m, p) >= 0.0 {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0_f64, 60.0_f64);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > 0.0 {
            lo = mid
        } else {
            hi = mid
        }
    }
    0.5 * (lo + hi)
}

/// Advance speed by `dt` seconds under constant power on a gradient.
/// Uses 50 ms sub-steps of semi-implicit Euler; never rolls backwards.
pub fn integrate_speed(v0: f64, power: f64, grade: f64, ele_m: f64, p: &PhysicsParams, dt: f64) -> f64 {
    if dt <= 0.0 {
        return v0;
    }
    let m_eff = p.mass_kg + ROTATIONAL_MASS_KG;
    let n = ((dt / 0.05).ceil() as usize).max(1);
    let h = dt / n as f64;
    let mut v = v0.max(0.0);
    for _ in 0..n {
        let prop = if power > 0.0 { power * p.drivetrain_eff / v.max(MIN_PROPULSION_SPEED) } else { 0.0 };
        let net = prop - resistive_force(v, grade, ele_m, p);
        v = (v + net / m_eff * h).max(0.0);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_250w_is_about_37kmh() {
        let p = PhysicsParams { mass_kg: 80.0, ..Default::default() };
        let v = steady_speed(250.0, 0.0, 0.0, &p) * 3.6;
        assert!((35.0..39.5).contains(&v), "{v}");
    }

    #[test]
    fn climbing_is_slow() {
        let p = PhysicsParams { mass_kg: 80.0, ..Default::default() };
        let v = steady_speed(250.0, 0.08, 0.0, &p) * 3.6;
        assert!((10.0..15.0).contains(&v), "{v}");
    }

    #[test]
    fn power_speed_roundtrip() {
        let p = PhysicsParams::default();
        for &g in &[-0.03, 0.0, 0.02, 0.07] {
            let v = steady_speed(220.0, g, 500.0, &p);
            let w = power_for_speed(v, g, 500.0, &p);
            assert!((w - 220.0).abs() < 0.5, "grade {g}: {w}");
        }
    }

    #[test]
    fn integration_converges_to_steady_speed() {
        let p = PhysicsParams::default();
        let mut v = 0.0;
        for _ in 0..600 {
            v = integrate_speed(v, 200.0, 0.01, 0.0, &p, 0.25);
        }
        let s = steady_speed(200.0, 0.01, 0.0, &p);
        assert!((v - s).abs() < 0.05, "{v} vs {s}");
    }

    #[test]
    fn coasting_uphill_stops_and_never_reverses() {
        let p = PhysicsParams::default();
        let mut v = 5.0;
        for _ in 0..200 {
            v = integrate_speed(v, 0.0, 0.06, 0.0, &p, 0.25);
            assert!(v >= 0.0);
        }
        assert_eq!(v, 0.0);
    }

    #[test]
    fn descending_without_power_rolls() {
        let p = PhysicsParams::default();
        let s = steady_speed(0.0, -0.06, 0.0, &p) * 3.6;
        assert!(s > 45.0 && s < 80.0, "{s}");
    }
}
