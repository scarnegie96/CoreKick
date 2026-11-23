//! Tauri IPC commands. Every command returns `Result<T, String>` so errors
//! surface as rejected promises in the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Local;
use corekick_core::engine::{FreeOptions, Goal, RideAction, RideEngine, RideInfo, RideSetup};
use corekick_core::events::{self, Occurrence};
use corekick_core::fitness_test::{self, TestInfo, TestKind};
use corekick_core::model::{Event, EventTarget, FtpEntry, GhostTrace, HrZone, Profile, RideKind, RideRecord, RideSummary};
use corekick_core::physics::PhysicsParams;
use corekick_core::route::{self, GenParams, Route, RouteStats, SegmentSpec, Terrain};
use corekick_core::stats::{self, AccountStats};
use corekick_core::workout::{self, builtin_workouts, Block, Step, Workout, WorkoutEstimate};
use corekick_core::{export, new_id, now_rfc3339};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ble::{BleStatus, DeviceInfo};
use crate::strava::{self, StravaStatus};
use tauri_plugin_opener::OpenerExt;
use crate::{spawn_ride_loop, ActiveRide, AppState};

type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    format!("{err:#}")
}

fn profile(state: &AppState) -> Profile {
    state.profile.lock().unwrap().clone()
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>) -> R<Profile> {
    Ok(profile(&state))
}

#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: Profile) -> R<Profile> {
    let store = state.store.lock().unwrap();
    let old = store.profile().map_err(e)?;
    let mut p = profile;
    p.ftp = p.ftp.clamp(50.0, 600.0);
    p.weight_kg = p.weight_kg.clamp(30.0, 200.0);
    p.gross_efficiency = p.gross_efficiency.clamp(0.15, 0.30);
    p.trainer_difficulty = p.trainer_difficulty.clamp(0.0, 1.5);
    p.max_trainer_grade = p.max_trainer_grade.clamp(1.0, 20.0);
    p.min_trainer_grade = p.min_trainer_grade.clamp(-20.0, 0.0);
    store.save_profile(&p).map_err(e)?;
    if (old.ftp - p.ftp).abs() >= 1.0 {
        store
            .add_ftp(&FtpEntry { date: Local::now().date_naive().to_string(), ftp: p.ftp, source: "Manual".into() })
            .map_err(e)?;
    }
    *state.profile.lock().unwrap() = p.clone();
    Ok(p)
}

#[tauri::command]
pub fn set_ftp(state: State<'_, AppState>, ftp: f64, source: String) -> R<Profile> {
    let store = state.store.lock().unwrap();
    let mut p = store.profile().map_err(e)?;
    p.ftp = ftp.clamp(50.0, 600.0).round();
    store.save_profile(&p).map_err(e)?;
    store.add_ftp(&FtpEntry { date: Local::now().date_naive().to_string(), ftp: p.ftp, source }).map_err(e)?;
    *state.profile.lock().unwrap() = p.clone();
    Ok(p)
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RouteSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: route::RouteSource,
    pub created_at: String,
    pub is_loop: bool,
    pub tags: Vec<String>,
    pub favourite: bool,
    pub stats: RouteStats,
    pub thumbnail: Vec<[f64; 2]>,
    pub est_time_s: f64,
    pub best_time_s: Option<f64>,
    pub attempts: u32,
}

#[derive(Serialize)]
pub struct RouteDetail {
    pub route: Route,
    pub stats: RouteStats,
    pub est_time_s: f64,
    pub best_time_s: Option<f64>,
    pub attempts: Vec<RideSummary>,
}

fn est_time(r: &Route, p: &Profile) -> f64 {
    r.estimate_time(p.ftp * 0.7, &PhysicsParams::from_profile(p))
}

