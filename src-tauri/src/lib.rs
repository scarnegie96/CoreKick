//! CoreKick desktop app (Tauri 2).

mod ble;
mod commands;
mod sim;
mod strava;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use corekick_core::db::Store;
use corekick_core::engine::{RideEngine, RideInfo, TrainerCommand};
use corekick_core::model::{FtpEntry, Profile};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub struct ActiveRide {
    pub id: String,
    pub engine: Arc<Mutex<RideEngine>>,
    pub stop: Arc<AtomicBool>,
    pub info: RideInfo,
}

pub struct AppState {
    pub store: Mutex<Store>,
    pub ble: Arc<ble::Ble>,
    pub sim: Arc<Mutex<sim::SimTrainer>>,
    pub use_sim: Arc<AtomicBool>,
    pub profile: Arc<Mutex<Profile>>,
    pub ride: Mutex<Option<ActiveRide>>,
}

#[derive(Clone, Serialize)]
pub struct LiveTelemetry {
    pub source: &'static str,
    pub connected: bool,
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub heart_rate: Option<f64>,
}

/// Where trainer data comes from this tick (simulator or real Bluetooth).
fn read_input(state: &AppState, dt: f64, last: Option<TrainerCommand>) -> corekick_core::engine::TrainerInput {
    if state.use_sim.load(Ordering::SeqCst) {
        let p = state.profile.lock().unwrap().clone();
        let mut s = state.sim.lock().unwrap();
        if let Some(c) = last {
            s.command(c);
        }
        s.update(dt, &p)
    } else {
        state.ble.input()
    }
}

/// The 4 Hz ride loop: telemetry → engine → trainer command + UI snapshot.
pub fn spawn_ride_loop(app: AppHandle, engine: Arc<Mutex<RideEngine>>, stop: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        let start = Instant::now();
        let mut last = Instant::now();
        let mut iv = tokio::time::interval(Duration::from_millis(250));
        iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_cmd: Option<TrainerCommand> = None;
        loop {
            iv.tick().await;
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let state = app.state::<AppState>();
            let dt = last.elapsed().as_secs_f64();
            last = Instant::now();
            let input = read_input(&state, dt, last_cmd);
            let (snap, cmd) = engine.lock().unwrap().tick(start.elapsed().as_millis() as u64, input);
            if let Some(c) = cmd {
                last_cmd = Some(c);
                if state.use_sim.load(Ordering::SeqCst) {
                    state.sim.lock().unwrap().command(c);
                } else {
                    state.ble.send(c);
                }
            }
            let _ = app.emit("ride://tick", &snap);
        }
    });
}

/// Broadcast live readings at 2 Hz when no ride is running (Devices page, pre-ride).
fn spawn_idle_telemetry(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut iv = tokio::time::interval(Duration::from_millis(500));
        loop {
            iv.tick().await;
            let state = app.state::<AppState>();
            if state.ride.lock().unwrap().is_some() {
                continue;
            }
            let sim = state.use_sim.load(Ordering::SeqCst);
            let input = read_input(&state, 0.5, None);
            let t = LiveTelemetry {
                source: if sim { "simulator" } else { "bluetooth" },
                connected: input.connected,
                power_w: input.power_w,
                cadence_rpm: input.cadence_rpm,
                heart_rate: input.heart_rate,
            };
            let _ = app.emit("ble://telemetry", t);
        }
    });
}

fn seed(store: &Store) -> anyhow::Result<()> {
    if store.meta("seeded")?.is_none() {
        for r in corekick_core::route::seed_routes() {
            store.save_route(&r)?;
        }
        let p = store.profile()?;
        store.save_profile(&p)?;
        store.add_ftp(&FtpEntry { date: chrono::Local::now().date_naive().to_string(), ftp: p.ftp, source: "Initial".into() })?;
        store.set_meta("seeded", "1")?;
    }
    Ok(())
}

