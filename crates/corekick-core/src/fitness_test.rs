//! FTP tests: protocols (as workouts) and result evaluation.
//!
//! * Ramp: +~6.7 % FTP every minute until failure. FTP ≈ 75 % of best 1-min power.
//! * 20-minute: classic Allen/Coggan. FTP ≈ 95 % of 20-min average.
//! * 2×8-minute: Carmichael. FTP ≈ 90 % of the average of both efforts.

use serde::{Deserialize, Serialize};

use crate::metrics;
use crate::workout::{Block, Step, StepTarget, Workout};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestKind {
    Ramp,
    Ftp20,
    Ftp8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TestInfo {
    pub kind: TestKind,
    pub name: String,
    pub summary: String,
    pub how: String,
    pub approx_minutes: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TestResult {
    pub kind: TestKind,
    pub ftp: f64,
    /// The measured value FTP is derived from (best 1-min, 20-min average…)
    pub measured_w: f64,
    pub factor: f64,
    pub detail: String,
    pub max_power: f64,
    pub valid: bool,
}

pub fn test_catalogue() -> Vec<TestInfo> {
    vec![
        TestInfo {
            kind: TestKind::Ramp,
            name: "Ramp Test".into(),
            summary: "Short and sharp. ERG mode raises the target every minute until you can't hold it.".into(),
            how: "5 min warm-up, then the target climbs ~6.7 % of your current FTP every minute. Keep cadence above 70 rpm; \
                  the test ends automatically when power or cadence collapses. FTP = 75 % of your best 1-minute power."
                .into(),
            approx_minutes: 20,
        },
        TestInfo {
            kind: TestKind::Ftp20,
            name: "20-Minute FTP Test".into(),
            summary: "The gold standard. Pace a 20-minute all-out effort in resistance mode.".into(),
            how: "Warm-up, a 5-minute blow-out effort, recovery, then 20 minutes as hard as you can sustain on a 1 % virtual \
                  gradient – you control power with your gears. FTP = 95 % of the 20-minute average."
                .into(),
            approx_minutes: 60,
        },
        TestInfo {
            kind: TestKind::Ftp8,
            name: "2 × 8-Minute Test".into(),
            summary: "Two shorter maximal efforts – easier to pace than 20 minutes.".into(),
            how: "Warm-up, then two 8-minute maximal efforts separated by 10 minutes of easy spinning. \
                  FTP = 90 % of the average of both efforts."
                .into(),
            approx_minutes: 45,
        },
    ]
}

fn round_to(x: f64, m: f64) -> f64 {
    (x / m).round() * m
}

/// Ramp parameters for a current FTP: (start watts, step watts).
pub fn ramp_params(ftp: f64) -> (f64, f64) {
    let ftp = ftp.max(80.0);
    (round_to(ftp * 0.45, 10.0).max(60.0), round_to(ftp / 15.0, 5.0).max(10.0))
}

pub fn protocol(kind: TestKind, ftp: f64) -> Workout {
    let ftp = ftp.max(80.0);
    let free = |d: u32, label: &str, effort: bool, grade: f64| Block::Free { duration_s: d, grade_pct: grade, label: Some(label.into()), effort };
    let steady = |d: u32, p: f64, label: &str| Block::Steady { duration_s: d, power: p, cadence: None, label: Some(label.into()) };
    let blocks = match kind {
        TestKind::Ramp => {
            let (start, step) = ramp_params(ftp);
            let mut b = vec![steady(180, start * 0.8 / ftp, "Warm-up"), steady(120, start / ftp, "Warm-up")];
            // Enough steps to reach 2.2× FTP – nobody survives beyond that.
            let n = (((ftp * 2.2 - start) / step).ceil() as usize).clamp(15, 60);
            for i in 0..n {
                let w = start + step * i as f64;
                b.push(Block::Steady { duration_s: 60, power: w / ftp, cadence: None, label: Some(format!("Ramp {}", i + 1)) });
            }
            b.push(free(300, "Cool-down", false, 0.0));
            b
        }
        TestKind::Ftp20 => vec![
            Block::Ramp { duration_s: 600, from: 0.45, to: 0.75, label: Some("Warm-up".into()) },
            Block::Intervals { repeats: 3, on_s: 60, on_power: 1.0, off_s: 60, off_power: 0.5, on_cadence: Some(100) },
            steady(180, 0.5, "Easy"),
            free(300, "Blow-out effort", false, 1.0),
            steady(600, 0.5, "Recover"),
            free(1200, "20-minute test", true, 1.0),
            free(600, "Cool-down", false, 0.0),
        ],
        TestKind::Ftp8 => vec![
            Block::Ramp { duration_s: 600, from: 0.45, to: 0.75, label: Some("Warm-up".into()) },
            Block::Intervals { repeats: 3, on_s: 60, on_power: 1.0, off_s: 60, off_power: 0.5, on_cadence: Some(100) },
            steady(240, 0.5, "Easy"),
            free(480, "Effort 1 of 2", true, 1.0),
            steady(600, 0.5, "Recover"),
            free(480, "Effort 2 of 2", true, 1.0),
            free(600, "Cool-down", false, 0.0),
        ],
    };
    let info = test_catalogue().into_iter().find(|t| t.kind == kind).unwrap();
    Workout {
        id: format!("test-{:?}", kind).to_lowercase(),
        name: info.name,
        description: info.how,
        category: "Test".into(),
        blocks,
        builtin: true,
        created_at: String::new(),
    }
}

/// Evaluate a completed test. `power` and `t` are the 1 Hz samples, `t` being
/// *workout* time in seconds (so effort windows line up with steps).
pub fn evaluate(kind: TestKind, steps: &[Step], t: &[f64], power: &[f64]) -> Option<TestResult> {
    if power.is_empty() {
        return None;
    }
    let max_power = power.iter().cloned().fold(0.0, f64::max);
    match kind {
        TestKind::Ramp => {
            let ramp_power: Vec<f64> = t
                .iter()
                .zip(power)
                .filter(|(tt, _)| steps.iter().any(|s| s.label.starts_with("Ramp") && **tt >= s.start_s as f64 && **tt < s.end_s() as f64))
                .map(|(_, p)| *p)
                .collect();
            let best = metrics::best_average(&ramp_power, 60)?;
            Some(TestResult {
                kind,
                ftp: (best * 0.75).round(),
                measured_w: best.round(),
                factor: 0.75,
                detail: format!("Best 1-minute power {:.0} W over {} ramp minutes", best, ramp_power.len() / 60),
                max_power,
                valid: ramp_power.len() >= 120,
            })
        }
        TestKind::Ftp20 | TestKind::Ftp8 => {
            let windows: Vec<&Step> = steps.iter().filter(|s| s.effort && matches!(s.target, StepTarget::Free { .. })).collect();
            if windows.is_empty() {
                return None;
            }
            let mut avgs = Vec::new();
            let mut complete = true;
            for s in &windows {
                let v: Vec<f64> = t
                    .iter()
                    .zip(power)
                    .filter(|(tt, _)| **tt >= s.start_s as f64 && **tt < s.end_s() as f64)
                    .map(|(_, p)| *p)
                    .collect();
                if (v.len() as f64) < s.duration_s as f64 * 0.9 {
                    complete = false;
                }
                if !v.is_empty() {
                    avgs.push(metrics::mean(&v));
                }
            }
            if avgs.is_empty() {
                return None;
            }
            let measured = metrics::mean(&avgs);
            let factor = if kind == TestKind::Ftp20 { 0.95 } else { 0.90 };
            let detail = if kind == TestKind::Ftp20 {
                format!("20-minute average {:.0} W", measured)
            } else {
                format!("8-minute averages {}", avgs.iter().map(|a| format!("{a:.0} W")).collect::<Vec<_>>().join(" / "))
            };
            Some(TestResult {
                kind,
                ftp: (measured * factor).round(),
                measured_w: measured.round(),
                factor,
                detail,
                max_power,
                valid: complete,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_protocol_shape() {
        let w = protocol(TestKind::Ramp, 250.0);
        let s = w.steps();
        assert_eq!(s.first().unwrap().label, "Warm-up");
        assert_eq!(s.iter().filter(|x| x.label.starts_with("Ramp")).count(), 30);
        assert_eq!(ramp_params(250.0), (110.0, 15.0));
    }

    #[test]
    fn ramp_evaluation() {
        let w = protocol(TestKind::Ramp, 250.0);
        let steps = w.steps();
        // Rider follows the ramp for 15 minutes then stops.
        let mut t = Vec::new();
        let mut p = Vec::new();
        for sec in 0..(300 + 15 * 60) {
            let (i, into) = crate::workout::step_at(&steps, sec as f64).unwrap();
            t.push(sec as f64);
            p.push(steps[i].fraction_at(into).unwrap() * 250.0);
        }
        let r = evaluate(TestKind::Ramp, &steps, &t, &p).unwrap();
        // last minute: 110 + 15*14 = 320 W → FTP 240
        assert_eq!(r.measured_w, 320.0);
        assert_eq!(r.ftp, 240.0);
        assert!(r.valid);
    }

    #[test]
    fn ftp20_evaluation() {
        let w = protocol(TestKind::Ftp20, 250.0);
        let steps = w.steps();
        let total = steps.last().unwrap().end_s();
        let effort = steps.iter().find(|s| s.effort && matches!(s.target, StepTarget::Free { .. })).unwrap().clone();
        let t: Vec<f64> = (0..total).map(|x| x as f64).collect();
        let p: Vec<f64> = t.iter().map(|&x| if x >= effort.start_s as f64 && x < effort.end_s() as f64 { 280.0 } else { 150.0 }).collect();
        let r = evaluate(TestKind::Ftp20, &steps, &t, &p).unwrap();
        assert_eq!(r.ftp, 266.0);
        assert!(r.valid);
    }

    #[test]
    fn ftp8_evaluation_incomplete() {
        let steps = protocol(TestKind::Ftp8, 250.0).steps();
        let first = steps.iter().find(|s| s.effort && matches!(s.target, StepTarget::Free { .. })).unwrap().clone();
        let t: Vec<f64> = (0..first.end_s()).map(|x| x as f64).collect();
        let p = vec![300.0; t.len()];
        let r = evaluate(TestKind::Ftp8, &steps, &t, &p).unwrap();
        assert_eq!(r.ftp, 270.0);
        assert!(!r.valid); // second effort missing
    }
}