fn summarize_route(r: &Route, p: &Profile, rides: &[RideSummary]) -> RouteSummary {
    let mine: Vec<&RideSummary> = rides.iter().filter(|x| x.route_id.as_deref() == Some(&r.id)).collect();
    let best = mine.iter().filter(|x| x.completed && x.laps == 1).filter_map(|x| x.route_time_s).fold(None, |m: Option<f64>, t| {
        Some(m.map_or(t, |m| m.min(t)))
    });
    RouteSummary {
        id: r.id.clone(),
        name: r.name.clone(),
        description: r.description.clone(),
        source: r.source,
        created_at: r.created_at.clone(),
        is_loop: r.is_loop,
        tags: r.tags.clone(),
        favourite: r.favourite,
        stats: r.stats(),
        thumbnail: r.thumbnail(160),
        est_time_s: est_time(r, p),
        best_time_s: best,
        attempts: mine.len() as u32,
    }
}

fn detail(r: Route, p: &Profile, attempts: Vec<RideSummary>) -> RouteDetail {
    let best = attempts.iter().filter(|x| x.completed && x.laps == 1).filter_map(|x| x.route_time_s).fold(None, |m: Option<f64>, t| {
        Some(m.map_or(t, |m| m.min(t)))
    });
    RouteDetail { stats: r.stats(), est_time_s: est_time(&r, p), best_time_s: best, attempts, route: r }
}

#[tauri::command]
pub fn list_routes(state: State<'_, AppState>) -> R<Vec<RouteSummary>> {
    let p = profile(&state);
    let store = state.store.lock().unwrap();
    let rides = store.ride_summaries().map_err(e)?;
    let mut v: Vec<RouteSummary> = store.routes().map_err(e)?.iter().map(|r| summarize_route(r, &p, &rides)).collect();
    v.sort_by(|a, b| b.favourite.cmp(&a.favourite).then(b.created_at.cmp(&a.created_at)));
    Ok(v)
}

#[tauri::command]
pub fn get_route(state: State<'_, AppState>, id: String) -> R<RouteDetail> {
    let p = profile(&state);
    let store = state.store.lock().unwrap();
    let r = store.route(&id).map_err(e)?.ok_or("Route not found")?;
    let attempts = store.rides_for_route(&id).map_err(e)?;
    Ok(detail(r, &p, attempts))
}

#[tauri::command]
pub fn save_route(state: State<'_, AppState>, route: Route) -> R<Route> {
    let mut r = route;
    if r.points.len() < 2 {
        return Err("A route needs at least two points".into());
    }
    if r.id.is_empty() {
        r.id = new_id();
    }
    if r.created_at.is_empty() {
        r.created_at = now_rfc3339();
    }
    if r.source == route::RouteSource::Builtin {
        // editing a built-in creates the user's own copy
        r.source = route::RouteSource::Custom;
    }
    state.store.lock().unwrap().save_route(&r).map_err(e)?;
    Ok(r)
}

#[tauri::command]
pub fn delete_route(state: State<'_, AppState>, id: String) -> R<()> {
    state.store.lock().unwrap().delete_route(&id).map_err(e)
}

#[tauri::command]
pub fn toggle_favourite_route(state: State<'_, AppState>, id: String) -> R<bool> {
    let store = state.store.lock().unwrap();
    let mut r = store.route(&id).map_err(e)?.ok_or("Route not found")?;
    r.favourite = !r.favourite;
    store.save_route(&r).map_err(e)?;
    Ok(r.favourite)
}

/// Preview a generated route (not saved until `save_route`).
#[tauri::command]
pub fn generate_route(state: State<'_, AppState>, params: GenParams, name: String) -> R<RouteDetail> {
    let p = profile(&state);
    let name = if name.trim().is_empty() { "Generated route".to_string() } else { name };
    Ok(detail(route::generate(&params, &name), &p, vec![]))
}

#[tauri::command]
pub fn build_route(state: State<'_, AppState>, segments: Vec<SegmentSpec>, name: String, start_ele: f64) -> R<RouteDetail> {
    if segments.is_empty() {
        return Err("Add at least one segment".into());
    }
    let p = profile(&state);
    let name = if name.trim().is_empty() { "Custom route".to_string() } else { name };
    Ok(detail(route::build_from_segments(&name, &segments, start_ele, true), &p, vec![]))
}

