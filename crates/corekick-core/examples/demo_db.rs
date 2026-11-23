//! Build a realistic demo database (12 weeks of riding) by running the real
//! ride engine fast-forward with a synthetic rider. Used for screenshots.
//!
//!     cargo run -p corekick-core --example demo_db -- /path/to/corekick.db

use chrono::{Duration, Local, NaiveDate, TimeZone};
use corekick_core::db::Store;
use corekick_core::engine::{RideEngine, RideSetup, TrainerInput};
use corekick_core::model::*;
use corekick_core::rng::Rng;
use corekick_core::route::seed_routes;
use corekick_core::workout::builtin_workouts;

struct Plan {
    days_ago: i64,
    hour: u32,
    kind: RideKind,
    route: Option<&'static str>,
    workout: Option<&'static str>,
    laps: u32,
    /// Free / route effort as a fraction of true FTP
    effort: f64,
    minutes: f64,
    event: Option<usize>,
    /// Include a hard sustained block (seconds, fraction of FTP) at the midpoint
    push: Option<(f64, f64)>,
}

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("db path");
    let _ = std::fs::remove_file(&path);
    let store = Store::open(std::path::Path::new(&path))?;
    let routes = seed_routes();
    for r in &routes {
        store.save_route(r)?;
    }
    store.set_meta("seeded", "1")?;
    let today = Local::now().date_naive();
    let mut profile = Profile { name: "Scott".into(), weight_kg: 76.0, ftp: 225.0, max_hr: 188, rest_hr: 48, age: 41, ..Default::default() };
    store.save_profile(&profile)?;
    let d = |n: i64| (today - Duration::days(n)).to_string();
    store.add_ftp(&FtpEntry { date: d(84), ftp: 225.0, source: "Initial".into() })?;

    let route = |name: &str| routes.iter().find(|r| r.name == name).unwrap().clone();
    let workouts = builtin_workouts();
    let workout = |id: &str| workouts.iter().find(|w| w.id == format!("builtin-{id}")).unwrap().clone();

    // Recurring events
    let hill = Event {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Tuesday hill reps".into(),
        description: "Three laps of the lunchtime loop – chase the ghost.".into(),
        target: EventTarget::Route { route_id: route("Lunchtime Hill Reps").id, laps: 3 },
        recurrence: Recurrence { freq: Freq::Weekly, start_date: d(84), time: "18:30".into(), end_date: None, weekdays: vec![1], interval: 1 },
        created_at: d(84),
        color: "#E3A600".into(),
        active: true,
    };
    let ss = Event {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Thursday sweet spot".into(),
        description: "Midweek quality session.".into(),
        target: EventTarget::Workout { workout_id: "builtin-sweet-spot-2x20".into(), route_id: None },
        recurrence: Recurrence { freq: Freq::Weekly, start_date: d(84), time: "07:00".into(), end_date: None, weekdays: vec![3], interval: 1 },
        created_at: d(84),
        color: "#2F7FE0".into(),
        active: true,
    };
    let sat = Event {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Saturday long ride".into(),
        description: "Big aerobic day, alternating routes.".into(),
        target: EventTarget::Route { route_id: route("Valley Classic").id, laps: 1 },
        recurrence: Recurrence { freq: Freq::Weekly, start_date: d(84), time: "09:00".into(), end_date: None, weekdays: vec![5], interval: 1 },
        created_at: d(84),
        color: "#1DB870".into(),
        active: true,
    };
    let events = [hill, ss, sat];
    for e in &events {
        store.save_event(e)?;
    }

    // 12 weeks, 3–4 rides a week, building fitness. Day 0 = today (Monday).
    let mut plans: Vec<Plan> = vec![];
    for wk in 0..12i64 {
        let base = 84 - wk * 7; // Monday of that week
        if base - 1 < 0 { break; }
        plans.push(Plan { days_ago: base - 1, hour: 18, kind: RideKind::Route, route: Some("Lunchtime Hill Reps"), workout: None, laps: 3, effort: 0.82, minutes: 0.0, event: Some(0), push: None });
        if base - 3 >= 0 {
            let w = ["sweet-spot-2x20", "sweet-spot-2x20", "threshold-3x10", "sweet-spot-2x20", "vo2-5x3", "over-unders"][wk as usize % 6];
            plans.push(Plan { days_ago: base - 3, hour: 7, kind: RideKind::Workout, route: None, workout: Some(w), laps: 1, effort: 0.0, minutes: 0.0, event: if w == "sweet-spot-2x20" { Some(1) } else { None }, push: None });
        }
        if base - 5 >= 0 {
            let r = ["Valley Classic", "Rolling Downs", "Col de Kickr", "Valley Classic"][wk as usize % 4];
            plans.push(Plan { days_ago: base - 5, hour: 9, kind: RideKind::Route, route: Some(r), workout: None, laps: 1, effort: 0.7, minutes: 0.0, event: if r == "Valley Classic" { Some(2) } else { None }, push: if wk >= 8 { Some((1200.0, 1.0)) } else { None } });
        }
        if wk % 2 == 1 && base - 6 >= 0 {
            plans.push(Plan { days_ago: base - 6, hour: 17, kind: RideKind::Free, route: None, workout: None, laps: 1, effort: 0.6, minutes: 45.0, event: None, push: if wk >= 9 { Some((300.0, 1.2)) } else { None } });
        }
    }
    plans.push(Plan { days_ago: 0, hour: 7, kind: RideKind::Route, route: Some("Coastal Flats"), workout: None, laps: 1, effort: 0.62, minutes: 0.0, event: None, push: None });
    plans.sort_by_key(|p| -p.days_ago);

    let mut rng = Rng::new(7);
    let mut best_times: std::collections::HashMap<(String, u32), f64> = Default::default();
    for p in plans {
        // True fitness climbs from 228 W to ~252 W over the 12 weeks.
        let true_ftp = 228.0 + (84 - p.days_ago) as f64 * 0.29;
        if p.days_ago <= 35 && profile.ftp < 240.0 {
            profile.ftp = 240.0;
            store.save_profile(&profile)?;
            store.add_ftp(&FtpEntry { date: d(35), ftp: 240.0, source: "20-Minute FTP Test".into() })?;
        }
        let date: NaiveDate = today - Duration::days(p.days_ago);
        let start = Local.from_local_datetime(&date.and_hms_opt(p.hour, 0, 0).unwrap()).unwrap();
        let (name, r, w) = match p.kind {
            RideKind::Workout => { let w = workout(p.workout.unwrap()); (w.name.clone(), None, Some(w)) }
            RideKind::Route => { let r = route(p.route.unwrap()); (r.name.clone(), Some(r), None) }
            _ => ("Free ride".to_string(), Some(corekick_core::route::endless_route(corekick_core::route::Terrain::Rolling, 99 + p.days_ago as u64)), None),
        };
        let mut setup = RideSetup::simple(p.kind, &name);
        setup.route = r;
        setup.workout = w;
        setup.laps = p.laps;
        setup.event_id = p.event.map(|i| events[i].id.clone());
        let mut eng = RideEngine::new(profile.clone(), setup, start.to_rfc3339());
        let mut hr = 70.0;
        let mut wob = 0.0;
        let cap_s = if p.minutes > 0.0 { p.minutes * 60.0 } else { 4.0 * 3600.0 };
        let mut now = 0u64;
        let mut power = 0.0;
        let mut cad = 88.0;
        loop {
            let el = now as f64 / 1000.0;
            let input = TrainerInput { power_w: power, cadence_rpm: cad, heart_rate: Some(hr), connected: true };
            let (snap, _) = eng.tick(now, input);
            if snap.finished || snap.route_complete || snap.workout_complete || el >= cap_s {
                break;
            }
            wob = wob * 0.97 + rng.range(-1.0, 1.0) * 0.03;
            let mut target = match snap.target_w {
                Some(t) if t > 0.0 => t * (1.0 + rng.range(-0.025, 0.025)),
                _ => {
                    let g = snap.grade_pct.clamp(-6.0, 10.0);
                    let warm = (el / 600.0).min(1.0);
                    let f = p.effort * (0.75 + 0.25 * warm) * (1.0 + 0.07 * g) * (1.0 + wob * 3.0);
                    (f * true_ftp).max(if g < -2.0 { 30.0 } else { 90.0 })
                }
            };
            if let Some((len, frac)) = p.push {
                let mid = if p.minutes > 0.0 { cap_s * 0.5 } else { 2400.0 };
                if el > mid && el < mid + len {
                    target = true_ftp * frac * (1.0 + wob * 2.5 + rng.range(-0.07, 0.07));
                }
            }
            power = power + (target - power) * 0.35;
            cad = (cad + (90.0 + wob * 60.0 - cad) * 0.1 + rng.range(-1.5, 1.5)).clamp(70.0, 105.0);
            let hr_target = 62.0 + 110.0 * (power / true_ftp).min(1.3) * 0.95 + el / 600.0;
            hr += (hr_target.min(186.0) - hr) * 0.012;
            now += 250;
        }
        let end = start + Duration::milliseconds(now as i64);
        let mut rec = eng.finish(end.to_rfc3339(), uuid::Uuid::new_v4().to_string());
        if let (Some(rid), Some(t)) = (rec.summary.route_id.clone(), rec.summary.route_time_s) {
            let k = (rid, rec.summary.laps);
            let prev = best_times.get(&k).copied();
            rec.summary.personal_best = prev.map(|pt| t < pt).unwrap_or(true);
            if rec.summary.personal_best { best_times.insert(k, t); }
        }
        println!("{} {:<22} {:>5.0} min {:>4.0} W np {:>4.0} tss {:>4.0}", date, rec.summary.name, rec.summary.moving_s / 60.0, rec.summary.avg_power, rec.summary.np, rec.summary.tss);
        store.save_ride(&rec)?;
    }
    Ok(())
}