/// Re-estimate FTP from the last 90 days of valid rides and act on it
/// according to the rider's preference. Persists and broadcasts the result.
pub fn reassess_ftp(app: &AppHandle, trigger: Option<String>) -> Option<corekick_core::ftp_estimate::FtpAssessment> {
    use corekick_core::ftp_estimate::{assess, estimate, FtpAssessment, Suggestion};
    use corekick_core::model::FtpAutoMode;
    let state = app.state::<AppState>();
    let today = chrono::Local::now().date_naive();
    let store = state.store.lock().unwrap();
    let mut profile = store.profile().ok()?;
    let rides = store.ride_summaries().ok()?;
    let prev: Option<FtpAssessment> = store.meta("ftp_assessment").ok().flatten().and_then(|j| serde_json::from_str(&j).ok());
    let mut a = assess(profile.ftp, estimate(&rides, today), today, trigger);
    // Keep a dismissal until the suggestion meaningfully changes.
    if let Some(p) = &prev {
        if p.dismissed && p.suggestion == a.suggestion {
            if let (Some(x), Some(y)) = (p.suggested_ftp, a.suggested_ftp) {
                if (x - y).abs() < 5.0 {
                    a.dismissed = true;
                }
            }
        }
    }
    if profile.ftp_auto == FtpAutoMode::AutoIncrease && a.suggestion == Suggestion::Increase && !a.dismissed {
        if let Some(new) = a.suggested_ftp {
            profile.ftp = new;
            let _ = store.save_profile(&profile);
            let _ = store.add_ftp(&FtpEntry { date: today.to_string(), ftp: new, source: "Auto (eFTP)".into() });
            *state.profile.lock().unwrap() = profile.clone();
            a.auto_applied = true;
        }
    }
    let _ = store.set_meta("ftp_assessment", &serde_json::to_string(&a).unwrap_or_default());
    drop(store);
    let _ = app.emit("ftp://assessment", &a);
    if a.auto_applied {
        let _ = app.emit("profile://changed", &profile);
    }
    Some(a)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let store = Store::open(&dir.join("corekick.db"))?;
            seed(&store)?;
            let profile = store.profile()?;
            let use_sim = std::env::var("COREKICK_SIMULATOR").map(|v| v == "1").unwrap_or(false);
            app.manage(AppState {
                sim: Arc::new(Mutex::new(sim::SimTrainer::new(&profile))),
                profile: Arc::new(Mutex::new(profile)),
                store: Mutex::new(store),
                ble: ble::Ble::new(app.handle().clone()),
                use_sim: Arc::new(AtomicBool::new(use_sim)),
                ride: Mutex::new(None),
            });
            spawn_idle_telemetry(app.handle().clone());
            // Daily re-assessment (older efforts age out of the 90-day window).
            let today = chrono::Local::now().date_naive().to_string();
            let last = app.state::<AppState>().store.lock().unwrap().meta("ftp_assessment").ok().flatten()
                .and_then(|j| serde_json::from_str::<corekick_core::ftp_estimate::FtpAssessment>(&j).ok())
                .map(|a| a.date);
            if last.as_deref() != Some(today.as_str()) {
                reassess_ftp(app.handle(), None);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_profile,
            commands::save_profile,
            commands::set_ftp,
            commands::list_routes,
            commands::get_route,
            commands::save_route,
            commands::delete_route,
            commands::toggle_favourite_route,
            commands::generate_route,
            commands::build_route,
            commands::import_gpx,
            commands::suggested_gain,
            commands::list_workouts,
            commands::save_workout,
            commands::delete_workout,
            commands::preview_workout,
            commands::parse_workout_text,
            commands::workout_to_text,
            commands::import_workout,
            commands::export_workout,
            commands::list_tests,
            commands::list_events,
            commands::save_event,
            commands::delete_event,
            commands::upcoming_events,
            commands::event_rides,
            commands::list_rides,
            commands::get_ride,
            commands::delete_ride,
            commands::update_ride,
            commands::export_ride,
            commands::account_stats,
            commands::ble_status,
            commands::ble_devices,
            commands::ble_scan,
            commands::ble_stop_scan,
            commands::ble_connect,
            commands::ble_disconnect,
            commands::sim_get,
            commands::sim_set,
            commands::ride_start,
            commands::ride_info,
            commands::ride_pause,
            commands::ride_resume,
            commands::ride_action,
            commands::ride_end,
            commands::hr_zones,
            commands::ftp_assessment,
            commands::ftp_reassess,
            commands::ftp_apply_suggestion,
            commands::ftp_dismiss,
            commands::strava_status,
            commands::strava_save_client,
            commands::strava_connect,
            commands::strava_disconnect,
            commands::strava_set_auto,
            commands::strava_upload,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running CoreKick");
}