#[tauri::command]
pub fn import_gpx(state: State<'_, AppState>, text: String, name: Option<String>) -> R<RouteDetail> {
    let p = profile(&state);
    let r = route::route_from_gpx(&text, name.as_deref().filter(|s| !s.trim().is_empty()))?;
    state.store.lock().unwrap().save_route(&r).map_err(e)?;
    Ok(detail(r, &p, vec![]))
}

#[tauri::command]
pub fn suggested_gain(terrain: Terrain, distance_km: f64) -> f64 {
    route::suggested_gain(terrain, distance_km)
}

// ---------------------------------------------------------------------------
// Workouts & tests
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct WorkoutView {
    pub workout: Workout,
    pub steps: Vec<Step>,
    pub estimate: WorkoutEstimate,
    pub times_ridden: u32,
}

fn view(w: Workout, ftp: f64, rides: &[RideSummary]) -> WorkoutView {
    let n = rides.iter().filter(|r| r.workout_id.as_deref() == Some(&w.id)).count() as u32;
    WorkoutView { steps: w.steps(), estimate: w.estimate(ftp), times_ridden: n, workout: w }
}

fn find_workout(state: &AppState, id: &str) -> R<Workout> {
    if let Some(w) = builtin_workouts().into_iter().find(|w| w.id == id) {
        return Ok(w);
    }
    state.store.lock().unwrap().workout(id).map_err(e)?.ok_or_else(|| "Workout not found".to_string())
}

#[tauri::command]
pub fn list_workouts(state: State<'_, AppState>) -> R<Vec<WorkoutView>> {
    let ftp = profile(&state).ftp;
    let store = state.store.lock().unwrap();
    let rides = store.ride_summaries().map_err(e)?;
    let mut v: Vec<WorkoutView> = store.workouts().map_err(e)?.into_iter().map(|w| view(w, ftp, &rides)).collect();
    v.extend(builtin_workouts().into_iter().map(|w| view(w, ftp, &rides)));
    Ok(v)
}

#[tauri::command]
pub fn save_workout(state: State<'_, AppState>, workout: Workout) -> R<WorkoutView> {
    let mut w = workout;
    if w.blocks.is_empty() {
        return Err("A workout needs at least one block".into());
    }
    if w.id.is_empty() || w.builtin {
        w.id = new_id();
        w.builtin = false;
        w.created_at = now_rfc3339();
    }
    if w.created_at.is_empty() {
        w.created_at = now_rfc3339();
    }
    state.store.lock().unwrap().save_workout(&w).map_err(e)?;
    Ok(view(w, profile(&state).ftp, &[]))
}

#[tauri::command]
pub fn delete_workout(state: State<'_, AppState>, id: String) -> R<()> {
    state.store.lock().unwrap().delete_workout(&id).map_err(e)
}

/// Parse the compact text format (e.g. `10m 50-75%, 3x(8m @95%, 4m @55%)`).
#[tauri::command]
pub fn parse_workout_text(state: State<'_, AppState>, text: String) -> R<Vec<Block>> {
    workout::parse_text(&text, profile(&state).ftp)
}

#[tauri::command]
pub fn workout_to_text(blocks: Vec<Block>) -> String {
    workout::to_text(&blocks)
}

/// Import a .zwo / .erg / .mrc / text file and save it to the library.
#[tauri::command]
pub fn import_workout(state: State<'_, AppState>, file_name: String, text: String) -> R<WorkoutView> {
    let ftp = profile(&state).ftp;
    let mut w = workout::import_file(&file_name, &text, ftp)?;
    w.id = new_id();
    w.created_at = now_rfc3339();
    state.store.lock().unwrap().save_workout(&w).map_err(e)?;
    Ok(view(w, ftp, &[]))
}

/// Export a workout as Zwift .zwo (works in Zwift, TrainerRoad, MyWhoosh, Rouvy…).
#[tauri::command]
pub fn export_workout(state: State<'_, AppState>, id: String, path: String) -> R<String> {
    let w = find_workout(&state, &id)?;
    std::fs::write(&path, workout::to_zwo(&w)).map_err(e)?;
    Ok(path)
}

#[tauri::command]
pub fn preview_workout(state: State<'_, AppState>, workout: Workout) -> R<WorkoutView> {
    Ok(view(workout, profile(&state).ftp, &[]))
}

