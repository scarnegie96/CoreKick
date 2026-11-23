//! Shared data model (serialised to the UI and to SQLite as JSON).

use serde::{Deserialize, Serialize};

use crate::fitness_test::{TestKind, TestResult};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    #[default]
    Male,
    Female,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub weight_kg: f64,
    pub bike_weight_kg: f64,
    pub height_cm: f64,
    pub age: u32,
    pub sex: Sex,
    pub ftp: f64,
    pub max_hr: u32,
    pub rest_hr: u32,
    pub units: Units,
    /// Gross mechanical efficiency used to turn work (kJ) into metabolic energy (kcal).
    pub gross_efficiency: f64,
    /// Aerodynamic drag area (m²). ~0.40 upright, 0.32 hoods, 0.28 drops, 0.24 TT.
    pub cda: f64,
    pub crr: f64,
    /// Fraction of the real gradient sent to the trainer (like Zwift's "trainer difficulty").
    pub trainer_difficulty: f64,
    pub max_trainer_grade: f64,
    pub min_trainer_grade: f64,
    pub weekly_goal_hours: f64,
    /// Seconds of ERG "smoothing" the UI shows; also used by the simulator.
    pub erg_ramp_s: f64,
    /// Single-speed setups (e.g. Zwift Cog): emulate gears in software.
    pub virtual_shifting: bool,
    /// Wheel circumference used to turn cadence × gear into speed (m).
    pub wheel_circumference_m: f64,
    /// Virtual gear (1-based) selected at the start of each ride.
    pub start_gear: u32,
    /// Heart-rate zone model and lactate-threshold HR (for the LTHR model).
    pub hr_zone_model: HrZoneModel,
    pub lthr: u32,
    /// Default audio alerts for leaving a heart-rate zone range.
    pub hr_alerts: HrAlertSettings,
    /// What to do when rides suggest a new FTP.
    pub ftp_auto: FtpAutoMode,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FtpAutoMode {
    Off,
    /// Show a suggestion; you decide.
    #[default]
    Suggest,
    /// Apply increases automatically; still only suggest decreases.
    AutoIncrease,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HrZoneModel {
    /// 5 zones as % of max HR (50/60/70/80/90 %)
    #[default]
    MaxHr,
    /// 5 zones as % of heart-rate reserve (Karvonen)
    Reserve,
    /// Joe Friel's 7 zones from lactate-threshold HR
    Lthr,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HrAlertSettings {
    pub enabled: bool,
    /// Allowed range, 1-based zone numbers (inclusive)
    pub low_zone: u32,
    pub high_zone: u32,
    pub alert_high: bool,
    pub alert_low: bool,
    /// No alerts during the first N seconds (warm-up)
    pub grace_s: f64,
    /// Repeat while still out of range every N seconds
    pub repeat_s: f64,
    pub voice: bool,
    pub volume: f64,
}

impl Default for HrAlertSettings {
    fn default() -> Self {
        HrAlertSettings { enabled: false, low_zone: 2, high_zone: 3, alert_high: true, alert_low: true, grace_s: 300.0, repeat_s: 30.0, voice: true, volume: 0.7 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HrZone {
    /// 1-based
    pub zone: u32,
    pub name: String,
    pub lo: f64,
    /// None = open-ended
    pub hi: Option<f64>,
}

impl Profile {
    /// Heart-rate zone boundaries (bpm) for the selected model.
    pub fn hr_zones(&self) -> Vec<HrZone> {
        let mk = |zone: u32, name: &str, lo: f64, hi: Option<f64>| HrZone { zone, name: name.into(), lo: lo.round(), hi: hi.map(|h| h.round()) };
        let max = self.max_hr.max(100) as f64;
        let rest = self.rest_hr.min(self.max_hr.saturating_sub(20)) as f64;
        match self.hr_zone_model {
            HrZoneModel::MaxHr | HrZoneModel::Reserve => {
                let at = |p: f64| if self.hr_zone_model == HrZoneModel::MaxHr { max * p } else { rest + (max - rest) * p };
                vec![
                    mk(1, "Recovery", at(0.5), Some(at(0.6))),
                    mk(2, "Endurance", at(0.6), Some(at(0.7))),
                    mk(3, "Tempo", at(0.7), Some(at(0.8))),
                    mk(4, "Threshold", at(0.8), Some(at(0.9))),
                    mk(5, "VO2 Max", at(0.9), None),
                ]
            }
            HrZoneModel::Lthr => {
                let l = if self.lthr > 0 { self.lthr as f64 } else { max * 0.9 };
                vec![
                    mk(1, "Recovery", 0.0, Some(l * 0.81)),
                    mk(2, "Aerobic", l * 0.81, Some(l * 0.90)),
                    mk(3, "Tempo", l * 0.90, Some(l * 0.94)),
                    mk(4, "Sub-threshold", l * 0.94, Some(l * 1.00)),
                    mk(5, "Supra-threshold", l * 1.00, Some(l * 1.03)),
                    mk(6, "Aerobic capacity", l * 1.03, Some(l * 1.07)),
                    mk(7, "Anaerobic", l * 1.07, None),
                ]
            }
        }
    }
}

/// 1-based zone for a heart rate (below zone 1 counts as zone 1).
pub fn hr_zone_of(hr: f64, zones: &[HrZone]) -> u32 {
    zones.iter().rev().find(|z| hr >= z.lo).map(|z| z.zone).unwrap_or(1)
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            name: "Rider".into(),
            weight_kg: 75.0,
            bike_weight_kg: 8.5,
            height_cm: 178.0,
            age: 40,
            sex: Sex::Male,
            ftp: 220.0,
            max_hr: 182,
            rest_hr: 55,
            units: Units::Metric,
            gross_efficiency: 0.24,
            cda: 0.32,
            crr: 0.004,
            trainer_difficulty: 1.0,
            max_trainer_grade: 16.0,
            min_trainer_grade: -10.0,
            weekly_goal_hours: 5.0,
            erg_ramp_s: 3.0,
            virtual_shifting: false,
            wheel_circumference_m: 2.105,
            start_gear: 10,
            hr_zone_model: HrZoneModel::MaxHr,
            lthr: 0,
            hr_alerts: HrAlertSettings::default(),
            ftp_auto: FtpAutoMode::Suggest,
        }
    }
}

impl Profile {
    pub fn wkg(&self) -> f64 {
        if self.weight_kg > 0.0 {
            self.ftp / self.weight_kg
        } else {
            0.0
        }
    }
}

/// Columnar 1 Hz samples. `hr` / `target` use 0 for "no value".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RideSamples {
    pub t: Vec<u32>,
    pub power: Vec<f32>,
    pub cadence: Vec<f32>,
    pub hr: Vec<f32>,
    pub speed: Vec<f32>,
    pub distance: Vec<f32>,
    pub ele: Vec<f32>,
    pub grade: Vec<f32>,
    pub target: Vec<f32>,
}

impl RideSamples {
    pub fn len(&self) -> usize {
        self.t.len()
    }
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
    pub fn power_f64(&self) -> Vec<f64> {
        self.power.iter().map(|&p| p as f64).collect()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RideKind {
    #[default]
    Free,
    Route,
    Workout,
    Test,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RideSummary {
    pub id: String,
    pub name: String,
    pub kind: RideKind,
    pub started_at: String,
    pub ended_at: String,
    pub route_id: Option<String>,
    pub route_name: Option<String>,
    pub workout_id: Option<String>,
    pub workout_name: Option<String>,
    pub event_id: Option<String>,
    pub test: Option<TestKind>,
    pub laps: u32,
    pub completed: bool,
    /// Time (s) to complete the full route (all laps), if completed.
    pub route_time_s: Option<f64>,
    pub personal_best: bool,
    pub elapsed_s: f64,
    pub moving_s: f64,
    pub distance_m: f64,
    pub elevation_gain_m: f64,
    pub avg_speed_mps: f64,
    pub max_speed_mps: f64,
    pub avg_power: f64,
    pub max_power: f64,
    pub np: f64,
    pub intensity_factor: f64,
    pub tss: f64,
    pub variability_index: f64,
    pub work_kj: f64,
    pub calories_kcal: f64,
    pub calories_hr_kcal: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: f64,
    pub max_cadence: f64,
    pub ftp: f64,
    pub weight_kg: f64,
    /// (duration seconds, best average watts)
    pub power_bests: Vec<(u32, f64)>,
    /// Seconds in Coggan power zones 1..7
    pub zone_s: [f64; 7],
    pub test_result: Option<TestResult>,
    pub notes: String,
    /// Manual laps, route laps and timed sectors.
    pub splits: Vec<Split>,
    /// The route was edited during the ride (sectors added).
    pub route_modified: bool,
    /// Seconds in each heart-rate zone (model at ride time).
    pub hr_zone_s: Vec<f64>,
    pub strava_activity_id: Option<u64>,
    /// Ridden on the built-in simulator (excluded from FTP estimates).
    pub simulated: bool,
}

/// A slice of a ride: a manual lap, a route lap or a timed sector.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Split {
    pub index: u32,
    /// "lap" | "route_lap" | "sector"
    pub kind: String,
    pub name: String,
    pub start_s: f64,
    pub duration_s: f64,
    pub distance_m: f64,
    pub elevation_gain_m: f64,
    pub avg_power: f64,
    pub max_power: f64,
    pub np: f64,
    pub avg_hr: Option<f64>,
    pub avg_cadence: f64,
    pub avg_speed_kmh: f64,
    pub avg_grade_pct: f64,
}

impl Split {
    /// Build from 1 Hz samples in `[a, b)`.
    pub fn from_samples(s: &RideSamples, a: usize, b: usize, kind: &str, name: String, index: u32) -> Option<Split> {
        let b = b.min(s.len());
        if b <= a {
            return None;
        }
        let p: Vec<f64> = s.power[a..b].iter().map(|x| *x as f64).collect();
        let hr: Vec<f64> = s.hr[a..b].iter().filter(|h| **h > 0.0).map(|h| *h as f64).collect();
        let cad: Vec<f64> = s.cadence[a..b].iter().filter(|c| **c > 0.0).map(|c| *c as f64).collect();
        let d0 = if a > 0 { s.distance[a - 1] } else { 0.0 } as f64;
        let dist = s.distance[b - 1] as f64 - d0;
        let mut gain = 0.0;
        let mut prev = if a > 0 { s.ele[a - 1] } else { s.ele[a] } as f64;
        for e in &s.ele[a..b] {
            let e = *e as f64;
            if e > prev && e - prev < 20.0 {
                gain += e - prev;
            }
            prev = e;
        }
        let dur = (b - a) as f64;
        let e0 = if a > 0 { s.ele[a - 1] } else { s.ele[a] } as f64;
        Some(Split {
            index,
            kind: kind.into(),
            name,
            start_s: s.t[a] as f64,
            duration_s: dur,
            distance_m: dist,
            elevation_gain_m: gain,
            avg_power: crate::metrics::mean(&p),
            max_power: p.iter().cloned().fold(0.0, f64::max),
            np: crate::metrics::normalized_power(&p),
            avg_hr: if hr.is_empty() { None } else { Some(crate::metrics::mean(&hr)) },
            avg_cadence: crate::metrics::mean(&cad),
            avg_speed_kmh: dist / dur * 3.6,
            avg_grade_pct: if dist > 1.0 { (s.ele[b - 1] as f64 - e0) / dist * 100.0 } else { 0.0 },
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RideRecord {
    pub summary: RideSummary,
    pub samples: RideSamples,
}

/// Distance-vs-time trace of a previous attempt, raced as a "ghost".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GhostTrace {
    pub ride_id: String,
    pub started_at: String,
    pub total_s: f64,
    pub t: Vec<f32>,
    pub d: Vec<f32>,
}

impl GhostTrace {
    pub fn from_record(r: &RideRecord) -> Self {
        GhostTrace {
            ride_id: r.summary.id.clone(),
            started_at: r.summary.started_at.clone(),
            total_s: r.summary.route_time_s.unwrap_or(r.summary.elapsed_s),
            t: r.samples.t.iter().map(|&x| x as f32).collect(),
            d: r.samples.distance.clone(),
        }
    }

    /// Ghost time (s) when it reached `dist`; None if it never did.
    pub fn time_at_distance(&self, dist: f64) -> Option<f64> {
        let n = self.d.len();
        if n == 0 || dist > self.d[n - 1] as f64 {
            return None;
        }
        let idx = self.d.partition_point(|&x| (x as f64) < dist);
        if idx == 0 {
            return Some(self.t[0] as f64 * (dist / (self.d[0] as f64).max(1e-6)).min(1.0));
        }
        let (d0, d1) = (self.d[idx - 1] as f64, self.d[idx] as f64);
        let (t0, t1) = (self.t[idx - 1] as f64, self.t[idx] as f64);
        let f = if d1 > d0 { (dist - d0) / (d1 - d0) } else { 0.0 };
        Some(t0 + f * (t1 - t0))
    }

    pub fn distance_at_time(&self, t: f64) -> f64 {
        let n = self.t.len();
        if n == 0 {
            return 0.0;
        }
        let idx = self.t.partition_point(|&x| (x as f64) < t);
        if idx == 0 {
            return self.d[0] as f64;
        }
        if idx >= n {
            return self.d[n - 1] as f64;
        }
        let (t0, t1) = (self.t[idx - 1] as f64, self.t[idx] as f64);
        let (d0, d1) = (self.d[idx - 1] as f64, self.d[idx] as f64);
        let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
        d0 + f * (d1 - d0)
    }
}

// ---------------------------------------------------------------------------
// Recurring events
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventTarget {
    Route { route_id: String, laps: u32 },
    Workout { workout_id: String, route_id: Option<String> },
    Test { test: TestKind },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Freq {
    Once,
    Daily,
    Weekly,
    EveryNDays,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Recurrence {
    pub freq: Freq,
    /// YYYY-MM-DD
    pub start_date: String,
    /// HH:MM local
    pub time: String,
    #[serde(default)]
    pub end_date: Option<String>,
    /// 0 = Monday … 6 = Sunday (weekly only; empty = start date's weekday)
    #[serde(default)]
    pub weekdays: Vec<u8>,
    /// Every N weeks (weekly) / N days (every_n_days). Defaults to 1.
    #[serde(default = "one")]
    pub interval: u32,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub target: EventTarget,
    pub recurrence: Recurrence,
    #[serde(default)]
    pub created_at: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "yes")]
    pub active: bool,
}

fn default_color() -> String {
    "#1DB870".into()
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FtpEntry {
    pub date: String,
    pub ftp: f64,
    pub source: String,
}

#[cfg(test)]
mod hr_tests {
    use super::*;

    #[test]
    fn hr_zone_models() {
        let mut p = Profile { max_hr: 190, rest_hr: 50, ..Default::default() };
        let z = p.hr_zones();
        assert_eq!(z.len(), 5);
        assert_eq!(z[1].lo, 114.0); // 60 % of 190
        assert_eq!(hr_zone_of(150.0, &z), 3);
        assert_eq!(hr_zone_of(80.0, &z), 1);
        assert_eq!(hr_zone_of(185.0, &z), 5);
        p.hr_zone_model = HrZoneModel::Reserve;
        let z = p.hr_zones();
        assert_eq!(z[1].lo, 134.0); // 50 + 0.6 × 140
        p.hr_zone_model = HrZoneModel::Lthr;
        p.lthr = 170;
        let z = p.hr_zones();
        assert_eq!(z.len(), 7);
        assert_eq!(hr_zone_of(171.0, &z), 5);
        assert_eq!(hr_zone_of(100.0, &z), 1);
    }
}
