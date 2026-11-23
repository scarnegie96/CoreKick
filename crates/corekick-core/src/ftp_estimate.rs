//! Automatic FTP estimation from everyday riding ("eFTP").
//!
//! Uses the 2-parameter **critical power** model on your mean-maximal power
//! over the last 90 days: work done in a maximal effort of duration t is
//! `W(t) = CP·t + W′`, so a straight-line fit of work vs time through your
//! best 3–20-minute efforts gives CP (≈ FTP) as the slope and W′ (anaerobic
//! work capacity) as the intercept. Bests are pooled across rides, so a hard
//! 5-minute effort on Tuesday and a 20-minute climb on Saturday both count.
//!
//! Only valid rides are used: ≥ 20 min moving, real power data, not ridden on
//! the simulator. If the fit is implausible we fall back to 95 % of the best
//! 20-minute power.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::model::RideSummary;
use crate::stats::ride_date;

pub const WINDOW_DAYS: i64 = 90;
/// Durations (s) used for the CP fit.
const CP_DURATIONS: [u32; 5] = [180, 300, 480, 600, 1200];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EffortPoint {
    pub duration_s: u32,
    pub watts: f64,
    pub ride_id: String,
    pub ride_name: String,
    pub date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FtpEstimate {
    pub estimate: f64,
    /// "critical_power" | "best_20min"
    pub method: String,
    pub cp: Option<f64>,
    pub w_prime_kj: Option<f64>,
    pub r2: Option<f64>,
    pub points: Vec<EffortPoint>,
    /// "high" | "medium" | "low"
    pub confidence: String,
    pub rides_considered: u32,
    /// Did any ride in the window look properly hard (IF ≥ 0.85)?
    pub hard_ride_in_window: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Suggestion {
    Increase,
    Decrease,
    Hold,
    NotEnoughData,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FtpAssessment {
    /// YYYY-MM-DD the assessment was made
    pub date: String,
    pub current_ftp: f64,
    pub estimate: Option<FtpEstimate>,
    pub suggested_ftp: Option<f64>,
    pub change_w: f64,
    pub change_pct: f64,
    pub suggestion: Suggestion,
    pub reason: String,
    pub trigger_ride_id: Option<String>,
    /// Applied automatically (auto-increase mode)
    pub auto_applied: bool,
    /// The rider dismissed this suggestion
    pub dismissed: bool,
}

/// Rides that may contribute to an FTP estimate.
pub fn is_valid_ride(r: &RideSummary) -> bool {
    !r.simulated && r.moving_s >= 1200.0 && r.avg_power > 20.0 && r.max_power < 2500.0 && !r.power_bests.is_empty()
}

/// Least-squares fit of work = CP·t + W′. Returns (CP, W′ joules, r²).
pub fn fit_cp(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if points.len() < 3 {
        return None;
    }
    let n = points.len() as f64;
    let xs: Vec<f64> = points.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.0 * p.1).collect(); // work = t × P
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    if sxx <= 0.0 {
        return None;
    }
    let cp = sxy / sxx;
    let wp = my - cp * mx;
    let ss_tot: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
    let ss_res: f64 = xs.iter().zip(&ys).map(|(x, y)| (y - (cp * x + wp)).powi(2)).sum();
    let r2 = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 0.0 };
    Some((cp, wp, r2))
}

pub fn estimate(rides: &[RideSummary], today: NaiveDate) -> Option<FtpEstimate> {
    let recent: Vec<(&RideSummary, NaiveDate)> = rides
        .iter()
        .filter(|r| is_valid_ride(r))
        .filter_map(|r| ride_date(r).map(|d| (r, d)))
        .filter(|(_, d)| (today - *d).num_days() <= WINDOW_DAYS && *d <= today)
        .collect();
    if recent.is_empty() {
        return None;
    }
    let hard = recent.iter().any(|(r, _)| r.intensity_factor >= 0.85);
    // Pool mean-maximal power per duration across rides.
    let best = |dur: u32| -> Option<EffortPoint> {
        recent
            .iter()
            .filter_map(|(r, d)| r.power_bests.iter().find(|(x, _)| *x == dur).map(|(_, w)| (r, d, *w)))
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(r, d, w)| EffortPoint { duration_s: dur, watts: w, ride_id: r.id.clone(), ride_name: r.name.clone(), date: d.to_string() })
    };
    let points: Vec<EffortPoint> = CP_DURATIONS.iter().filter_map(|&d| best(d)).collect();
    let b20 = points.iter().find(|p| p.duration_s == 1200).map(|p| p.watts);

    let short = points.iter().any(|p| p.duration_s <= 300);
    let long = points.iter().any(|p| p.duration_s >= 600);
    if points.len() >= 3 && short && long {
        let xy: Vec<(f64, f64)> = points.iter().map(|p| (p.duration_s as f64, p.watts)).collect();
        if let Some((cp, wp, r2)) = fit_cp(&xy) {
            let max_p = points.iter().map(|p| p.watts).fold(0.0, f64::max);
            let plausible = (50.0..=600.0).contains(&cp) && (3_000.0..=60_000.0).contains(&wp) && cp < max_p && r2 > 0.95;
            if plausible {
                let confidence = if points.len() >= 4 && b20.is_some() && r2 > 0.99 { "high" } else { "medium" };
                return Some(FtpEstimate {
                    estimate: cp.round(),
                    method: "critical_power".into(),
                    cp: Some(cp),
                    w_prime_kj: Some(wp / 1000.0),
                    r2: Some(r2),
                    points,
                    confidence: confidence.into(),
                    rides_considered: recent.len() as u32,
                    hard_ride_in_window: hard,
                });
            }
        }
    }
    b20.map(|w| FtpEstimate {
        estimate: (w * 0.95).round(),
        method: "best_20min".into(),
        cp: None,
        w_prime_kj: None,
        r2: None,
        points,
        confidence: "low".into(),
        rides_considered: recent.len() as u32,
        hard_ride_in_window: hard,
    })
}