#[derive(Serialize)]
pub struct TestView {
    pub info: TestInfo,
    pub steps: Vec<Step>,
    pub last_result: Option<RideSummary>,
}

#[tauri::command]
pub fn list_tests(state: State<'_, AppState>) -> R<Vec<TestView>> {
    let ftp = profile(&state).ftp;
    let rides = state.store.lock().unwrap().ride_summaries().map_err(e)?;
    Ok(fitness_test::test_catalogue()
        .into_iter()
        .map(|info| {
            let last = rides.iter().find(|r| r.test == Some(info.kind) && r.test_result.is_some()).cloned();
            TestView { steps: fitness_test::protocol(info.kind, ftp).steps(), info, last_result: last }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct EventView {
    pub event: Event,
    pub target_name: String,
    pub next: Option<String>,
    pub attempts: u32,
    pub best_time_s: Option<f64>,
    pub best_np: Option<f64>,
    pub last_ride: Option<RideSummary>,
    pub streak: u32,
}

fn target_name(state: &AppState, t: &EventTarget) -> String {
    match t {
        EventTarget::Route { route_id, laps } => {
            let n = state.store.lock().unwrap().route(route_id).ok().flatten().map(|r| r.name).unwrap_or_else(|| "Deleted route".into());
            if *laps > 1 {
                format!("{n} × {laps}")
            } else {
                n
            }
        }
        EventTarget::Workout { workout_id, .. } => find_workout(state, workout_id).map(|w| w.name).unwrap_or_else(|_| "Deleted workout".into()),
        EventTarget::Test { test } => fitness_test::test_catalogue().into_iter().find(|i| i.kind == *test).map(|i| i.name).unwrap_or_default(),
    }
}

fn event_view(state: &AppState, ev: Event) -> R<EventView> {
    let rides = state.store.lock().unwrap().rides_for_event(&ev.id).map_err(e)?;
    let today = Local::now().date_naive();
    let next = events::occurrences(&ev, today, today + chrono::Duration::days(366))
        .into_iter()
        .find(|dt| *dt >= Local::now().naive_local() - chrono::Duration::hours(2))
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string());
    let best_time = rides.iter().filter(|r| r.completed).filter_map(|r| r.route_time_s).fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.min(t))));
    let best_np = rides.iter().map(|r| r.np).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
    // Streak: consecutive past occurrences (most recent first) that have a ride on that day.
    let ride_days: std::collections::HashSet<chrono::NaiveDate> = rides.iter().filter_map(stats::ride_date).collect();
    let past = events::occurrences(&ev, today - chrono::Duration::days(365), today);
    let mut streak = 0;
    for o in past.iter().rev() {
        let d = o.date();
        if ride_days.contains(&d) {
            streak += 1;
        } else if d < today {
            break;
        }
    }
    Ok(EventView {
        target_name: target_name(state, &ev.target),
        next,
        attempts: rides.len() as u32,
        best_time_s: best_time,
        best_np,
        last_ride: rides.first().cloned(),
        streak,
        event: ev,
    })
}

#[tauri::command]
pub fn list_events(state: State<'_, AppState>) -> R<Vec<EventView>> {
    let evs = state.store.lock().unwrap().events().map_err(e)?;
    evs.into_iter().map(|ev| event_view(&state, ev)).collect()
}

#[tauri::command]
pub fn save_event(state: State<'_, AppState>, event: Event) -> R<EventView> {
    let mut ev = event;
    if events::parse_date(&ev.recurrence.start_date).is_none() {
        return Err("Start date must be YYYY-MM-DD".into());
    }
    if ev.id.is_empty() {
        ev.id = new_id();
    }
    if ev.created_at.is_empty() {
        ev.created_at = now_rfc3339();
    }
    state.store.lock().unwrap().save_event(&ev).map_err(e)?;
    event_view(&state, ev)
}

#[tauri::command]
pub fn delete_event(state: State<'_, AppState>, id: String) -> R<()> {
    state.store.lock().unwrap().delete_event(&id).map_err(e)
}

