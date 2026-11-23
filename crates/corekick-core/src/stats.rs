//! Account-level statistics: totals, weekly volume, fitness/fatigue (CTL/ATL/TSB),
//! power curve, records and streaks.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::metrics::BEST_DURATIONS;
use crate::model::{FtpEntry, Profile, RideSummary};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Totals {
    pub rides: u32,
    pub distance_m: f64,
    pub moving_s: f64,
    pub elevation_m: f64,
    pub work_kj: f64,
    pub calories_kcal: f64,
    pub tss: f64,
}

impl Totals {
    fn add(&mut self, r: &RideSummary) {
        self.rides += 1;
        self.distance_m += r.distance_m;
        self.moving_s += r.moving_s;
        self.elevation_m += r.elevation_gain_m;
        self.work_kj += r.work_kj;
        self.calories_kcal += r.calories_kcal;
        self.tss += r.tss;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WeekBucket {
    pub week_start: String,
    pub totals: Totals,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FitnessPoint {
    pub date: String,
    pub tss: f64,
    /// Chronic training load (42-day EWMA) – "fitness"
    pub ctl: f64,
    /// Acute training load (7-day EWMA) – "fatigue"
    pub atl: f64,
    /// Training stress balance (yesterday's CTL − ATL) – "form"
    pub tsb: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecordRef {
    pub ride_id: String,
    pub ride_name: String,
    pub date: String,
    pub value: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Records {
    pub longest_distance: Option<RecordRef>,
    pub longest_time: Option<RecordRef>,
    pub most_elevation: Option<RecordRef>,
    pub most_work: Option<RecordRef>,
    pub highest_np: Option<RecordRef>,
    pub highest_tss: Option<RecordRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurvePoint {
    pub duration_s: u32,
    pub watts: f64,
    pub wkg: f64,
    pub ride_id: String,
    pub date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AccountStats {
    pub all_time: Totals,
    pub this_week: Totals,
    pub last_week: Totals,
    pub this_month: Totals,
    pub this_year: Totals,
    pub weekly: Vec<WeekBucket>,
    pub fitness: Vec<FitnessPoint>,
    pub power_curve_all: Vec<CurvePoint>,
    pub power_curve_90d: Vec<CurvePoint>,
    pub records: Records,
    pub ftp_history: Vec<FtpEntry>,
    pub zone_s_28d: [f64; 7],
    pub week_streak: u32,
    pub ftp: f64,
    pub wkg: f64,
    pub weekly_goal_hours: f64,
}

pub fn ride_date(r: &RideSummary) -> Option<NaiveDate> {
    DateTime::<FixedOffset>::parse_from_rfc3339(&r.started_at).ok().map(|d| d.with_timezone(&Local).date_naive())
}

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

fn rec(best: &mut Option<RecordRef>, r: &RideSummary, value: f64, date: NaiveDate) {
    if value <= 0.0 {
        return;
    }
    if best.as_ref().map(|b| value > b.value).unwrap_or(true) {
        *best = Some(RecordRef { ride_id: r.id.clone(), ride_name: r.name.clone(), date: date.to_string(), value });
    }
}

fn curve(rides: &[(NaiveDate, &RideSummary)], weight: f64) -> Vec<CurvePoint> {
    let mut out = Vec::new();
    for &d in BEST_DURATIONS.iter() {
        let mut best: Option<CurvePoint> = None;
        for (date, r) in rides {
            if let Some(&(_, w)) = r.power_bests.iter().find(|(dd, _)| *dd == d) {
                if best.as_ref().map(|b| w > b.watts).unwrap_or(true) {
                    let kg = if r.weight_kg > 0.0 { r.weight_kg } else { weight };
                    best = Some(CurvePoint { duration_s: d, watts: w, wkg: if kg > 0.0 { w / kg } else { 0.0 }, ride_id: r.id.clone(), date: date.to_string() });
                }
            }
        }
        out.extend(best);
    }
    out
}

pub fn compute(rides: &[RideSummary], profile: &Profile, ftp_history: Vec<FtpEntry>, today: NaiveDate) -> AccountStats {
    let dated: Vec<(NaiveDate, &RideSummary)> = rides.iter().filter_map(|r| ride_date(r).map(|d| (d, r))).collect();
    let this_monday = monday(today);
    let last_monday = this_monday - Duration::days(7);

    let mut all = Totals::default();
    let mut week = Totals::default();
    let mut last_week = Totals::default();
    let mut month = Totals::default();
    let mut year = Totals::default();
    let mut records = Records::default();
    let mut zones = [0.0; 7];
    let mut by_week: BTreeMap<NaiveDate, Totals> = BTreeMap::new();
    let mut tss_by_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();

    for (d, r) in &dated {
        all.add(r);
        if *d >= this_monday {
            week.add(r);
        } else if *d >= last_monday {
            last_week.add(r);
        }
        if d.year() == today.year() && d.month() == today.month() {
            month.add(r);
        }
        if d.year() == today.year() {
            year.add(r);
        }
        by_week.entry(monday(*d)).or_default().add(r);
        *tss_by_day.entry(*d).or_default() += r.tss;
        if (today - *d).num_days() < 28 {
            for (z, s) in zones.iter_mut().zip(r.zone_s.iter()) {
                *z += s;
            }
        }
        rec(&mut records.longest_distance, r, r.distance_m, *d);
        rec(&mut records.longest_time, r, r.moving_s, *d);
        rec(&mut records.most_elevation, r, r.elevation_gain_m, *d);
        rec(&mut records.most_work, r, r.work_kj, *d);
        if r.moving_s >= 1200.0 {
            rec(&mut records.highest_np, r, r.np, *d);
        }
        rec(&mut records.highest_tss, r, r.tss, *d);
    }

    // Last 16 weeks including empty ones.
    let weekly: Vec<WeekBucket> = (0..16)
        .rev()
        .map(|k| {
            let ws = this_monday - Duration::days(7 * k);
            WeekBucket { week_start: ws.to_string(), totals: by_week.get(&ws).cloned().unwrap_or_default() }
        })
        .collect();

    // CTL / ATL over the last 180 days, seeded from the first ride.
    let first = tss_by_day.keys().next().cloned().unwrap_or(today);
    let start = first.min(today - Duration::days(180));
    let (kc, ka) = (1.0 - (-1.0_f64 / 42.0).exp(), 1.0 - (-1.0_f64 / 7.0).exp());
    let (mut ctl, mut atl) = (0.0, 0.0);
    let mut fitness = Vec::new();
    let mut d = start;
    while d <= today {
        let t = tss_by_day.get(&d).cloned().unwrap_or(0.0);
        let tsb = ctl - atl;
        ctl += (t - ctl) * kc;
        atl += (t - atl) * ka;
        if (today - d).num_days() < 120 {
            fitness.push(FitnessPoint { date: d.to_string(), tss: t, ctl, atl, tsb });
        }
        d = d.succ_opt().unwrap();
    }

    // Consecutive weeks (ending this week or last) with at least one ride.
    let mut streak = 0;
    let mut wk = if by_week.contains_key(&this_monday) { this_monday } else { last_monday };
    while by_week.get(&wk).map(|t| t.rides > 0).unwrap_or(false) {
        streak += 1;
        wk -= Duration::days(7);
    }

    let recent: Vec<(NaiveDate, &RideSummary)> = dated.iter().filter(|(d, _)| (today - *d).num_days() <= 90).cloned().collect();

    AccountStats {
        all_time: all,
        this_week: week,
        last_week,
        this_month: month,
        this_year: year,
        weekly,
        fitness,
        power_curve_all: curve(&dated, profile.weight_kg),
        power_curve_90d: curve(&recent, profile.weight_kg),
        records,
        ftp_history,
        zone_s_28d: zones,
        week_streak: streak,
        ftp: profile.ftp,
        wkg: profile.wkg(),
        weekly_goal_hours: profile.weekly_goal_hours,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ride(id: &str, date: &str, tss: f64, km: f64) -> RideSummary {
        RideSummary {
            id: id.into(),
            name: id.into(),
            started_at: format!("{date}T12:00:00+00:00"),
            tss,
            distance_m: km * 1000.0,
            moving_s: 3600.0,
            np: 200.0 + tss,
            power_bests: vec![(5, 600.0 + tss), (1200, 250.0)],
            weight_kg: 75.0,
            ..Default::default()
        }
    }

    #[test]
    fn totals_streak_and_fitness() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(); // Monday
        let rides = vec![
            ride("a", "2026-11-23", 80.0, 40.0),
            ride("b", "2026-11-18", 60.0, 30.0),
            ride("c", "2026-11-10", 50.0, 25.0),
            ride("d", "2026-06-01", 100.0, 90.0),
        ];
        let s = compute(&rides, &Profile::default(), vec![], today);
        assert_eq!(s.all_time.rides, 4);
        assert_eq!(s.this_week.rides, 1);
        assert_eq!(s.last_week.rides, 1);
        assert_eq!(s.this_month.rides, 3);
        assert_eq!(s.week_streak, 3);
        assert_eq!(s.weekly.len(), 16);
        assert_eq!(s.weekly.last().unwrap().week_start, "2026-11-23");
        assert_eq!(s.records.longest_distance.as_ref().unwrap().ride_id, "d");
        let last = s.fitness.last().unwrap();
        assert!(last.ctl > 0.0 && last.atl > last.ctl);
        let five = s.power_curve_all.iter().find(|c| c.duration_s == 5).unwrap();
        assert_eq!(five.watts, 700.0);
        let five90 = s.power_curve_90d.iter().find(|c| c.duration_s == 5).unwrap();
        assert_eq!(five90.watts, 680.0);
    }

    #[test]
    fn empty() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let s = compute(&[], &Profile::default(), vec![], today);
        assert_eq!(s.all_time.rides, 0);
        assert_eq!(s.week_streak, 0);
        assert!(s.power_curve_all.is_empty());
    }
}
