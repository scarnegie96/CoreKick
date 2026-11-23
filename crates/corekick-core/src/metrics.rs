//! Training metrics: NP, IF, TSS, power-duration bests, zones, energy and calories.

use crate::model::{Profile, Sex};

/// Standard durations for power-duration bests (seconds).
pub const BEST_DURATIONS: [u32; 14] = [1, 5, 15, 30, 60, 120, 180, 300, 480, 600, 1200, 1800, 3600, 5400];

/// Coggan power zone upper bounds as a fraction of FTP (Z1..Z6; Z7 is everything above).
pub const ZONE_BOUNDS: [f64; 6] = [0.55, 0.75, 0.90, 1.05, 1.20, 1.50];
pub const ZONE_NAMES: [&str; 7] =
    ["Active Recovery", "Endurance", "Tempo", "Threshold", "VO2 Max", "Anaerobic", "Neuromuscular"];

pub fn mean(x: &[f64]) -> f64 {
    if x.is_empty() {
        0.0
    } else {
        x.iter().sum::<f64>() / x.len() as f64
    }
}

/// Normalized Power from 1 Hz power: 30 s rolling mean → 4th power → mean → 4th root.
pub fn normalized_power(power: &[f64]) -> f64 {
    const W: usize = 30;
    if power.is_empty() {
        return 0.0;
    }
    if power.len() < W {
        return mean(power);
    }
    let mut sum: f64 = power[..W].iter().sum();
    let mut acc = (sum / W as f64).powi(4);
    let mut n = 1usize;
    for i in W..power.len() {
        sum += power[i] - power[i - W];
        acc += (sum / W as f64).max(0.0).powi(4);
        n += 1;
    }
    (acc / n as f64).powf(0.25)
}

pub fn intensity_factor(np: f64, ftp: f64) -> f64 {
    if ftp > 0.0 {
        np / ftp
    } else {
        0.0
    }
}

/// Training Stress Score = (s · NP · IF) / (FTP · 3600) · 100
pub fn tss(seconds: f64, np: f64, ftp: f64) -> f64 {
    if ftp <= 0.0 {
        return 0.0;
    }
    let iff = np / ftp;
    seconds * np * iff / (ftp * 3600.0) * 100.0
}

/// Best average power for a window of `secs` samples (1 Hz). None if ride is shorter.
pub fn best_average(power: &[f64], secs: usize) -> Option<f64> {
    if secs == 0 || power.len() < secs {
        return None;
    }
    let mut sum: f64 = power[..secs].iter().sum();
    let mut best = sum;
    for i in secs..power.len() {
        sum += power[i] - power[i - secs];
        if sum > best {
            best = sum;
        }
    }
    Some(best / secs as f64)
}

pub fn power_bests(power: &[f64]) -> Vec<(u32, f64)> {
    BEST_DURATIONS
        .iter()
        .filter_map(|&d| best_average(power, d as usize).map(|w| (d, (w * 10.0).round() / 10.0)))
        .collect()
}

pub fn zone_of(power: f64, ftp: f64) -> usize {
    if ftp <= 0.0 {
        return 0;
    }
    let f = power / ftp;
    ZONE_BOUNDS.iter().position(|&b| f <= b).unwrap_or(6)
}

/// Seconds spent in each of the 7 zones (1 Hz samples).
pub fn time_in_zones(power: &[f64], ftp: f64) -> [f64; 7] {
    let mut z = [0.0; 7];
    for &p in power {
        z[zone_of(p, ftp)] += 1.0;
    }
    z
}

/// Mechanical work → metabolic energy. kcal = kJ / (4.184 · gross efficiency).
/// With the usual ~24 % efficiency this gives the well-known "1 kJ ≈ 1 kcal".
pub fn calories_from_work(kj: f64, gross_efficiency: f64) -> f64 {
    let ge = gross_efficiency.clamp(0.15, 0.30);
    kj / (4.184 * ge)
}

/// Keytel et al. (2005) heart-rate based energy expenditure, kcal for `minutes`.
pub fn calories_from_hr(avg_hr: f64, minutes: f64, profile: &Profile) -> f64 {
    let w = profile.weight_kg;
    let a = profile.age as f64;
    let per_min = match profile.sex {
        Sex::Male => (-55.0969 + 0.6309 * avg_hr + 0.1988 * w + 0.2017 * a) / 4.184,
        Sex::Female => (-20.4022 + 0.4472 * avg_hr - 0.1263 * w + 0.074 * a) / 4.184,
    };
    (per_min * minutes).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn np_of_constant_is_constant() {
        let p = vec![250.0; 600];
        assert!((normalized_power(&p) - 250.0).abs() < 1e-9);
    }

    #[test]
    fn np_exceeds_average_for_variable_power() {
        let p: Vec<f64> = (0..1200).map(|i| if (i / 60) % 2 == 0 { 350.0 } else { 100.0 }).collect();
        let np = normalized_power(&p);
        assert!(np > mean(&p) + 20.0, "np {np}");
    }

    #[test]
    fn one_hour_at_ftp_is_100_tss() {
        assert!((tss(3600.0, 250.0, 250.0) - 100.0).abs() < 1e-9);
        assert!((tss(1800.0, 250.0, 250.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn best_average_works() {
        let mut p = vec![100.0; 600];
        for x in p.iter_mut().skip(100).take(60) {
            *x = 400.0;
        }
        assert_eq!(best_average(&p, 60), Some(400.0));
        assert_eq!(best_average(&p, 601), None);
        let b = power_bests(&p);
        assert_eq!(b.iter().find(|(d, _)| *d == 5).unwrap().1, 400.0);
    }

    #[test]
    fn zones() {
        assert_eq!(zone_of(100.0, 250.0), 0);
        assert_eq!(zone_of(250.0, 250.0), 3);
        assert_eq!(zone_of(500.0, 250.0), 6);
        let z = time_in_zones(&[100.0, 250.0, 250.0], 250.0);
        assert_eq!(z[0], 1.0);
        assert_eq!(z[3], 2.0);
    }

    #[test]
    fn calories() {
        let kcal = calories_from_work(1000.0, 0.24);
        assert!((kcal - 995.9).abs() < 1.0);
        let p = Profile::default();
        let k = calories_from_hr(150.0, 60.0, &p);
        assert!(k > 600.0 && k < 1100.0, "{k}");
    }
}