#[tauri::command]
pub fn upcoming_events(state: State<'_, AppState>, days: i64) -> R<Vec<Occurrence>> {
    let evs = state.store.lock().unwrap().events().map_err(e)?;
    Ok(events::upcoming(&evs, Local::now().date_naive(), days.clamp(1, 120)))
}

#[tauri::command]
pub fn event_rides(state: State<'_, AppState>, id: String) -> R<Vec<RideSummary>> {
    state.store.lock().unwrap().rides_for_event(&id).map_err(e)
}

// ---------------------------------------------------------------------------
// Rides & stats
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_rides(state: State<'_, AppState>) -> R<Vec<RideSummary>> {
    state.store.lock().unwrap().ride_summaries().map_err(e)
}

#[tauri::command]
pub fn get_ride(state: State<'_, AppState>, id: String) -> R<RideRecord> {
    state.store.lock().unwrap().ride(&id).map_err(e)?.ok_or_else(|| "Ride not found".into())
}

#[tauri::command]
pub fn delete_ride(state: State<'_, AppState>, id: String) -> R<()> {
    state.store.lock().unwrap().delete_ride(&id).map_err(e)
}

#[tauri::command]
pub fn update_ride(state: State<'_, AppState>, id: String, name: String, notes: String) -> R<RideSummary> {
    let store = state.store.lock().unwrap();
    let mut r = store.ride(&id).map_err(e)?.ok_or("Ride not found")?;
    r.summary.name = name;
    r.summary.notes = notes;
    store.update_summary(&r.summary).map_err(e)?;
    Ok(r.summary)
}

#[tauri::command]
pub fn export_ride(state: State<'_, AppState>, id: String, format: String, path: String) -> R<String> {
    let r = state.store.lock().unwrap().ride(&id).map_err(e)?.ok_or("Ride not found")?;
    let body: Vec<u8> = match format.as_str() {
        "csv" => export::to_csv(&r).into_bytes(),
        "fit" => corekick_core::fit::to_fit(&r),
        _ => export::to_tcx(&r).into_bytes(),
    };
    std::fs::write(&path, body).map_err(e)?;
    Ok(path)
}

#[tauri::command]
pub fn account_stats(state: State<'_, AppState>) -> R<AccountStats> {
    let p = profile(&state);
    let store = state.store.lock().unwrap();
    let rides = store.ride_summaries().map_err(e)?;
    let ftp = store.ftp_history().map_err(e)?;
    Ok(stats::compute(&rides, &p, ftp, Local::now().date_naive()))
}

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DeviceStatus {
    #[serde(flatten)]
    pub ble: BleStatus,
    pub simulator: bool,
}

#[tauri::command]
pub fn ble_status(state: State<'_, AppState>) -> DeviceStatus {
    DeviceStatus { ble: state.ble.status(), simulator: state.use_sim.load(Ordering::SeqCst) }
}

#[tauri::command]
pub fn ble_devices(state: State<'_, AppState>) -> Vec<DeviceInfo> {
    state.ble.devices()
}

#[tauri::command]
pub async fn ble_scan(state: State<'_, AppState>, seconds: Option<u64>) -> R<()> {
    let ble = state.ble.clone();
    ble.start_scan(seconds.unwrap_or(20)).await.map_err(e)
}

#[tauri::command]
pub fn ble_stop_scan(state: State<'_, AppState>) {
    state.ble.stop_scan();
}

#[tauri::command]
pub fn ble_connect(state: State<'_, AppState>, id: String, role: String) -> R<()> {
    match role.as_str() {
        "trainer" => {
            state.use_sim.store(false, Ordering::SeqCst);
            state.ble.connect_trainer(id)
        }
        "hrm" => state.ble.connect_hrm(id),
        _ => return Err("role must be trainer or hrm".into()),
    }
    Ok(())
}

#[tauri::command]
pub fn ble_disconnect(state: State<'_, AppState>, role: String) {
    match role.as_str() {
        "hrm" => state.ble.disconnect_hrm(),
        _ => state.ble.disconnect_trainer(),
    }
}

#[derive(Serialize, Deserialize)]
pub struct SimSettings {
    pub enabled: bool,
    pub effort_w: f64,
}

