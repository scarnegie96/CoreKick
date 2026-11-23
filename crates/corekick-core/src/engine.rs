//! Real-time ride engine.
//!
//! The engine is pure: feed it wall-clock timestamps and trainer telemetry via
//! [`RideEngine::tick`] (≈4 Hz) and it returns a [`Snapshot`] for the UI plus,
//! when needed, a [`TrainerCommand`] to send to the trainer. It owns:
//! * virtual speed / distance / elevation via [`crate::physics`]
//! * SIM gradient (with lookahead, smoothing, trainer difficulty and clamps)
//! * ERG targets from structured workouts and tests, editable mid-ride
//! * free-ride modes: gradient, power (ERG), resistance level, heart-rate
//! * goals, manual laps, route laps and timed sectors (splits)
//! * in-ride route editing: splice new sectors into the road ahead
//! * 1 Hz recording, energy (kJ) and calorie integration
//! * ghost racing, ramp-test failure detection, virtual shifting

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::fitness_test::{self, TestKind, TestResult};
use crate::gears;
use crate::metrics;
use crate::model::{hr_zone_of, GhostTrace, HrZone, Profile, RideKind, RideRecord, RideSamples, RideSummary, Split};
use crate::physics::{integrate_speed, PhysicsParams};
use crate::route::{insert_segments, rejoin_segment, segments_summary, Climb, Route, RoutePoint, RouteSource, RouteStats, SectorKind, SectorSpec};
use crate::workout::{blocks_to_steps, reflow, step_at, Block, Step, StepTarget, Workout};