/// Compare an estimate with the current FTP and decide what to suggest.
///
/// * Increase when the estimate is ≥ max(5 W, 3 %) above FTP.
/// * Decrease only when it is ≥ max(8 W, 5 %) below, the fit is confident and
///   you've actually ridden hard recently – easy weeks don't lower your FTP.
pub fn assess(current_ftp: f64, est: Option<FtpEstimate>, date: NaiveDate, trigger: Option<String>) -> FtpAssessment {
    let mut a = FtpAssessment {
        date: date.to_string(),
        current_ftp,
        estimate: est.clone(),
        suggested_ftp: None,
        change_w: 0.0,
        change_pct: 0.0,
        suggestion: Suggestion::NotEnoughData,
        reason: "Not enough hard efforts in the last 90 days – ride at least 20 minutes with some sustained efforts, or take a test.".into(),
        trigger_ride_id: trigger,
        auto_applied: false,
        dismissed: false,
    };
    let Some(e) = est else { return a };
    let diff = e.estimate - current_ftp;
    a.change_w = diff;
    a.change_pct = if current_ftp > 0.0 { diff / current_ftp * 100.0 } else { 0.0 };
    let how = if e.method == "critical_power" {
        format!("critical-power fit of your best {} efforts (3–20 min) from the last 90 days", e.points.len())
    } else {
        "95 % of your best 20-minute power in the last 90 days".to_string()
    };
    if diff >= (current_ftp * 0.03).max(5.0) {
        a.suggestion = Suggestion::Increase;
        a.suggested_ftp = Some(e.estimate);
        a.reason = format!("Your riding shows more than your current FTP – {how}.");
    } else if -diff >= (current_ftp * 0.05).max(8.0) && e.confidence == "high" && e.hard_ride_in_window {
        a.suggestion = Suggestion::Decrease;
        a.suggested_ftp = Some(e.estimate);
        a.reason = format!("Your recent maximal efforts are below your FTP – {how}. Consider a test to confirm.");
    } else {
        a.suggestion = Suggestion::Hold;
        a.reason = format!("Your FTP still matches your riding ({how}).");
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ride(id: &str, date: &str, cp: f64, wp: f64, iff: f64) -> RideSummary {
        let bests = [1u32, 5, 60, 180, 300, 480, 600, 1200]
            .iter()
            .map(|&d| (d, if d >= 180 { cp + wp / d as f64 } else { cp * 2.0 }))
            .collect();
        RideSummary {
            id: id.into(),
            name: id.into(),
            started_at: format!("{date}T18:00:00+00:00"),
            moving_s: 3600.0,
            avg_power: cp * 0.8,
            max_power: cp * 3.0,
            intensity_factor: iff,
            power_bests: bests,
            ..Default::default()
        }
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn recovers_cp_from_mean_max() {
        let e = estimate(&[ride("a", "2026-11-20", 250.0, 20_000.0, 0.9)], d("2026-11-23")).unwrap();
        assert_eq!(e.method, "critical_power");
        assert_eq!(e.estimate, 250.0);
        assert!((e.w_prime_kj.unwrap() - 20.0).abs() < 0.1);
        assert_eq!(e.confidence, "high");
    }

    #[test]
    fn pools_bests_across_rides_and_window() {
        // Short efforts from one ride, long ones from another; an old ride is ignored.
        let mut a = ride("short", "2026-11-10", 270.0, 20_000.0, 0.9);
        a.power_bests.retain(|(d, _)| *d <= 300);
        let mut b = ride("long", "2026-11-18", 270.0, 20_000.0, 0.9);
        b.power_bests.retain(|(d, _)| *d >= 600);
        let old = ride("old", "2026-06-01", 400.0, 20_000.0, 0.9);
        let e = estimate(&[a, b, old], d("2026-11-23")).unwrap();
        assert_eq!(e.estimate, 270.0);
        assert_eq!(e.points.iter().map(|p| p.ride_id.as_str()).collect::<Vec<_>>(), vec!["short", "short", "long", "long"]);
    }

    #[test]
    fn excludes_simulator_short_and_powerless_rides() {
        let mut sim = ride("sim", "2026-11-20", 300.0, 20_000.0, 0.9);
        sim.simulated = true;
        let mut short = ride("short", "2026-11-20", 300.0, 20_000.0, 0.9);
        short.moving_s = 600.0;
        let mut nopower = ride("np", "2026-11-20", 300.0, 20_000.0, 0.9);
        nopower.avg_power = 0.0;
        assert!(estimate(&[sim, short, nopower], d("2026-11-23")).is_none());
    }

    #[test]
    fn falls_back_to_best_20() {
        let mut r = ride("a", "2026-11-20", 250.0, 20_000.0, 0.9);
        r.power_bests.retain(|(d, _)| *d == 1200 || *d < 180);
        let e = estimate(&[r], d("2026-11-23")).unwrap();
        assert_eq!(e.method, "best_20min");
        assert_eq!(e.estimate, ((250.0 + 20_000.0 / 1200.0) * 0.95_f64).round());
        assert_eq!(e.confidence, "low");
    }

    #[test]
    fn suggestions() {
        let today = d("2026-11-23");
        let est = estimate(&[ride("a", "2026-11-20", 260.0, 20_000.0, 0.9)], today);
        // increase: 260 vs 240
        let a = assess(240.0, est.clone(), today, None);
        assert_eq!(a.suggestion, Suggestion::Increase);
        assert_eq!(a.suggested_ftp, Some(260.0));
        // hold: within 3 %
        assert_eq!(assess(255.0, est.clone(), today, None).suggestion, Suggestion::Hold);
        // decrease: confident + hard ride in window
        assert_eq!(assess(290.0, est.clone(), today, None).suggestion, Suggestion::Decrease);
        // …but not if the window only has easy rides
        let easy = estimate(&[ride("e", "2026-11-20", 200.0, 20_000.0, 0.65)], today);
        assert_eq!(assess(260.0, easy, today, None).suggestion, Suggestion::Hold);
        assert_eq!(assess(250.0, None, today, None).suggestion, Suggestion::NotEnoughData);
    }

    #[test]
    fn rejects_implausible_fits() {
        // Flat mean-max (no anaerobic capacity) → W′≈0 → fallback.
        let mut r = ride("flat", "2026-11-20", 250.0, 0.0, 0.9);
        r.power_bests = vec![(180, 250.0), (300, 250.0), (600, 250.0), (1200, 250.0)];
        assert_eq!(estimate(&[r], d("2026-11-23")).unwrap().method, "best_20min");
    }
}