#[tauri::command]
pub fn sim_get(state: State<'_, AppState>) -> SimSettings {
    SimSettings { enabled: state.use_sim.load(Ordering::SeqCst), effort_w: state.sim.lock().unwrap().effort_w }
}

#[tauri::command]
pub fn sim_set(state: State<'_, AppState>, settings: SimSettings) -> SimSettings {
    if settings.enabled && !state.use_sim.load(Ordering::SeqCst) {
        state.ble.disconnect_trainer();
    }
    state.use_sim.store(settings.enabled, Ordering::SeqCst);
    state.sim.lock().unwrap().effort_w = settings.effort_w.clamp(0.0, 1500.0);
    sim_get(state)
}

// ---------------------------------------------------------------------------
// Riding
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct StartRequest {
    pub kind: RideKind,
    pub route_id: Option<String>,
    pub laps: Option<u32>,
    pub workout_id: Option<String>,
    pub test: Option<TestKind>,
    pub event_id: Option<String>,
    pub ghost: Option<bool>,
    pub name: Option<String>,
    pub free: Option<FreeOptions>,
    pub goal: Option<Goal>,
}

#[tauri::command]
pub fn ride_start(app: AppHandle, state: State<'_, AppState>, req: StartRequest) -> R<RideInfo> {
    if state.ride.lock().unwrap().is_some() {
        return Err("A ride is already in progress".into());
    }
    let mut req = req;
    // An event fills in whatever the request didn't specify.
    let mut event_name = None;
    if let Some(eid) = &req.event_id {
        let ev = state.store.lock().unwrap().event(eid).map_err(e)?.ok_or("Event not found")?;
        event_name = Some(ev.name.clone());
        match ev.target {
            EventTarget::Route { route_id, laps } => {
                req.kind = RideKind::Route;
                req.route_id = req.route_id.or(Some(route_id));
                req.laps = req.laps.or(Some(laps));
                req.ghost = req.ghost.or(Some(true));
            }
            EventTarget::Workout { workout_id, route_id } => {
                req.kind = RideKind::Workout;
                req.workout_id = req.workout_id.or(Some(workout_id));
                req.route_id = req.route_id.or(route_id);
            }
            EventTarget::Test { test } => {
                req.kind = RideKind::Test;
                req.test = req.test.or(Some(test));
            }
        }
    }

    let p = profile(&state);
    let mut route = match &req.route_id {
        Some(id) => Some(state.store.lock().unwrap().route(id).map_err(e)?.ok_or("Route not found")?),
        None => None,
    };
    // Free rides can roll over endless generated terrain.
    if req.kind == RideKind::Free && route.is_none() {
        if let Some(t) = req.free.as_ref().and_then(|f| f.terrain) {
            let seed = req.free.as_ref().map(|f| f.seed).filter(|s| *s != 0).unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);
            route = Some(route::endless_route(t, seed));
        }
    }
    let workout = match (&req.kind, &req.workout_id) {
        (RideKind::Workout, Some(id)) => Some(find_workout(&state, id)?),
        (RideKind::Workout, None) => return Err("Choose a workout".into()),
        _ => None,
    };
    if req.kind == RideKind::Route && route.is_none() {
        return Err("Choose a route".into());
    }
    if req.kind == RideKind::Test && req.test.is_none() {
        return Err("Choose a test".into());
    }
    let laps = req.laps.unwrap_or(1).max(1);
    let ghost = if req.ghost.unwrap_or(true) && req.kind == RideKind::Route {
        route
            .as_ref()
            .and_then(|r| state.store.lock().unwrap().best_route_ride(&r.id, laps).ok().flatten())
            .map(|rec| GhostTrace::from_record(&rec))
    } else {
        None
    };
    let name = req.name.clone().filter(|n| !n.trim().is_empty()).or(event_name).unwrap_or_else(|| match req.kind {
        RideKind::Route => route.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        RideKind::Workout => workout.as_ref().map(|w| w.name.clone()).unwrap_or_default(),
        RideKind::Test => req
            .test
            .and_then(|t| fitness_test::test_catalogue().into_iter().find(|i| i.kind == t))
            .map(|i| i.name)
            .unwrap_or_default(),
        RideKind::Free => match route.as_ref() {
            Some(r) if r.source == route::RouteSource::Generated && r.name.starts_with("Endless") => r.name.clone(),
            Some(r) => format!("Free ride · {}", r.name),
            None => "Free ride".into(),
        },
    });
    let setup = RideSetup {
        kind: req.kind,
        name,
        route,
        laps,
        workout,
        test: if req.kind == RideKind::Test { req.test } else { None },
        event_id: req.event_id.clone(),
        ghost,
        free: req.free.clone(),
        goal: req.goal,
    };
    let engine = RideEngine::new(p, setup, now_rfc3339());
    let info = engine.info();
    let engine = Arc::new(Mutex::new(engine));
    let stop = Arc::new(AtomicBool::new(false));
    *state.ride.lock().unwrap() = Some(ActiveRide { id: new_id(), engine: engine.clone(), stop: stop.clone(), info: info.clone() });
    spawn_ride_loop(app, engine, stop);
    Ok(info)
}