// ---------------------------------------------------------------------------
// Setup & options
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FreeMode {
    /// SIM: ride a gradient (manual, or the terrain's + an offset)
    #[default]
    Gradient,
    /// ERG: hold fixed watts
    Power,
    /// FTMS resistance level (like a classic trainer)
    Level,
    /// ERG adjusted automatically to hold a heart rate
    HeartRate,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FreeOptions {
    pub mode: FreeMode,
    pub grade_pct: f64,
    pub watts: f64,
    pub level_pct: f64,
    pub target_hr: f64,
    /// Endless generated terrain (the command layer builds the route).
    pub terrain: Option<crate::route::Terrain>,
    pub seed: u64,
}

impl Default for FreeOptions {
    fn default() -> Self {
        FreeOptions { mode: FreeMode::Gradient, grade_pct: 0.0, watts: 0.0, level_pct: 30.0, target_hr: 0.0, terrain: None, seed: 0 }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Goal {
    Time { seconds: f64 },
    Distance { meters: f64 },
    Energy { kj: f64 },
    Calories { kcal: f64 },
    Elevation { meters: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RideSetup {
    pub kind: RideKind,
    pub name: String,
    #[serde(default)]
    pub route: Option<Route>,
    #[serde(default = "one")]
    pub laps: u32,
    #[serde(default)]
    pub workout: Option<Workout>,
    #[serde(default)]
    pub test: Option<TestKind>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub ghost: Option<GhostTrace>,
    #[serde(default)]
    pub free: Option<FreeOptions>,
    #[serde(default)]
    pub goal: Option<Goal>,
}

fn one() -> u32 {
    1
}

impl RideSetup {
    pub fn simple(kind: RideKind, name: &str) -> Self {
        RideSetup { kind, name: name.into(), route: None, laps: 1, workout: None, test: None, event_id: None, ghost: None, free: None, goal: None }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrainerCommand {
    Simulation { grade_pct: f64, wind_mps: f64, crr: f64, cw: f64 },
    TargetPower { watts: f64 },
    /// Percent of the trainer's supported resistance range.
    Resistance { level_pct: f64 },
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TrainerInput {
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub heart_rate: Option<f64>,
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
    Sim,
    Erg,
    Level,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RideAction {
    /// Scale all ERG targets (workouts/tests), percent points.
    ErgBias { delta_pct: f64 },
    SkipStep,
    /// Switch a workout between ERG and resistance (SIM) mode.
    ToggleErg,
    /// Free ride: change the simulated gradient (or the terrain offset).
    Grade { delta_pct: f64 },
    /// Free ride: set (or clear) a manual ERG target. Kept for compatibility.
    FreeErg { watts: Option<f64> },
    /// Route rides: change trainer difficulty live (percent points).
    Difficulty { delta_pct: f64 },
    /// Virtual shifting: change gear by ±n.
    Shift { delta: i32 },

    // ---- free ride ----
    SetFreeMode { mode: FreeMode },
    FreeWatts { delta_w: f64 },
    Level { delta_pct: f64 },
    HrTarget { delta_bpm: f64 },
    SetGoal { goal: Option<Goal> },

    // ---- laps / route ----
    Lap,
    AddLaps { delta: i32 },
    AddSector { sector: SectorSpec },
    UndoSector,

    // ---- workout editing ----
    ExtendStep { seconds: i32 },
    AdjustStep { delta_pct: f64 },
    InsertBlocks { blocks: Vec<Block>, at_end: bool },
    RemoveStep { index: usize },
    OneMoreRep,
    SkipToCooldown,
    EndWorkout,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RideEvent {
    pub kind: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StepSnapshot {
    pub index: usize,
    pub label: String,
    pub elapsed_s: f64,
    pub remaining_s: f64,
    pub target_w: Option<f64>,
    pub grade_pct: Option<f64>,
    pub effort: bool,
    pub cadence: Option<u32>,
    pub next_label: Option<String>,
    pub next_target_w: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClimbSnapshot {
    pub index: usize,
    pub category: String,
    pub length_m: f64,
    pub gain_m: f64,
    pub avg_grade_pct: f64,
    pub distance_to_m: f64,
    /// 0..1 when on the climb
    pub progress: f64,
    pub remaining_m: f64,
    pub remaining_gain_m: f64,
}

/// A sector spliced into the route during the ride.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActiveSector {
    pub id: u32,
    pub name: String,
    pub kind: SectorKind,
    /// Absolute ride distance where the sector starts / ends.
    pub start_total: f64,
    pub end_total: f64,
    /// Position on the route (for drawing).
    pub start_d: f64,
    pub end_d: f64,
    pub length_m: f64,
    pub avg_grade_pct: f64,
    pub timed: bool,
    pub started_idx: Option<usize>,
    pub done: bool,
    pub result: Option<Split>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SectorSnapshot {
    pub id: u32,
    pub name: String,
    pub kind: SectorKind,
    pub distance_to_m: f64,
    pub length_m: f64,
    pub avg_grade_pct: f64,
    pub progress: f64,
    pub active: bool,
    pub done: bool,
    pub timed: bool,
    pub elapsed_s: f64,
    pub result: Option<Split>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GoalSnapshot {
    pub goal: Goal,
    pub label: String,
    pub progress: f64,
    pub remaining: String,
    pub done: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub kind: RideKind,
    pub plan_version: u32,
    pub elapsed_s: f64,
    pub moving_s: f64,
    pub paused: bool,
    pub finished: bool,
    pub trainer_connected: bool,

    pub distance_m: f64,
    pub lap: u32,
    pub laps: u32,
    pub lap_distance_m: f64,
    pub route_distance_m: Option<f64>,
    pub route_progress: Option<f64>,
    pub remaining_m: Option<f64>,
    pub route_complete: bool,
    pub route_time_s: Option<f64>,
    pub endless: bool,
    pub route_modified: bool,

    pub ele_m: f64,
    pub grade_pct: f64,
    pub trainer_grade_pct: f64,
    pub mode: ControlMode,
    pub target_w: Option<f64>,
    pub trainer_difficulty: f64,

    pub speed_kmh: f64,
    pub power_w: f64,
    pub power_3s: f64,
    pub cadence_rpm: f64,
    pub heart_rate: Option<f64>,
    /// 1-based heart-rate zone
    pub hr_zone: Option<u32>,
    pub hr_zone_count: u32,
    pub avg_power: f64,
    pub max_power: f64,
    pub np: f64,
    pub work_kj: f64,
    pub calories_kcal: f64,
    pub elevation_gain_m: f64,
    pub wkg_3s: f64,
    pub tss: f64,

    pub structured: bool,
    pub step: Option<StepSnapshot>,
    pub workout_t: f64,
    pub workout_total_s: Option<f64>,
    pub workout_complete: bool,
    pub erg_bias_pct: f64,
    pub erg_disabled: bool,

    pub free_mode: FreeMode,
    pub free_grade_pct: f64,
    pub free_erg_w: Option<f64>,
    pub free_watts: f64,
    pub free_level_pct: f64,
    pub hr_target_bpm: f64,
    pub hr_control_w: f64,

    pub goal: Option<GoalSnapshot>,
    pub split_count: u32,
    pub split_elapsed_s: f64,
    pub split_distance_m: f64,
    pub split_avg_power: f64,
    pub recent_splits: Vec<Split>,
    pub sectors: Vec<SectorSnapshot>,
    pub can_undo_sector: bool,

    pub ghost_gap_s: Option<f64>,
    pub ghost_distance_m: Option<f64>,

    pub current_climb: Option<ClimbSnapshot>,
    pub next_climb: Option<ClimbSnapshot>,

    pub test_status: Option<String>,
    pub test_done: bool,
    pub notice: Option<String>,

    /// Virtual shifting enabled in the profile
    pub virtual_shifting: bool,
    /// …and currently controlling the trainer (SIM-type riding)
    pub shifting_active: bool,
    pub gear: u32,
    pub gear_count: u32,
    pub gear_ratio: f64,

    pub events: Vec<RideEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RideInfo {
    pub kind: RideKind,
    pub name: String,
    pub started_at: String,
    pub route: Option<Route>,
    pub route_stats: Option<RouteStats>,
    pub laps: u32,
    pub steps: Vec<Step>,
    pub ftp: f64,
    pub weight_kg: f64,
    pub workout_name: Option<String>,
    pub test: Option<TestKind>,
    pub ghost: Option<GhostTrace>,
    pub event_id: Option<String>,
    pub free: Option<FreeOptions>,
    pub goal: Option<Goal>,
    pub sectors: Vec<ActiveSector>,
    pub plan_version: u32,
    pub endless: bool,
}

#[derive(Default)]
struct SecondAcc {
    dt: f64,
    p: f64,
    c: f64,
    hr: f64,
    hr_dt: f64,
    v: f64,
    target: f64,
    target_dt: f64,
}

pub struct RideEngine {
    profile: Profile,
    params: PhysicsParams,
    setup: RideSetup,
    steps: Vec<Step>,
    climbs: Vec<Climb>,
    route_stats: Option<RouteStats>,
    started_at: String,
    plan_version: u32,

    last_ms: Option<u64>,
    now_s: f64,
    active_s: f64,
    moving_s: f64,
    offset_s: f64,
    paused: bool,
    finished: bool,

    speed: f64,
    max_speed: f64,
    distance: f64,
    virtual_ele: f64,
    ele: f64,
    gain: f64,
    grade: f64,
    trainer_grade_pct: f64,
    difficulty: f64,

    kj: f64,
    power: f64,
    cadence: f64,
    hr: Option<f64>,
    connected: bool,
    recent: VecDeque<(f64, f64)>,
    acc: SecondAcc,
    samples: RideSamples,
    workout_t_samples: Vec<f64>,
    max_power: f64,

    mode: ControlMode,
    target_w: Option<f64>,
    erg_disabled: bool,
    erg_bias: f64,
    last_cmd: Option<TrainerCommand>,
    last_cmd_at: f64,
    force_cmd: bool,

    free_mode: FreeMode,
    free_grade_pct: f64,
    free_watts: f64,
    free_level_pct: f64,
    hr_target: f64,
    hr_w: f64,
    hr_ctrl_t: f64,

    goal: Option<Goal>,
    goal_done: bool,

    splits: Vec<Split>,
    split_start_idx: usize,
    split_start_s: f64,
    split_start_d: f64,
    route_lap_start_idx: usize,
    sectors: Vec<ActiveSector>,
    next_sector_id: u32,
    route_history: Vec<(Route, Vec<ActiveSector>)>,
    route_modified: bool,

    events: Vec<RideEvent>,
    lap: u32,
    route_complete_at: Option<f64>,
    workout_complete: bool,
    step_idx: Option<usize>,
    announced: HashSet<usize>,
    ramp_fail_s: f64,
    test_done: bool,

    gear_idx: usize,
    cad_ema: f64,
    shifting_active: bool,
    hr_zones: Vec<HrZone>,
}

impl RideEngine {
    pub fn new(profile: Profile, mut setup: RideSetup, started_at: String) -> Self {
        let params = PhysicsParams::from_profile(&profile);
        if let Some(kind) = setup.test {
            setup.workout = Some(fitness_test::protocol(kind, profile.ftp));
        }
        if setup.route.as_ref().map(|r| r.points.len() < 2).unwrap_or(false) {
            setup.route = None;
        }
        setup.laps = setup.laps.clamp(1, 50);
        let steps = setup.workout.as_ref().map(|w| w.steps()).unwrap_or_default();
        let route_stats = setup.route.as_ref().map(|r| r.stats());
        let climbs = route_stats.as_ref().map(|s| s.climbs.clone()).unwrap_or_default();
        let start_ele = setup.route.as_ref().map(|r| r.elevation_at(0.0)).unwrap_or(50.0);
        let difficulty = profile.trainer_difficulty.clamp(0.0, 1.5);
        let free = setup.free.clone().unwrap_or_default();
        let ftp = profile.ftp.max(50.0);
        RideEngine {
            params,
            steps,
            climbs,
            route_stats,
            started_at,
            plan_version: 1,
            last_ms: None,
            now_s: 0.0,
            active_s: 0.0,
            moving_s: 0.0,
            offset_s: 0.0,
            paused: false,
            finished: false,
            speed: 0.0,
            max_speed: 0.0,
            distance: 0.0,
            virtual_ele: start_ele,
            ele: start_ele,
            gain: 0.0,
            grade: 0.0,
            trainer_grade_pct: 0.0,
            difficulty,
            kj: 0.0,
            power: 0.0,
            cadence: 0.0,
            hr: None,
            connected: false,
            recent: VecDeque::new(),
            acc: SecondAcc::default(),
            samples: RideSamples::default(),
            workout_t_samples: Vec::new(),
            max_power: 0.0,
            mode: ControlMode::Sim,
            target_w: None,
            erg_disabled: false,
            erg_bias: 1.0,
            last_cmd: None,
            last_cmd_at: f64::NEG_INFINITY,
            force_cmd: true,
            free_mode: free.mode,
            free_grade_pct: free.grade_pct.clamp(profile.min_trainer_grade, profile.max_trainer_grade),
            free_watts: if free.watts > 0.0 { free.watts.clamp(30.0, 1500.0) } else { (ftp * 0.65).round() },
            free_level_pct: free.level_pct.clamp(0.0, 100.0),
            hr_target: if free.target_hr > 0.0 { free.target_hr } else { (profile.rest_hr as f64 + 0.65 * (profile.max_hr as f64 - profile.rest_hr as f64)).round() },
            hr_w: (ftp * 0.55).round(),
            hr_ctrl_t: 0.0,
            goal: setup.goal,
            goal_done: false,
            splits: Vec::new(),
            split_start_idx: 0,
            split_start_s: 0.0,
            split_start_d: 0.0,
            route_lap_start_idx: 0,
            sectors: Vec::new(),
            next_sector_id: 1,
            route_history: Vec::new(),
            route_modified: false,
            events: Vec::new(),
            lap: 1,
            route_complete_at: None,
            workout_complete: false,
            step_idx: None,
            announced: HashSet::new(),
            ramp_fail_s: 0.0,
            test_done: false,
            gear_idx: gears::index_from_gear(profile.start_gear),
            cad_ema: 0.0,
            shifting_active: false,
            hr_zones: profile.hr_zones(),
            profile,
            setup,
        }
    }

    pub fn info(&self) -> RideInfo {
        RideInfo {
            kind: self.setup.kind,
            name: self.setup.name.clone(),
            started_at: self.started_at.clone(),
            route: self.setup.route.clone(),
            route_stats: self.route_stats.clone(),
            laps: self.setup.laps,
            steps: self.steps.clone(),
            ftp: self.profile.ftp,
            weight_kg: self.profile.weight_kg,
            workout_name: self.setup.workout.as_ref().map(|w| w.name.clone()),
            test: self.setup.test,
            ghost: self.setup.ghost.clone(),
            event_id: self.setup.event_id.clone(),
            free: self.setup.free.clone(),
            goal: self.goal,
            sectors: self.sectors.clone(),
            plan_version: self.plan_version,
            endless: self.endless(),
        }
    }

    pub fn setup(&self) -> &RideSetup {
        &self.setup
    }

    pub fn route_modified(&self) -> bool {
        self.route_modified
    }

    pub fn current_route(&self) -> Option<&Route> {
        self.setup.route.as_ref()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Only route rides have a finish; everything else loops its scenery forever.
    fn endless(&self) -> bool {
        self.setup.kind != RideKind::Route
    }

    fn structured_active(&self) -> bool {
        !self.steps.is_empty() && !self.workout_complete
    }

    pub fn pause(&mut self) {
        if !self.paused {
            self.paused = true;
            self.force_cmd = true;
            self.push_event("pause", "Ride paused");
        }
    }

    pub fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.force_cmd = true;
            self.push_event("resume", "Ride resumed");
        }
    }

    fn bump_plan(&mut self) {
        self.plan_version += 1;
        self.force_cmd = true;
    }

    fn current_step(&self) -> Option<(usize, f64)> {
        if !self.structured_active() {
            return None;
        }
        step_at(&self.steps, self.workout_t())
    }

    pub fn apply(&mut self, action: RideAction) {
        let ftp = self.profile.ftp.max(50.0);
        match action {
            RideAction::ErgBias { delta_pct } => {
                self.erg_bias = (self.erg_bias + delta_pct / 100.0).clamp(0.5, 1.5);
                self.force_cmd = true;
                self.push_event("bias", &format!("Intensity {:.0}%", self.erg_bias * 100.0));
            }
            RideAction::SkipStep => {
                if let Some((i, _)) = self.current_step() {
                    self.offset_s = self.steps[i].end_s() as f64 - self.active_s;
                    self.force_cmd = true;
                    self.push_event("skip", &format!("Skipped “{}”", self.steps[i].label));
                }
            }
            RideAction::ToggleErg => {
                self.erg_disabled = !self.erg_disabled;
                self.force_cmd = true;
                let m = if self.erg_disabled { "Resistance mode – you control power" } else { "ERG mode on" };
                self.push_event("mode", m);
            }
            RideAction::Grade { delta_pct } => {
                self.free_grade_pct = (self.free_grade_pct + delta_pct).clamp(self.profile.min_trainer_grade, self.profile.max_trainer_grade);
                self.free_grade_pct = (self.free_grade_pct * 10.0).round() / 10.0;
            }
            RideAction::FreeErg { watts } => {
                match watts {
                    Some(w) => {
                        self.free_watts = w.clamp(30.0, 1500.0);
                        self.free_mode = FreeMode::Power;
                    }
                    None => self.free_mode = FreeMode::Gradient,
                }
                self.force_cmd = true;
            }
            RideAction::Shift { delta } => {
                let n = gears::gear_count() as i32;
                self.gear_idx = (self.gear_idx as i32 + delta).clamp(0, n - 1) as usize;
            }
            RideAction::Difficulty { delta_pct } => {
                self.difficulty = (self.difficulty + delta_pct / 100.0).clamp(0.0, 1.5);
                self.push_event("difficulty", &format!("Trainer difficulty {:.0}%", self.difficulty * 100.0));
            }
            RideAction::SetFreeMode { mode } => {
                if mode == FreeMode::HeartRate {
                    // Start the controller from what the rider is doing now.
                    let p3 = self.power_3s();
                    self.hr_w = if p3 > 30.0 { p3.round() } else { (ftp * 0.55).round() };
                    self.hr_ctrl_t = 0.0;
                }
                if mode == FreeMode::Power && self.power_3s() > 30.0 && self.free_mode != FreeMode::Power {
                    self.free_watts = (self.power_3s() / 5.0).round() * 5.0;
                }
                self.free_mode = mode;
                self.force_cmd = true;
                let m = match mode {
                    FreeMode::Gradient => "Gradient mode",
                    FreeMode::Power => "Power (ERG) mode",
                    FreeMode::Level => "Resistance level mode",
                    FreeMode::HeartRate => "Heart-rate mode",
                };
                self.push_event("mode", m);
            }
            RideAction::FreeWatts { delta_w } => {
                self.free_watts = (self.free_watts + delta_w).clamp(30.0, 1500.0);
            }
            RideAction::Level { delta_pct } => {
                self.free_level_pct = (self.free_level_pct + delta_pct).clamp(0.0, 100.0);
            }
            RideAction::HrTarget { delta_bpm } => {
                self.hr_target = (self.hr_target + delta_bpm).clamp(80.0, 200.0);
            }
            RideAction::SetGoal { goal } => {
                self.goal = goal;
                self.goal_done = false;
                self.plan_version += 1;
            }
            RideAction::Lap => self.close_manual_lap(),
            RideAction::AddLaps { delta } => {
                if self.setup.kind == RideKind::Route {
                    let min = self.lap;
                    let n = (self.setup.laps as i32 + delta).clamp(min as i32, 50) as u32;
                    self.setup.laps = n;
                    if let Some(len) = self.route_len() {
                        if self.route_complete_at.is_some() && self.distance < len * n as f64 {
                            self.route_complete_at = None;
                        }
                    }
                    self.push_event("laps", &format!("Riding {n} lap{}", if n == 1 { "" } else { "s" }));
                    self.plan_version += 1;
                }
            }
            RideAction::AddSector { sector } => self.add_sector(sector),
            RideAction::UndoSector => {
                let ok = self.sectors.last().map(|s| s.started_idx.is_none() && !s.done && self.distance < s.start_total).unwrap_or(false);
                if ok {
                    if let Some((r, secs)) = self.route_history.pop() {
                        self.set_route(r);
                        self.sectors = secs;
                        self.route_modified = !self.route_history.is_empty() || self.sectors.iter().any(|s| s.started_idx.is_some());
                        self.push_event("sector", "Removed the last sector");
                        self.bump_plan();
                    }
                } else {
                    self.push_event("sector", "Can't undo – you've already reached that sector");
                }
            }
            RideAction::ExtendStep { seconds } => {
                if let Some((i, into)) = self.current_step() {
                    let min = (into.ceil() as u32 + 5).max(1);
                    let d = (self.steps[i].duration_s as i64 + seconds as i64).max(min as i64) as u32;
                    self.steps[i].duration_s = d;
                    reflow(&mut self.steps, i + 1);
                    self.bump_plan();
                }
            }
            RideAction::AdjustStep { delta_pct } => {
                if let Some((i, _)) = self.current_step() {
                    if let StepTarget::Erg { from, to } = self.steps[i].target {
                        let d = delta_pct / 100.0;
                        self.steps[i].target = StepTarget::Erg { from: (from + d).clamp(0.2, 4.0), to: (to + d).clamp(0.2, 4.0) };
                        self.bump_plan();
                    }
                }
            }
            RideAction::InsertBlocks { blocks, at_end } => {
                let mut new = blocks_to_steps(&blocks);
                if new.is_empty() {
                    return;
                }
                let n = new.len();
                if self.steps.is_empty() {
                    // Turn any ride into a structured one starting now.
                    self.offset_s = -self.active_s;
                    self.steps = new;
                    self.workout_complete = false;
                    self.step_idx = None;
                } else if self.workout_complete || self.current_step().is_none() {
                    let end = self.steps.last().map(|s| s.end_s()).unwrap_or(0);
                    for s in new.iter_mut() {
                        s.start_s += end;
                    }
                    self.offset_s = end as f64 - self.active_s;
                    self.steps.extend(new);
                    self.workout_complete = false;
                    self.step_idx = None;
                } else {
                    let (i, _) = self.current_step().unwrap();
                    let pos = if at_end { self.steps.len() } else { i + 1 };
                    self.steps.splice(pos..pos, new);
                    reflow(&mut self.steps, pos);
                }
                self.push_event("plan", &format!("Added {n} step{}", if n == 1 { "" } else { "s" }));
                self.bump_plan();
            }
            RideAction::RemoveStep { index } => {
                if let Some((i, _)) = self.current_step() {
                    if index > i && index < self.steps.len() {
                        let s = self.steps.remove(index);
                        reflow(&mut self.steps, index);
                        self.push_event("plan", &format!("Removed “{}”", s.label));
                        self.bump_plan();
                    }
                }
            }
            RideAction::OneMoreRep => {
                if let Some((i, _)) = self.current_step() {
                    if let Some(e) = (0..=i).rev().find(|&k| self.steps[k].effort) {
                        let mut add = vec![self.steps[e].clone()];
                        if e + 1 < self.steps.len() && !self.steps[e + 1].effort {
                            add.push(self.steps[e + 1].clone());
                        }
                        for s in add.iter_mut() {
                            s.label = if s.effort { "Extra rep".into() } else { "Recover".into() };
                        }
                        let mut pos = i + 1;
                        if self.steps[i].effort && pos < self.steps.len() && !self.steps[pos].effort {
                            pos += 1;
                        }
                        self.steps.splice(pos..pos, add);
                        reflow(&mut self.steps, pos);
                        self.push_event("plan", "One more rep added 💪");
                        self.bump_plan();
                    } else {
                        self.push_event("plan", "No interval to repeat yet");
                    }
                }
            }
            RideAction::SkipToCooldown => {
                if let Some((i, _)) = self.current_step() {
                    self.steps.truncate(i + 1);
                    let start = self.steps[i].end_s();
                    self.steps.push(Step {
                        start_s: start,
                        duration_s: 300,
                        target: StepTarget::Erg { from: 0.6, to: 0.4 },
                        label: "Cool-down".into(),
                        effort: false,
                        cadence: None,
                    });
                    self.offset_s = start as f64 - self.active_s;
                    self.push_event("plan", "Cooling down");
                    self.bump_plan();
                }
            }
            RideAction::EndWorkout => {
                if let Some((i, into)) = self.current_step() {
                    self.steps.truncate(i + 1);
                    self.steps[i].duration_s = (into.ceil() as u32).max(1);
                    self.offset_s = self.steps[i].end_s() as f64 - self.active_s;
                    self.bump_plan();
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // Route editing
    // ---------------------------------------------------------------------

    fn set_route(&mut self, r: Route) {
        let st = r.stats();
        self.climbs = st.climbs.clone();
        self.route_stats = Some(st);
        self.setup.route = Some(r);
        self.announced.clear();
    }

    fn add_sector(&mut self, spec: SectorSpec) {
        // Rides without scenery get a flat virtual road to splice into.
        if self.setup.route.is_none() {
            let len = (self.distance + 200_000.0).max(200_000.0);
            let e = self.ele;
            self.setup.route = Some(Route {
                id: crate::new_id(),
                name: "Virtual road".into(),
                description: String::new(),
                source: RouteSource::Custom,
                created_at: crate::now_rfc3339(),
                points: vec![RoutePoint { d: 0.0, ele: e }, RoutePoint { d: len, ele: e }],
                is_loop: false,
                tags: vec![],
                gen_params: None,
                segments: None,
                favourite: false,
            });
        }
        let len = self.route_len().unwrap_or(0.0);
        let lap_d = self.lap_distance();
        if self.route_complete_at.is_some() {
            self.push_event("sector", "The route is finished – add a lap first");
            return;
        }
        let gap = spec.in_m.max(30.0 + self.speed * 4.0);
        let at = if spec.at_end || lap_d + gap >= len { len } else { lap_d + gap };
        let mut segs = spec.segments(self.next_sector_id as u64 * 7919 + self.distance as u64);
        if segs.is_empty() {
            self.push_event("sector", "That sector has no length");
            return;
        }
        let (sec_len, net) = segments_summary(&segs);
        if spec.rejoin {
            segs.extend(rejoin_segment(net));
        }
        let before = (self.setup.route.clone().unwrap(), self.sectors.clone());
        let route = self.setup.route.as_mut().unwrap();
        let (ins, _) = insert_segments(route, at, &segs);
        // Shift sectors that lie beyond the insertion point.
        let start_total = self.distance + (at - lap_d);
        for s in self.sectors.iter_mut() {
            if s.start_d >= at && s.started_idx.is_none() {
                s.start_d += ins;
                s.end_d += ins;
                s.start_total += ins;
                s.end_total += ins;
            }
        }
        let name = spec.display_name();
        self.sectors.push(ActiveSector {
            id: self.next_sector_id,
            name: name.clone(),
            kind: spec.kind,
            start_total,
            end_total: start_total + sec_len,
            start_d: at,
            end_d: at + sec_len,
            length_m: sec_len,
            avg_grade_pct: if sec_len > 0.0 { net / sec_len * 100.0 } else { 0.0 },
            timed: spec.timed,
            started_idx: None,
            done: false,
            result: None,
        });
        self.next_sector_id += 1;
        self.route_history.push(before);
        let r = self.setup.route.clone().unwrap();
        self.set_route(r);
        self.route_modified = true;
        if self.setup.ghost.take().is_some() {
            self.push_event("ghost", "Ghost removed – the route has changed");
        }
        let where_ = if at >= len - 1.0 { "at the end of the route".to_string() } else { format!("in {:.0} m", at - lap_d) };
        self.push_event("sector", &format!("{name} added {where_}"));
        self.bump_plan();
    }

    fn close_manual_lap(&mut self) {
        let n = self.splits.iter().filter(|s| s.kind == "lap").count() as u32 + 1;
        if let Some(sp) = Split::from_samples(&self.samples, self.split_start_idx, self.samples.len(), "lap", format!("Lap {n}"), n) {
            self.push_event("lap", &format!("Lap {n}: {} · {:.0} W", fmt_dur(sp.duration_s), sp.avg_power));
            self.splits.push(sp);
        }
        self.split_start_idx = self.samples.len();
        self.split_start_s = self.active_s;
        self.split_start_d = self.distance;
    }

    fn push_event(&mut self, kind: &str, msg: &str) {
        self.events.push(RideEvent { kind: kind.into(), message: msg.into() });
    }

    fn workout_t(&self) -> f64 {
        self.active_s + self.offset_s
    }

    fn route_len(&self) -> Option<f64> {
        self.setup.route.as_ref().map(|r| r.distance()).filter(|d| *d > 0.0)
    }

    fn lap_distance(&self) -> f64 {
        match self.route_len() {
            Some(len) => {
                if self.route_complete_at.is_some() {
                    len
                } else {
                    self.distance % len
                }
            }
            None => self.distance,
        }
    }

    fn power_3s(&self) -> f64 {
        if self.recent.is_empty() {
            self.power
        } else {
            self.recent.iter().map(|x| x.1).sum::<f64>() / self.recent.len() as f64
        }
    }

    /// Advance the simulation. `now_ms` is a monotonic wall clock.
    pub fn tick(&mut self, now_ms: u64, input: TrainerInput) -> (Snapshot, Option<TrainerCommand>) {
        let dt = match self.last_ms {
            Some(l) => (now_ms.saturating_sub(l) as f64 / 1000.0).clamp(0.0, 2.0),
            None => 0.0,
        };
        self.last_ms = Some(now_ms);
        self.now_s = now_ms as f64 / 1000.0;
        self.connected = input.connected;
        self.power = if input.connected { input.power_w.max(0.0) } else { 0.0 };
        self.cadence = if input.connected { input.cadence_rpm.max(0.0) } else { 0.0 };
        self.hr = input.heart_rate.filter(|h| *h > 0.0);
        // Smoothed cadence for virtual shifting (≈0.6 s time constant).
        self.cad_ema += (self.cadence - self.cad_ema) * (dt / 0.6).min(1.0);
        if dt == 0.0 && self.cad_ema == 0.0 {
            self.cad_ema = self.cadence;
        }

        if !self.paused && !self.finished && dt > 0.0 {
            self.step_ride(dt);
        }
        let cmd = self.control(dt);
        (self.snapshot(), cmd)
    }

    fn free_gradient_active(&self) -> bool {
        self.setup.kind == RideKind::Free && !self.structured_active() && self.free_mode == FreeMode::Gradient
    }

    fn step_ride(&mut self, dt: f64) {
        self.active_s += dt;
        self.kj += self.power * dt / 1000.0;
        self.max_power = self.max_power.max(self.power);
        self.recent.push_back((self.active_s, self.power));
        while let Some(&(t, _)) = self.recent.front() {
            if self.active_s - t > 3.0 {
                self.recent.pop_front();
            } else {
                break;
            }
        }

        // ---- physics on the *real* gradient (not difficulty-scaled) ----
        let lap_d = self.lap_distance();
        let route_grade = self.setup.route.as_ref().map(|r| r.grade_at(lap_d, 10.0));
        let offset = if self.free_gradient_active() { self.free_grade_pct / 100.0 } else { 0.0 };
        let phys_grade = match (route_grade, self.route_complete_at) {
            (Some(g), None) => g + offset,
            (Some(_), Some(_)) => 0.0,
            (None, _) => {
                if self.mode == ControlMode::Sim || self.shifting_active {
                    self.trainer_grade_pct / 100.0
                } else {
                    0.0
                }
            }
        };
        self.grade = phys_grade;
        self.speed = integrate_speed(self.speed, self.power, phys_grade, self.ele, &self.params, dt);
        self.max_speed = self.max_speed.max(self.speed);
        if self.speed > 0.3 || self.power > 0.0 {
            self.moving_s += dt;
        }
        let ds = self.speed * dt;
        let prev_lap = self.lap;
        self.distance += ds;

        // ---- laps / completion ----
        if let Some(len) = self.route_len() {
            if self.endless() {
                let lap_now = (self.distance / len).floor() as u32 + 1;
                if lap_now != self.lap {
                    self.lap = lap_now;
                    self.announced.clear();
                }
            } else {
                let laps = self.setup.laps;
                if self.route_complete_at.is_none() {
                    let lap_now = ((self.distance / len).floor() as u32 + 1).min(laps + 1);
                    if lap_now > self.lap {
                        // close the route lap split
                        let n = self.lap;
                        if let Some(sp) = Split::from_samples(&self.samples, self.route_lap_start_idx, self.samples.len(), "route_lap", format!("Route lap {n}"), n) {
                            self.splits.push(sp);
                        }
                        self.route_lap_start_idx = self.samples.len();
                    }
                    if self.distance >= len * laps as f64 {
                        self.route_complete_at = Some(self.active_s);
                        let t = fmt_dur(self.active_s);
                        self.push_event("route_complete", &format!("Route complete in {t}!"));
                    } else if lap_now > self.lap {
                        self.lap = lap_now;
                        self.announced.clear();
                        self.push_event("lap", &format!("Lap {} of {}", lap_now, laps));
                    }
                }
            }
        }

        // ---- elevation ----
        let new_ele = match &self.setup.route {
            Some(r) => r.elevation_at(self.lap_distance()),
            None => {
                self.virtual_ele += phys_grade * ds;
                self.virtual_ele
            }
        };
        if self.lap == prev_lap && new_ele > self.ele {
            self.gain += new_ele - self.ele;
        }
        self.ele = new_ele;

        // ---- climb announcements ----
        if self.route_complete_at.is_none() {
            let lap_d = self.lap_distance();
            let mut msgs = Vec::new();
            for (i, c) in self.climbs.iter().enumerate() {
                let to = c.start_d - lap_d;
                if to > 0.0 && to <= 600.0 && !self.announced.contains(&i) {
                    msgs.push((i, format!(
                        "{} climb in {:.0} m: {:.1} km at {:.1}%",
                        if c.category == "Uncategorised" { "Short".to_string() } else { c.category.clone() },
                        to,
                        c.length_m / 1000.0,
                        c.avg_grade_pct
                    )));
                }
            }
            for (i, m) in msgs {
                self.announced.insert(i);
                self.push_event("climb", &m);
            }
        }

        // ---- ramp test failure detection ----
        if self.setup.test == Some(TestKind::Ramp) && !self.test_done {
            let wt = self.workout_t();
            if let Some((i, _)) = step_at(&self.steps, wt) {
                if self.steps[i].label.starts_with("Ramp") {
                    let target = self.target_w.unwrap_or(0.0);
                    let failing = self.cadence < 45.0 || self.power < 0.65 * target;
                    if failing {
                        self.ramp_fail_s += dt;
                    } else {
                        self.ramp_fail_s = (self.ramp_fail_s - dt).max(0.0);
                    }
                    if self.ramp_fail_s >= 8.0 {
                        self.test_done = true;
                        if let Some(cd) = self.steps.iter().find(|s| s.label == "Cool-down") {
                            self.offset_s = cd.start_s as f64 - self.active_s;
                        }
                        let msg = match self.live_test_result() {
                            Some(r) => format!("Ramp done! Best minute {:.0} W → FTP {:.0} W", r.measured_w, r.ftp),
                            None => "Ramp done!".into(),
                        };
                        self.push_event("test_complete", &msg);
                        self.force_cmd = true;
                    }
                }
            }
        }

        // ---- heart-rate controller (free ride HR mode) ----
        if self.setup.kind == RideKind::Free && !self.structured_active() && self.free_mode == FreeMode::HeartRate {
            self.hr_ctrl_t += dt;
            if self.hr_ctrl_t >= 4.0 {
                self.hr_ctrl_t = 0.0;
                if let Some(h) = self.hr {
                    let err = self.hr_target - h;
                    let ftp = self.profile.ftp.max(50.0);
                    self.hr_w = (self.hr_w + (err * 1.5).clamp(-15.0, 15.0)).clamp(0.3 * ftp, 1.3 * ftp).round();
                }
            }
        }

        // ---- 1 Hz recording ----
        self.acc.dt += dt;
        self.acc.p += self.power * dt;
        self.acc.c += self.cadence * dt;
        self.acc.v += self.speed * dt;
        if let Some(h) = self.hr {
            self.acc.hr += h * dt;
            self.acc.hr_dt += dt;
        }
        if let Some(t) = self.target_w {
            self.acc.target += t * dt;
            self.acc.target_dt += dt;
        }
        while (self.samples.len() as f64) + 1.0 <= self.active_s {
            let a = &self.acc;
            let n = a.dt.max(1e-9);
            let sec = self.samples.len() as u32;
            self.samples.t.push(sec);
            self.samples.power.push((a.p / n) as f32);
            self.samples.cadence.push((a.c / n) as f32);
            self.samples.speed.push((a.v / n) as f32);
            self.samples.hr.push(if a.hr_dt > 0.0 { (a.hr / a.hr_dt) as f32 } else { 0.0 });
            self.samples.target.push(if a.target_dt > 0.0 { (a.target / a.target_dt) as f32 } else { 0.0 });
            self.samples.distance.push(self.distance as f32);
            self.samples.ele.push(self.ele as f32);
            self.samples.grade.push((self.grade * 100.0) as f32);
            self.workout_t_samples.push(sec as f64 + self.offset_s);
            self.acc = SecondAcc::default();
        }

        // ---- timed sectors ----
        let n_samples = self.samples.len();
        let mut evs = Vec::new();
        let mut new_splits = Vec::new();
        for s in self.sectors.iter_mut() {
            if s.done {
                continue;
            }
            if s.started_idx.is_none() && self.distance >= s.start_total {
                s.started_idx = Some(n_samples);
                if s.timed {
                    evs.push(format!("Sector started: {}", s.name));
                }
            }
            if let Some(a) = s.started_idx {
                if self.distance >= s.end_total && n_samples > a {
                    s.done = true;
                    if s.timed {
                        if let Some(sp) = Split::from_samples(&self.samples, a, n_samples, "sector", s.name.clone(), s.id) {
                            evs.push(format!("{}: {} · {:.0} W", s.name, fmt_dur(sp.duration_s), sp.avg_power));
                            s.result = Some(sp.clone());
                            new_splits.push(sp);
                        }
                    }
                }
            }
        }
        self.splits.extend(new_splits);
        for e in evs {
            self.push_event("sector", &e);
        }

        // ---- goal ----
        if let (Some(g), false) = (self.goal, self.goal_done) {
            if self.goal_progress(&g) >= 1.0 {
                self.goal_done = true;
                self.push_event("goal", &format!("Goal reached: {}!", goal_label(&g)));
            }
        }
    }

    fn goal_progress(&self, g: &Goal) -> f64 {
        let (have, want) = match *g {
            Goal::Time { seconds } => (self.active_s, seconds),
            Goal::Distance { meters } => (self.distance, meters),
            Goal::Energy { kj } => (self.kj, kj),
            Goal::Calories { kcal } => (metrics::calories_from_work(self.kj, self.profile.gross_efficiency), kcal),
            Goal::Elevation { meters } => (self.gain, meters),
        };
        if want <= 0.0 {
            1.0
        } else {
            (have / want).clamp(0.0, 1.0)
        }
    }

    fn goal_remaining(&self, g: &Goal) -> String {
        match *g {
            Goal::Time { seconds } => format!("{} to go", fmt_dur((seconds - self.active_s).max(0.0))),
            Goal::Distance { meters } => format!("{:.2} km to go", ((meters - self.distance) / 1000.0).max(0.0)),
            Goal::Energy { kj } => format!("{:.0} kJ to go", (kj - self.kj).max(0.0)),
            Goal::Calories { kcal } => {
                format!("{:.0} kcal to go", (kcal - metrics::calories_from_work(self.kj, self.profile.gross_efficiency)).max(0.0))
            }
            Goal::Elevation { meters } => format!("{:.0} m to climb", (meters - self.gain).max(0.0)),
        }
    }

    /// Work out the control mode / target and, if it changed, a trainer command.
    fn control(&mut self, dt: f64) -> Option<TrainerCommand> {
        let ftp = self.profile.ftp.max(50.0);
        let mut step_grade: Option<(f64, bool)> = None;
        let mut mode = ControlMode::Sim;
        let mut target = None;

        if self.structured_active() {
            let wt = self.workout_t();
            match step_at(&self.steps, wt) {
                Some((i, into)) => {
                    if self.step_idx != Some(i) {
                        let first = self.step_idx.is_none() && i == 0;
                        self.step_idx = Some(i);
                        if !first {
                            let s = &self.steps[i];
                            let msg = match s.target {
                                StepTarget::Erg { from, .. } => format!("{} · {:.0} W", s.label, from * ftp * self.erg_bias),
                                StepTarget::Free { .. } => format!("{} · ride by feel", s.label),
                            };
                            self.push_event("step", &msg);
                        }
                    }
                    let s = &self.steps[i];
                    match s.target {
                        StepTarget::Erg { .. } => {
                            let w = s.fraction_at(into).unwrap_or(0.5) * ftp * self.erg_bias;
                            target = Some(w.round());
                            if !self.erg_disabled {
                                mode = ControlMode::Erg;
                            }
                        }
                        StepTarget::Free { grade_pct } => step_grade = Some((grade_pct, s.effort)),
                    }
                }
                None => {
                    self.workout_complete = true;
                    self.step_idx = None;
                    let m = if self.setup.test.is_some() {
                        "Test finished – great work!"
                    } else if self.setup.kind == RideKind::Workout {
                        "Workout complete!"
                    } else {
                        "Intervals done – back to free riding"
                    };
                    self.push_event("workout_complete", m);
                    self.force_cmd = true;
                }
            }
        } else if self.setup.kind == RideKind::Free {
            match self.free_mode {
                FreeMode::Gradient => {}
                FreeMode::Power => {
                    mode = ControlMode::Erg;
                    target = Some(self.free_watts);
                }
                FreeMode::HeartRate => {
                    mode = ControlMode::Erg;
                    target = Some(self.hr_w);
                }
                FreeMode::Level => mode = ControlMode::Level,
            }
        }
        self.mode = mode;
        self.target_w = target;

        // Desired SIM gradient (percent) before smoothing.
        let lap_d = self.lap_distance();
        let has_route = self.setup.route.is_some() && self.route_complete_at.is_none();
        let free_grad = self.free_gradient_active();
        let desired = if let Some((g, _)) = step_grade.filter(|(_, effort)| *effort || !has_route) {
            g
        } else if has_route {
            let r = self.setup.route.as_ref().unwrap();
            let look = lap_d + self.speed * 1.0;
            let half = (self.speed * 2.0).max(10.0);
            r.grade_at(look, half) * 100.0 * self.difficulty + if free_grad { self.free_grade_pct } else { 0.0 }
        } else if self.setup.kind == RideKind::Free {
            self.free_grade_pct
        } else {
            0.0
        };
        let desired = if self.paused { 0.0 } else { desired };
        let alpha = if dt > 0.0 { (dt / 1.0).min(1.0) } else { 0.0 };
        if self.force_cmd && self.last_cmd.is_none() {
            self.trainer_grade_pct = desired;
        } else {
            self.trainer_grade_pct += (desired - self.trainer_grade_pct) * alpha;
        }
        self.trainer_grade_pct = self.trainer_grade_pct.clamp(self.profile.min_trainer_grade, self.profile.max_trainer_grade);

        // Virtual shifting: in SIM-type riding hold the power the virtual gear needs.
        self.shifting_active = self.profile.virtual_shifting && self.mode == ControlMode::Sim && !self.paused && !self.finished;
        let shift_w = if self.shifting_active {
            Some(
                gears::target_power(self.cad_ema, self.gear_idx, self.profile.wheel_circumference_m, self.trainer_grade_pct / 100.0, self.ele, &self.params)
                    .round(),
            )
        } else {
            None
        };

        let cmd = if self.paused || self.finished {
            TrainerCommand::Simulation { grade_pct: 0.0, wind_mps: 0.0, crr: self.params.crr, cw: self.params.cw() }
        } else {
            match (self.mode, shift_w) {
                (ControlMode::Sim, Some(w)) => TrainerCommand::TargetPower { watts: w },
                (ControlMode::Sim, None) => TrainerCommand::Simulation {
                    grade_pct: (self.trainer_grade_pct * 10.0).round() / 10.0,
                    wind_mps: 0.0,
                    crr: self.params.crr,
                    cw: (self.params.cw() * 100.0).round() / 100.0,
                },
                (ControlMode::Erg, _) => TrainerCommand::TargetPower { watts: self.target_w.unwrap_or(100.0).round() },
                (ControlMode::Level, _) => TrainerCommand::Resistance { level_pct: (self.free_level_pct * 2.0).round() / 2.0 },
            }
        };
        let since = self.now_s - self.last_cmd_at;
        let changed = match (self.last_cmd, cmd) {
            (Some(TrainerCommand::Simulation { grade_pct: a, .. }), TrainerCommand::Simulation { grade_pct: b, .. }) => (a - b).abs() >= 0.1 && since >= 0.5,
            (Some(TrainerCommand::TargetPower { watts: a }), TrainerCommand::TargetPower { watts: b }) => (a - b).abs() >= 1.0 && since >= 0.5,
            (Some(TrainerCommand::Resistance { level_pct: a }), TrainerCommand::Resistance { level_pct: b }) => (a - b).abs() >= 0.5,
            _ => true,
        };
        if self.force_cmd || changed || since >= 5.0 {
            self.force_cmd = false;
            self.last_cmd = Some(cmd);
            self.last_cmd_at = self.now_s;
            Some(cmd)
        } else {
            None
        }
    }

    fn live_test_result(&self) -> Option<TestResult> {
        let kind = self.setup.test?;
        let p = self.samples.power_f64();
        fitness_test::evaluate(kind, &self.steps, &self.workout_t_samples, &p)
    }

    fn climb_snapshot(&self, i: usize, lap_d: f64) -> ClimbSnapshot {
        let c = &self.climbs[i];
        let on = lap_d >= c.start_d && lap_d <= c.end_d;
        let ele_now = self.setup.route.as_ref().map(|r| r.elevation_at(lap_d)).unwrap_or(0.0);
        ClimbSnapshot {
            index: i,
            category: c.category.clone(),
            length_m: c.length_m,
            gain_m: c.gain_m,
            avg_grade_pct: c.avg_grade_pct,
            distance_to_m: (c.start_d - lap_d).max(0.0),
            progress: if on { ((lap_d - c.start_d) / c.length_m).clamp(0.0, 1.0) } else { 0.0 },
            remaining_m: if on { c.end_d - lap_d } else { c.length_m },
            remaining_gain_m: if on { (c.top_ele - ele_now).max(0.0) } else { c.gain_m },
        }
    }

    pub fn snapshot(&mut self) -> Snapshot {
        let p = self.samples.power_f64();
        let np = metrics::normalized_power(&p);
        let avg = if self.active_s > 0.0 { self.kj * 1000.0 / self.active_s } else { 0.0 };
        let p3 = self.power_3s();
        let lap_d = self.lap_distance();
        let route_len = self.route_len();
        let laps = self.setup.laps;
        let endless = self.endless();
        let total_route = if endless { None } else { route_len.map(|l| l * laps as f64) };

        let wt = self.workout_t();
        let ftp = self.profile.ftp.max(50.0);
        let step = if !self.structured_active() {
            None
        } else {
            step_at(&self.steps, wt).map(|(i, into)| {
                let s = &self.steps[i];
                let next = self.steps.get(i + 1);
                StepSnapshot {
                    index: i,
                    label: s.label.clone(),
                    elapsed_s: into,
                    remaining_s: s.duration_s as f64 - into,
                    target_w: s.fraction_at(into).map(|f| (f * ftp * self.erg_bias).round()),
                    grade_pct: match s.target {
                        StepTarget::Free { grade_pct } => Some(grade_pct),
                        _ => None,
                    },
                    effort: s.effort,
                    cadence: s.cadence,
                    next_label: next.map(|n| n.label.clone()),
                    next_target_w: next.and_then(|n| n.fraction_at(0.0)).map(|f| (f * ftp * self.erg_bias).round()),
                }
            })
        };

        let (ghost_gap, ghost_d) = match &self.setup.ghost {
            Some(g) if self.distance > 5.0 => {
                let gap = g.time_at_distance(self.distance).map(|tg| self.active_s - tg);
                (gap, Some(g.distance_at_time(self.active_s)))
            }
            Some(g) => (None, Some(g.distance_at_time(self.active_s))),
            None => (None, None),
        };

        let (mut current, mut next) = (None, None);
        if self.route_complete_at.is_none() {
            for (i, c) in self.climbs.iter().enumerate() {
                if lap_d >= c.start_d && lap_d <= c.end_d {
                    current = Some(self.climb_snapshot(i, lap_d));
                } else if c.start_d > lap_d && next.is_none() {
                    next = Some(self.climb_snapshot(i, lap_d));
                }
            }
        }

        let test_status = self.setup.test.map(|k| match (k, self.live_test_result()) {
            (TestKind::Ramp, Some(r)) => format!("Best minute {:.0} W · est. FTP {:.0} W", r.measured_w, r.ftp),
            (_, Some(r)) if r.measured_w > 0.0 => format!("{} · est. FTP {:.0} W", r.detail, r.ftp),
            _ => "Warming up".to_string(),
        });

        let sectors: Vec<SectorSnapshot> = self
            .sectors
            .iter()
            .rev()
            .filter(|s| !s.done || s.result.is_some())
            .take(6)
            .map(|s| {
                let active = s.started_idx.is_some() && !s.done;
                SectorSnapshot {
                    id: s.id,
                    name: s.name.clone(),
                    kind: s.kind,
                    distance_to_m: (s.start_total - self.distance).max(0.0),
                    length_m: s.length_m,
                    avg_grade_pct: s.avg_grade_pct,
                    progress: if active { ((self.distance - s.start_total) / s.length_m.max(1.0)).clamp(0.0, 1.0) } else if s.done { 1.0 } else { 0.0 },
                    active,
                    done: s.done,
                    timed: s.timed,
                    elapsed_s: match s.started_idx {
                        Some(a) if active => (self.samples.len().saturating_sub(a)) as f64,
                        _ => 0.0,
                    },
                    result: s.result.clone(),
                }
            })
            .collect();

        let split_samples = self.samples.len().saturating_sub(self.split_start_idx);
        let split_avg = if split_samples > 0 { self.samples.power[self.split_start_idx..].iter().map(|x| *x as f64).sum::<f64>() / split_samples as f64 } else { 0.0 };

        let notice = if self.setup.kind == RideKind::Free && self.free_mode == FreeMode::HeartRate && self.hr.is_none() && !self.structured_active() {
            Some("No heart rate – connect a strap. Holding the last power.".into())
        } else {
            None
        };

        let goal = self.goal.map(|g| GoalSnapshot { goal: g, label: goal_label(&g), progress: self.goal_progress(&g), remaining: self.goal_remaining(&g), done: self.goal_done });

        let events = std::mem::take(&mut self.events);
        Snapshot {
            kind: self.setup.kind,
            plan_version: self.plan_version,
            elapsed_s: self.active_s,
            moving_s: self.moving_s,
            paused: self.paused,
            finished: self.finished,
            trainer_connected: self.connected,
            distance_m: self.distance,
            lap: self.lap,
            laps,
            lap_distance_m: lap_d,
            route_distance_m: route_len,
            route_progress: total_route.map(|t| (self.distance / t).min(1.0)),
            remaining_m: total_route.map(|t| (t - self.distance).max(0.0)),
            route_complete: self.route_complete_at.is_some(),
            route_time_s: self.route_complete_at,
            endless,
            route_modified: self.route_modified,
            ele_m: self.ele,
            grade_pct: self.grade * 100.0,
            trainer_grade_pct: self.trainer_grade_pct,
            mode: self.mode,
            target_w: self.target_w,
            trainer_difficulty: self.difficulty,
            speed_kmh: self.speed * 3.6,
            power_w: self.power,
            power_3s: p3,
            cadence_rpm: self.cadence,
            heart_rate: self.hr,
            hr_zone: self.hr.map(|h| hr_zone_of(h, &self.hr_zones)),
            hr_zone_count: self.hr_zones.len() as u32,
            avg_power: avg,
            max_power: self.max_power,
            np,
            work_kj: self.kj,
            calories_kcal: metrics::calories_from_work(self.kj, self.profile.gross_efficiency),
            elevation_gain_m: self.gain,
            wkg_3s: if self.profile.weight_kg > 0.0 { p3 / self.profile.weight_kg } else { 0.0 },
            tss: metrics::tss(self.active_s, np, ftp),
            structured: self.structured_active(),
            step,
            workout_t: wt,
            workout_total_s: self.steps.last().map(|s| s.end_s() as f64),
            workout_complete: self.workout_complete,
            erg_bias_pct: self.erg_bias * 100.0,
            erg_disabled: self.erg_disabled,
            free_mode: self.free_mode,
            free_grade_pct: self.free_grade_pct,
            free_erg_w: if self.free_mode == FreeMode::Power { Some(self.free_watts) } else { None },
            free_watts: self.free_watts,
            free_level_pct: self.free_level_pct,
            hr_target_bpm: self.hr_target,
            hr_control_w: self.hr_w,
            goal,
            split_count: self.splits.iter().filter(|s| s.kind == "lap").count() as u32,
            split_elapsed_s: self.active_s - self.split_start_s,
            split_distance_m: self.distance - self.split_start_d,
            split_avg_power: split_avg,
            recent_splits: self.splits.iter().rev().take(8).cloned().collect(),
            sectors,
            can_undo_sector: self.sectors.last().map(|s| s.started_idx.is_none()).unwrap_or(false),
            ghost_gap_s: ghost_gap,
            ghost_distance_m: ghost_d,
            current_climb: current,
            next_climb: next,
            test_status,
            test_done: self.test_done || (self.setup.test.is_some() && self.workout_complete),
            notice,
            virtual_shifting: self.profile.virtual_shifting,
            shifting_active: self.shifting_active,
            gear: self.gear_idx as u32 + 1,
            gear_count: gears::gear_count() as u32,
            gear_ratio: gears::VIRTUAL_GEARS[self.gear_idx],
            events,
        }
    }

    /// Finalise the ride and build the record to persist.
    pub fn finish(&mut self, ended_at: String, id: String) -> RideRecord {
        self.finished = true;
        // Close an open manual lap if the rider used laps.
        if self.splits.iter().any(|s| s.kind == "lap") && self.samples.len() > self.split_start_idx + 5 {
            self.close_manual_lap();
        }
        let p = self.samples.power_f64();
        let profile = &self.profile;
        let ftp = profile.ftp.max(50.0);
        let np = metrics::normalized_power(&p);
        let avg_power = if self.active_s > 0.0 { self.kj * 1000.0 / self.active_s } else { 0.0 };
        let hrs: Vec<f64> = self.samples.hr.iter().filter(|h| **h > 0.0).map(|h| *h as f64).collect();
        let cad: Vec<f64> = self.samples.cadence.iter().filter(|c| **c > 0.0).map(|c| *c as f64).collect();
        let avg_hr = if hrs.is_empty() { None } else { Some(metrics::mean(&hrs)) };
        let max_hr = hrs.iter().cloned().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
        let completed = match self.setup.kind {
            RideKind::Route => self.route_complete_at.is_some(),
            RideKind::Workout | RideKind::Test => self.workout_complete || self.test_done,
            RideKind::Free => true,
        };
        let test_result = self.live_test_result();
        let mut splits = self.splits.clone();
        splits.sort_by(|a, b| a.start_s.partial_cmp(&b.start_s).unwrap_or(std::cmp::Ordering::Equal));
        let summary = RideSummary {
            id,
            name: self.setup.name.clone(),
            kind: self.setup.kind,
            started_at: self.started_at.clone(),
            ended_at,
            route_id: self.setup.route.as_ref().filter(|r| r.name != "Virtual road").map(|r| r.id.clone()),
            route_name: self.setup.route.as_ref().filter(|r| r.name != "Virtual road").map(|r| r.name.clone()),
            workout_id: if self.setup.test.is_none() { self.setup.workout.as_ref().map(|w| w.id.clone()) } else { None },
            workout_name: self.setup.workout.as_ref().map(|w| w.name.clone()),
            event_id: self.setup.event_id.clone(),
            test: self.setup.test,
            laps: self.setup.laps,
            completed,
            route_time_s: self.route_complete_at,
            personal_best: false,
            elapsed_s: self.active_s,
            moving_s: self.moving_s,
            distance_m: self.distance,
            elevation_gain_m: self.gain,
            avg_speed_mps: if self.moving_s > 0.0 { self.distance / self.moving_s } else { 0.0 },
            max_speed_mps: self.max_speed,
            avg_power,
            max_power: self.max_power,
            np,
            intensity_factor: metrics::intensity_factor(np, ftp),
            tss: metrics::tss(self.active_s, np, ftp),
            variability_index: if avg_power > 0.0 { np / avg_power } else { 0.0 },
            work_kj: self.kj,
            calories_kcal: metrics::calories_from_work(self.kj, profile.gross_efficiency),
            calories_hr_kcal: avg_hr.map(|h| metrics::calories_from_hr(h, self.moving_s / 60.0, profile)),
            avg_hr,
            max_hr,
            avg_cadence: metrics::mean(&cad),
            max_cadence: cad.iter().cloned().fold(0.0, f64::max),
            ftp: profile.ftp,
            weight_kg: profile.weight_kg,
            power_bests: metrics::power_bests(&p),
            zone_s: metrics::time_in_zones(&p, ftp),
            test_result,
            notes: String::new(),
            splits,
            route_modified: self.route_modified,
            hr_zone_s: {
                let mut z = vec![0.0; self.hr_zones.len()];
                for h in self.samples.hr.iter().filter(|h| **h > 0.0) {
                    z[(hr_zone_of(*h as f64, &self.hr_zones) as usize).saturating_sub(1)] += 1.0;
                }
                z
            },
            strava_activity_id: None,
            simulated: false,
        };
        RideRecord { summary, samples: self.samples.clone() }
    }
}

pub fn goal_label(g: &Goal) -> String {
    match *g {
        Goal::Time { seconds } => fmt_dur(seconds),
        Goal::Distance { meters } => format!("{:.1} km", meters / 1000.0),
        Goal::Energy { kj } => format!("{kj:.0} kJ"),
        Goal::Calories { kcal } => format!("{kcal:.0} kcal"),
        Goal::Elevation { meters } => format!("{meters:.0} m climbing"),
    }
}

pub fn fmt_dur(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::{build_from_segments, SegmentSpec};
    use crate::workout::{builtin_workouts, parse_text};

    fn input(p: f64) -> TrainerInput {
        TrainerInput { power_w: p, cadence_rpm: 90.0, heart_rate: Some(140.0), connected: true }
    }

    fn run(engine: &mut RideEngine, secs: f64, mut power: impl FnMut(&Snapshot) -> f64) -> (Vec<TrainerCommand>, Vec<RideEvent>) {
        let mut cmds = Vec::new();
        let mut evs = Vec::new();
        let start = engine.last_ms.unwrap_or(0);
        let mut t = start;
        let (mut snap, c) = engine.tick(t, input(0.0));
        cmds.extend(c);
        evs.extend(snap.events.clone());
        while ((t - start) as f64) < secs * 1000.0 {
            t += 250;
            let p = power(&snap);
            let (s, c) = engine.tick(t, input(p));
            cmds.extend(c);
            evs.extend(s.events.clone());
            snap = s;
        }
        (cmds, evs)
    }

    fn hill_route() -> Route {
        build_from_segments(
            "hill",
            &[
                SegmentSpec { length_m: 1000.0, grade_pct: 0.0 },
                SegmentSpec { length_m: 2000.0, grade_pct: 6.0 },
                SegmentSpec { length_m: 1000.0, grade_pct: -6.0 },
            ],
            100.0,
            true,
        )
    }

    fn route_setup(route: Route, laps: u32) -> RideSetup {
        RideSetup { route: Some(route), laps, ..RideSetup::simple(RideKind::Route, "r") }
    }

    #[test]
    fn route_ride_energy_distance_and_commands() {
        let profile = Profile { ftp: 250.0, weight_kg: 75.0, ..Default::default() };
        let mut e = RideEngine::new(profile, route_setup(hill_route(), 1), "2026-01-01T00:00:00Z".into());
        let (cmds, evs) = run(&mut e, 900.0, |_| 250.0);
        let s = e.snapshot();
        assert!(s.route_complete, "distance {}", s.distance_m);
        assert!(evs.iter().any(|x| x.kind == "climb"));
        assert!(evs.iter().any(|x| x.kind == "route_complete"));
        assert!((s.work_kj - 225.0).abs() < 0.5, "{}", s.work_kj);
        assert!((s.calories_kcal - 224.0).abs() < 2.0);
        assert!((s.elevation_gain_m - 120.0).abs() < 8.0, "{}", s.elevation_gain_m);
        let grades: Vec<f64> = cmds
            .iter()
            .filter_map(|c| match c {
                TrainerCommand::Simulation { grade_pct, .. } => Some(*grade_pct),
                _ => None,
            })
            .collect();
        assert!(grades.iter().cloned().fold(f64::MIN, f64::max) > 5.0);
        assert!(grades.iter().cloned().fold(f64::MAX, f64::min) < -4.0);
        assert!(cmds.len() < 900, "{}", cmds.len());

        let rec = e.finish("2026-01-01T00:15:00Z".into(), "id".into());
        assert_eq!(rec.samples.len(), 900);
        assert!(rec.summary.completed);
        assert!((rec.summary.avg_power - 250.0).abs() < 1.0);
        assert!((rec.summary.np - 250.0).abs() < 1.0);
        assert_eq!(rec.summary.avg_hr, Some(140.0));
        assert!(rec.summary.route_time_s.unwrap() < 900.0);
        assert!(!rec.summary.route_modified);
    }

    #[test]
    fn workout_drives_erg() {
        let profile = Profile { ftp: 200.0, ..Default::default() };
        let wk = builtin_workouts().into_iter().find(|w| w.id == "builtin-threshold-3x10").unwrap();
        let setup = RideSetup { workout: Some(wk), ..RideSetup::simple(RideKind::Workout, "w") };
        let mut e = RideEngine::new(profile, setup, "x".into());
        let (cmds, _) = run(&mut e, 700.0, |s| s.target_w.unwrap_or(0.0));
        let s = e.snapshot();
        assert_eq!(s.mode, ControlMode::Erg);
        assert_eq!(s.step.as_ref().unwrap().label, "Interval 1/3");
        assert_eq!(s.target_w, Some(200.0));
        assert!(cmds.iter().any(|c| matches!(c, TrainerCommand::TargetPower { watts } if *watts == 200.0)));
        e.apply(RideAction::ErgBias { delta_pct: 5.0 });
        let (_, c) = e.tick(700_250, input(200.0));
        assert!(matches!(c, Some(TrainerCommand::TargetPower { watts }) if watts == 210.0));
        e.apply(RideAction::SkipStep);
        let (s, _) = e.tick(700_500, input(200.0));
        assert_eq!(s.step.unwrap().label, "Recover");
        e.apply(RideAction::ToggleErg);
        let (s, c) = e.tick(700_750, input(200.0));
        assert_eq!(s.mode, ControlMode::Sim);
        assert!(matches!(c, Some(TrainerCommand::Simulation { .. })));
    }

    #[test]
    fn ramp_test_detects_failure_and_estimates_ftp() {
        let profile = Profile { ftp: 250.0, ..Default::default() };
        let setup = RideSetup { test: Some(TestKind::Ramp), ..RideSetup::simple(RideKind::Test, "ramp") };
        let mut e = RideEngine::new(profile, setup, "x".into());
        let mut t = 0u64;
        let mut snap = e.tick(t, input(0.0)).0;
        let mut done = false;
        while t < 3_000_000 {
            t += 250;
            let target = snap.target_w.unwrap_or(0.0);
            let p = if target > 330.0 { 120.0 } else { target };
            snap = e.tick(t, TrainerInput { power_w: p, cadence_rpm: if target > 330.0 { 40.0 } else { 90.0 }, heart_rate: None, connected: true }).0;
            if snap.test_done {
                done = true;
                break;
            }
        }
        assert!(done);
        let rec = e.finish("y".into(), "id".into());
        let r = rec.summary.test_result.unwrap();
        assert_eq!(r.measured_w, 320.0);
        assert_eq!(r.ftp, 240.0);
    }

    #[test]
    fn ghost_gap() {
        let ghost = GhostTrace {
            ride_id: "g".into(),
            started_at: "".into(),
            total_s: 500.0,
            t: (0..=500).map(|x| x as f32).collect(),
            d: (0..=500).map(|x| x as f32 * 8.0).collect(),
        };
        let setup = RideSetup { ghost: Some(ghost), ..route_setup(hill_route(), 1) };
        let mut e = RideEngine::new(Profile::default(), setup, "x".into());
        run(&mut e, 120.0, |_| 150.0);
        let s = e.snapshot();
        assert!(s.ghost_gap_s.is_some());
        assert!((s.ghost_distance_m.unwrap() - 960.0).abs() < 1.0);
    }

    #[test]
    fn pause_freezes_time_and_energy() {
        let mut e = RideEngine::new(Profile::default(), RideSetup::simple(RideKind::Free, "f"), "x".into());
        e.tick(0, input(200.0));
        e.tick(10_000, input(200.0));
        e.pause();
        e.tick(20_000, input(200.0));
        e.resume();
        let (s, _) = e.tick(21_000, input(200.0));
        assert!((s.elapsed_s - 3.0).abs() < 1e-6, "{}", s.elapsed_s);
        assert!((s.work_kj - 0.6).abs() < 1e-6);
    }

    #[test]
    fn free_ride_grade_and_erg_controls() {
        let mut e = RideEngine::new(Profile::default(), RideSetup::simple(RideKind::Free, "f"), "x".into());
        e.apply(RideAction::Grade { delta_pct: 4.0 });
        let mut last = None;
        for i in 0..40 {
            let (_, c) = e.tick(i * 250, input(200.0));
            if c.is_some() {
                last = c;
            }
        }
        assert!(matches!(last, Some(TrainerCommand::Simulation { grade_pct, .. }) if (grade_pct - 4.0).abs() < 0.2));
        e.apply(RideAction::FreeErg { watts: Some(180.0) });
        let (s, c) = e.tick(10_250, input(200.0));
        assert_eq!(s.mode, ControlMode::Erg);
        assert!(matches!(c, Some(TrainerCommand::TargetPower { watts }) if watts == 180.0));
    }

    #[test]
    fn free_ride_level_and_heart_rate_modes() {
        let profile = Profile { ftp: 200.0, ..Default::default() };
        let setup = RideSetup { free: Some(FreeOptions { mode: FreeMode::Level, level_pct: 40.0, ..Default::default() }), ..RideSetup::simple(RideKind::Free, "f") };
        let mut e = RideEngine::new(profile, setup, "x".into());
        let (s, c) = e.tick(0, input(150.0));
        assert_eq!(s.mode, ControlMode::Level);
        assert!(matches!(c, Some(TrainerCommand::Resistance { level_pct }) if level_pct == 40.0));
        e.apply(RideAction::Level { delta_pct: 5.0 });
        let (_, c) = e.tick(250, input(150.0));
        assert!(matches!(c, Some(TrainerCommand::Resistance { level_pct }) if level_pct == 45.0));

        // HR mode: HR below target → controller raises the ERG target.
        e.apply(RideAction::SetFreeMode { mode: FreeMode::HeartRate });
        e.apply(RideAction::HrTarget { delta_bpm: 200.0 }); // clamps to 200
        let start = e.snapshot().hr_control_w;
        run(&mut e, 60.0, |s| s.target_w.unwrap_or(0.0));
        let s = e.snapshot();
        assert_eq!(s.mode, ControlMode::Erg);
        assert!(s.hr_control_w > start + 50.0, "{} vs {start}", s.hr_control_w);
        assert!(s.hr_control_w <= 260.0);
    }

    #[test]
    fn goals_and_manual_laps() {
        let setup = RideSetup { goal: Some(Goal::Energy { kj: 30.0 }), ..RideSetup::simple(RideKind::Free, "f") };
        let mut e = RideEngine::new(Profile::default(), setup, "x".into());
        let (_, evs) = run(&mut e, 100.0, |_| 200.0);
        assert!(!evs.iter().any(|x| x.kind == "goal"));
        e.apply(RideAction::Lap);
        let (_, evs) = run(&mut e, 60.0, |_| 300.0);
        assert!(evs.iter().any(|x| x.kind == "goal"));
        let s = e.snapshot();
        assert!(s.goal.unwrap().done);
        assert_eq!(s.split_count, 1);
        let rec = e.finish("z".into(), "id".into());
        let laps: Vec<_> = rec.summary.splits.iter().filter(|s| s.kind == "lap").collect();
        assert_eq!(laps.len(), 2);
        assert!((laps[0].avg_power - 200.0).abs() < 2.0);
        assert!((laps[1].avg_power - 300.0).abs() < 2.0);
        assert!((laps[0].duration_s - 100.0).abs() <= 1.0);
    }

    #[test]
    fn adding_a_sector_ahead_changes_the_road_and_times_it() {
        let flat = build_from_segments("flat", &[SegmentSpec { length_m: 5000.0, grade_pct: 0.0 }], 50.0, false);
        let ghost = GhostTrace { ride_id: "g".into(), started_at: "".into(), total_s: 100.0, t: vec![0.0, 100.0], d: vec![0.0, 5000.0] };
        let setup = RideSetup { ghost: Some(ghost), ..route_setup(flat, 1) };
        let mut e = RideEngine::new(Profile::default(), setup, "x".into());
        run(&mut e, 20.0, |_| 250.0);
        let pos = e.snapshot().lap_distance_m;
        let spec = SectorSpec { kind: SectorKind::Climb, name: None, length_m: 500.0, grade_pct: 6.0, segments: vec![], terrain: None, in_m: 200.0, at_end: false, rejoin: true, timed: true };
        e.apply(RideAction::AddSector { sector: spec });
        let s = e.snapshot();
        assert!(s.route_modified);
        assert!(s.ghost_gap_s.is_none() && s.ghost_distance_m.is_none());
        assert!(s.can_undo_sector);
        assert!(s.route_distance_m.unwrap() > 5000.0 + 500.0 + 500.0);
        assert_eq!(s.sectors.len(), 1);
        let info = e.info();
        let sec = &info.sectors[0];
        assert!(sec.start_d >= pos + 200.0 - 1.0, "{} {}", sec.start_d, pos);
        let r = info.route.unwrap();
        assert!((r.grade_at(sec.start_d + 250.0, 20.0) - 0.06).abs() < 0.005);
        // undo + redo
        e.apply(RideAction::UndoSector);
        assert!(e.snapshot().sectors.is_empty());
        assert!((e.info().route.unwrap().distance() - 5000.0).abs() < 1.0);
        let spec = SectorSpec { kind: SectorKind::Climb, name: Some("KOM".into()), length_m: 500.0, grade_pct: 6.0, segments: vec![], terrain: None, in_m: 200.0, at_end: false, rejoin: true, timed: true };
        e.apply(RideAction::AddSector { sector: spec });
        let (_, evs) = run(&mut e, 400.0, |_| 250.0);
        assert!(evs.iter().any(|x| x.message.starts_with("Sector started: KOM")));
        assert!(evs.iter().any(|x| x.message.starts_with("KOM: ")), "{evs:?}");
        let rec = e.finish("z".into(), "id".into());
        let sp = rec.summary.splits.iter().find(|s| s.kind == "sector").unwrap();
        assert!((sp.distance_m - 500.0).abs() < 15.0, "{}", sp.distance_m);
        assert!(sp.avg_grade_pct > 4.5, "{}", sp.avg_grade_pct);
        assert!(rec.summary.route_modified);
    }

    #[test]
    fn sectors_on_rides_without_a_route() {
        let mut e = RideEngine::new(Profile::default(), RideSetup::simple(RideKind::Free, "f"), "x".into());
        run(&mut e, 10.0, |_| 200.0);
        let spec = SectorSpec { kind: SectorKind::Rollers, name: None, length_m: 2000.0, grade_pct: 5.0, segments: vec![], terrain: None, in_m: 100.0, at_end: false, rejoin: true, timed: false };
        e.apply(RideAction::AddSector { sector: spec });
        let s = e.snapshot();
        assert!(s.endless);
        assert!(e.info().route.is_some());
        run(&mut e, 300.0, |_| 250.0);
        assert!(e.snapshot().elevation_gain_m > 20.0);
    }

    #[test]
    fn editing_a_workout_mid_ride() {
        let profile = Profile { ftp: 200.0, ..Default::default() };
        let blocks = parse_text("60s @50%, 3x 60s @120% / 60s @50%, 60s @50%", 200.0).unwrap();
        let wk = Workout { id: "w".into(), name: "w".into(), description: String::new(), category: String::new(), blocks, builtin: false, created_at: String::new() };
        let setup = RideSetup { workout: Some(wk), ..RideSetup::simple(RideKind::Workout, "w") };
        let mut e = RideEngine::new(profile, setup, "x".into());
        run(&mut e, 70.0, |s| s.target_w.unwrap_or(0.0));
        let v0 = e.snapshot().plan_version;
        let total0 = e.snapshot().workout_total_s.unwrap();
        assert_eq!(e.snapshot().step.unwrap().label, "Interval 1/3");
        // extend + adjust
        e.apply(RideAction::ExtendStep { seconds: 30 });
        e.apply(RideAction::AdjustStep { delta_pct: 10.0 });
        let s = e.snapshot();
        assert!(s.plan_version > v0);
        assert_eq!(s.workout_total_s.unwrap(), total0 + 30.0);
        assert_eq!(s.target_w, None.or(s.target_w)); // target updates on next tick
        let (s, c) = e.tick(e.last_ms.unwrap() + 250, input(260.0));
        assert_eq!(s.target_w, Some(260.0));
        assert!(matches!(c, Some(TrainerCommand::TargetPower { watts }) if watts == 260.0));
        // one more rep adds effort + recovery after the current recovery
        e.apply(RideAction::OneMoreRep);
        assert_eq!(e.snapshot().workout_total_s.unwrap(), total0 + 30.0 + 90.0 + 60.0); // copies the (extended) current rep
        assert!(e.info().steps.iter().any(|s| s.label == "Extra rep"));
        // insert blocks next
        let add = parse_text("2m @90%", 200.0).unwrap();
        e.apply(RideAction::InsertBlocks { blocks: add, at_end: false });
        let info = e.info();
        assert_eq!(info.steps[2].duration_s, 120);
        for w in info.steps.windows(2) {
            assert_eq!(w[0].end_s(), w[1].start_s);
        }
        // remove a future step
        let n = info.steps.len();
        e.apply(RideAction::RemoveStep { index: n - 1 });
        assert_eq!(e.info().steps.len(), n - 1);
        // skip to cool-down
        e.apply(RideAction::SkipToCooldown);
        let (s, _) = e.tick(e.last_ms.unwrap() + 250, input(100.0));
        assert_eq!(s.step.unwrap().label, "Cool-down");
    }

    #[test]
    fn intervals_inside_a_free_ride() {
        let profile = Profile { ftp: 200.0, ..Default::default() };
        let mut e = RideEngine::new(profile, RideSetup::simple(RideKind::Free, "f"), "x".into());
        run(&mut e, 30.0, |_| 150.0);
        assert!(!e.snapshot().structured);
        e.apply(RideAction::InsertBlocks { blocks: parse_text("2x 20s @150% / 10s @50%", 200.0).unwrap(), at_end: false });
        let (s, _) = e.tick(e.last_ms.unwrap() + 250, input(150.0));
        assert!(s.structured);
        assert_eq!(s.mode, ControlMode::Erg);
        assert_eq!(s.target_w, Some(300.0));
        let (_, evs) = run(&mut e, 70.0, |s| s.target_w.unwrap_or(150.0));
        assert!(evs.iter().any(|x| x.kind == "workout_complete"));
        let s = e.snapshot();
        assert!(!s.structured);
        assert_eq!(s.mode, ControlMode::Sim);
        // and again – appended after the finished plan
        e.apply(RideAction::InsertBlocks { blocks: parse_text("30s @100%", 200.0).unwrap(), at_end: false });
        let (s, _) = e.tick(e.last_ms.unwrap() + 250, input(150.0));
        assert_eq!(s.target_w, Some(200.0));
    }

    #[test]
    fn route_laps_can_be_added_mid_ride() {
        let route = build_from_segments("short", &[SegmentSpec { length_m: 500.0, grade_pct: 0.0 }], 0.0, false);
        let mut e = RideEngine::new(Profile::default(), route_setup(route, 1), "x".into());
        run(&mut e, 100.0, |_| 250.0);
        assert!(e.snapshot().route_complete);
        e.apply(RideAction::AddLaps { delta: 2 });
        assert!(!e.snapshot().route_complete);
        let (_, evs) = run(&mut e, 200.0, |_| 250.0);
        assert!(evs.iter().any(|x| x.kind == "route_complete"));
        let rec = e.finish("z".into(), "id".into());
        assert!(rec.summary.splits.iter().filter(|s| s.kind == "route_lap").count() >= 2);
    }

    #[test]
    fn endless_terrain_never_finishes() {
        let route = build_from_segments("loop", &[SegmentSpec { length_m: 400.0, grade_pct: 2.0 }, SegmentSpec { length_m: 400.0, grade_pct: -2.0 }], 0.0, false);
        let setup = RideSetup { route: Some(route), ..RideSetup::simple(RideKind::Free, "f") };
        let mut e = RideEngine::new(Profile::default(), setup, "x".into());
        let (_, evs) = run(&mut e, 300.0, |_| 250.0);
        let s = e.snapshot();
        assert!(s.endless && !s.route_complete && s.distance_m > 1600.0);
        assert!(!evs.iter().any(|x| x.kind == "lap" || x.kind == "route_complete"));
        assert!(s.elevation_gain_m > 10.0);
    }

    #[test]
    fn virtual_shifting_sets_resistance_from_cadence_and_gear() {
        let profile = Profile { virtual_shifting: true, start_gear: 12, ..Default::default() };
        let mut e = RideEngine::new(profile.clone(), RideSetup::simple(RideKind::Free, "vs"), "x".into());
        let mut last = None;
        for i in 0..40 {
            let (s, c) = e.tick(i * 250, input(180.0));
            assert!(s.shifting_active);
            if c.is_some() {
                last = c;
            }
        }
        let params = PhysicsParams::from_profile(&profile);
        let expect = gears::target_power(90.0, 11, profile.wheel_circumference_m, 0.0, e.ele, &params).round();
        let got = match last {
            Some(TrainerCommand::TargetPower { watts }) => watts,
            other => panic!("expected ERG target, got {other:?}"),
        };
        assert!((got - expect).abs() <= 2.0, "{got} vs {expect}");
        e.apply(RideAction::Shift { delta: 2 });
        let mut harder = got;
        for i in 40..50 {
            if let (_, Some(TrainerCommand::TargetPower { watts })) = e.tick(i * 250, input(180.0)) {
                harder = watts;
            }
        }
        assert!(harder > got * 1.15, "{harder} vs {got}");
        assert_eq!(e.snapshot().gear, 14);
        e.apply(RideAction::Grade { delta_pct: 5.0 });
        let mut climb = harder;
        for i in 50..90 {
            if let (_, Some(TrainerCommand::TargetPower { watts })) = e.tick(i * 250, input(180.0)) {
                climb = watts;
            }
        }
        assert!(climb > harder * 2.0, "{climb} vs {harder}");
    }

    #[test]
    fn virtual_shifting_leaves_erg_workouts_alone() {
        let profile = Profile { virtual_shifting: true, ftp: 200.0, ..Default::default() };
        let wk = builtin_workouts().into_iter().find(|w| w.id == "builtin-threshold-3x10").unwrap();
        let setup = RideSetup { workout: Some(wk), ..RideSetup::simple(RideKind::Workout, "w") };
        let mut e = RideEngine::new(profile, setup, "x".into());
        run(&mut e, 700.0, |s| s.target_w.unwrap_or(0.0));
        let s = e.snapshot();
        assert!(!s.shifting_active);
        assert_eq!(s.target_w, Some(200.0));
        e.apply(RideAction::ToggleErg);
        let (s, _) = e.tick(e.last_ms.unwrap() + 250, input(200.0));
        assert!(s.shifting_active);
    }

    #[test]
    fn laps() {
        let route = build_from_segments("short", &[SegmentSpec { length_m: 500.0, grade_pct: 0.0 }], 0.0, false);
        let mut e = RideEngine::new(Profile::default(), route_setup(route, 3), "x".into());
        let (_, evs) = run(&mut e, 300.0, |_| 250.0);
        let laps: Vec<_> = evs.iter().filter(|e| e.kind == "lap").collect();
        assert_eq!(laps.len(), 2);
        assert!(e.snapshot().route_complete);
    }
}