/// Current plan: steps, (possibly edited) route and sectors.
#[tauri::command]
pub fn ride_info(state: State<'_, AppState>) -> Option<RideInfo> {
    state.ride.lock().unwrap().as_ref().map(|r| r.engine.lock().unwrap().info())
}

#[tauri::command]
pub fn ride_pause(state: State<'_, AppState>) -> R<()> {
    let g = state.ride.lock().unwrap();
    let r = g.as_ref().ok_or("No active ride")?;
    r.engine.lock().unwrap().pause();
    Ok(())
}

#[tauri::command]
pub fn ride_resume(state: State<'_, AppState>) -> R<()> {
    let g = state.ride.lock().unwrap();
    let r = g.as_ref().ok_or("No active ride")?;
    r.engine.lock().unwrap().resume();
    Ok(())
}

#[tauri::command]
pub fn ride_action(state: State<'_, AppState>, action: RideAction) -> R<()> {
    let g = state.ride.lock().unwrap();
    let r = g.as_ref().ok_or("No active ride")?;
    r.engine.lock().unwrap().apply(action);
    Ok(())
}

// ---------------------------------------------------------------------------
// Automatic FTP assessment
// ---------------------------------------------------------------------------

use corekick_core::ftp_estimate::FtpAssessment;

#[tauri::command]
pub fn ftp_assessment(state: State<'_, AppState>) -> Option<FtpAssessment> {
    state.store.lock().unwrap().meta("ftp_assessment").ok().flatten().and_then(|j| serde_json::from_str(&j).ok())
}

#[tauri::command]
pub fn ftp_reassess(app: AppHandle) -> Option<FtpAssessment> {
    crate::reassess_ftp(&app, None)
}

#[tauri::command]
pub fn ftp_apply_suggestion(app: AppHandle, state: State<'_, AppState>) -> R<Profile> {
    let a = ftp_assessment(state.clone()).ok_or("No FTP assessment yet")?;
    let ftp = a.suggested_ftp.ok_or("No FTP change is suggested")?;
    let p = set_ftp(state, ftp, "Auto (eFTP)".into())?;
    crate::reassess_ftp(&app, a.trigger_ride_id);
    Ok(p)
}

#[tauri::command]
pub fn ftp_dismiss(state: State<'_, AppState>) -> R<()> {
    let store = state.store.lock().unwrap();
    let mut a: FtpAssessment = store.meta("ftp_assessment").ok().flatten().and_then(|j| serde_json::from_str(&j).ok()).ok_or("No FTP assessment")?;
    a.dismissed = true;
    store.set_meta("ftp_assessment", &serde_json::to_string(&a).map_err(e)?).map_err(e)
}

// ---------------------------------------------------------------------------
// Heart-rate zones & Strava
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn hr_zones(state: State<'_, AppState>) -> Vec<HrZone> {
    profile(&state).hr_zones()
}

#[tauri::command]
pub fn strava_status(app: AppHandle) -> StravaStatus {
    strava::status(&app)
}

#[tauri::command]
pub fn strava_save_client(app: AppHandle, client_id: String, client_secret: String) -> R<StravaStatus> {
    strava::save_client(&app, &client_id, &client_secret).map_err(e)?;
    Ok(strava::status(&app))
}

#[tauri::command]
pub async fn strava_connect(app: AppHandle) -> R<StravaStatus> {
    strava::connect(app).await.map_err(e)
}

#[tauri::command]
pub async fn strava_disconnect(app: AppHandle) -> R<StravaStatus> {
    strava::disconnect(app.clone()).await.map_err(e)?;
    Ok(strava::status(&app))
}

#[tauri::command]
pub fn strava_set_auto(app: AppHandle, on: bool) -> R<StravaStatus> {
    strava::set_auto(&app, on).map_err(e)?;
    Ok(strava::status(&app))
}

#[tauri::command]
pub async fn strava_upload(app: AppHandle, id: String) -> R<u64> {
    strava::upload(app, id).await.map_err(e)
}

/// Open an https link (Strava activity, Strava settings) in the default browser.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> R<()> {
    if !(url.starts_with("https://www.strava.com/") || url.starts_with("https://strava.com/")) {
        return Err("Only Strava links can be opened".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(e)
}

#[tauri::command]
pub fn ride_end(app: AppHandle, state: State<'_, AppState>, save: bool, save_route: Option<bool>) -> R<Option<RideSummary>> {
    let active = state.ride.lock().unwrap().take().ok_or("No active ride")?;
    active.stop.store(true, Ordering::SeqCst);
    let (mut record, edited_route) = {
        let mut eng = active.engine.lock().unwrap();
        let edited = if eng.route_modified() { eng.current_route().cloned() } else { None };
        (eng.finish(now_rfc3339(), active.id.clone()), edited)
    };
    // An edited route is a different course: keep it off the original's leaderboard,
    // optionally saving it as a new route the ride belongs to.
    if let Some(mut r) = edited_route {
        if save && save_route.unwrap_or(false) && r.name != "Virtual road" {
            r.id = new_id();
            let base = match r.name.strip_prefix("Endless ") {
                Some(t) if !t.is_empty() => format!("{}{} loop", t[..1].to_uppercase(), &t[1..]),
                _ => r.name.clone(),
            };
            r.name = format!("{base} (edited)");
            r.source = route::RouteSource::Custom;
            r.gen_params = None;
            r.segments = None;
            r.created_at = now_rfc3339();
            r.favourite = false;
            state.store.lock().unwrap().save_route(&r).map_err(e)?;
            record.summary.route_id = Some(r.id.clone());
            record.summary.route_name = Some(r.name.clone());
        } else {
            record.summary.route_id = None;
            record.summary.route_time_s = None;
        }
    }
    // Leave the trainer in an easy, neutral state.
    if state.use_sim.load(Ordering::SeqCst) {
        state.sim.lock().unwrap().command(corekick_core::engine::TrainerCommand::Simulation { grade_pct: 0.0, wind_mps: 0.0, crr: 0.004, cw: 0.51 });
    } else {
        state.ble.send(corekick_core::engine::TrainerCommand::Simulation { grade_pct: 0.0, wind_mps: 0.0, crr: 0.004, cw: 0.51 });
    }
    if !save || record.samples.len() < 10 {
        return Ok(None);
    }
    record.summary.simulated = state.use_sim.load(Ordering::SeqCst);
    let store = state.store.lock().unwrap();
    if let (Some(rid), Some(t)) = (&record.summary.route_id, record.summary.route_time_s) {
        let prev = store.best_route_ride(rid, record.summary.laps).map_err(e)?;
        record.summary.personal_best = prev.and_then(|p| p.summary.route_time_s).map(|pt| t < pt).unwrap_or(true);
    }
    store.save_ride(&record).map_err(e)?;
    drop(store);
    // Re-assess FTP with this ride included.
    crate::reassess_ftp(&app, Some(record.summary.id.clone()));
    // Auto-upload to Strava in the background.
    let st = strava::status(&app);
    if st.connected && st.auto_upload && record.summary.moving_s >= 60.0 {
        let id = record.summary.id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = strava::upload(app, id).await;
        });
    }
    Ok(Some(record.summary))
}
