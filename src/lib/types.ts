// Mirrors the Rust serde types (snake_case fields).

export type Units = "metric" | "imperial";
export type Sex = "male" | "female";

export interface Profile {
  name: string;
  weight_kg: number;
  bike_weight_kg: number;
  height_cm: number;
  age: number;
  sex: Sex;
  ftp: number;
  max_hr: number;
  rest_hr: number;
  units: Units;
  gross_efficiency: number;
  cda: number;
  crr: number;
  trainer_difficulty: number;
  max_trainer_grade: number;
  min_trainer_grade: number;
  weekly_goal_hours: number;
  erg_ramp_s: number;
  virtual_shifting: boolean;
  wheel_circumference_m: number;
  start_gear: number;
  hr_zone_model: HrZoneModel;
  lthr: number;
  hr_alerts: HrAlertSettings;
  ftp_auto: FtpAutoMode;
}

export type FtpAutoMode = "off" | "suggest" | "auto_increase";

export interface EffortPoint {
  duration_s: number;
  watts: number;
  ride_id: string;
  ride_name: string;
  date: string;
}

export interface FtpEstimate {
  estimate: number;
  method: "critical_power" | "best_20min";
  cp: number | null;
  w_prime_kj: number | null;
  r2: number | null;
  points: EffortPoint[];
  confidence: "high" | "medium" | "low";
  rides_considered: number;
  hard_ride_in_window: boolean;
}

export interface FtpAssessment {
  date: string;
  current_ftp: number;
  estimate: FtpEstimate | null;
  suggested_ftp: number | null;
  change_w: number;
  change_pct: number;
  suggestion: "increase" | "decrease" | "hold" | "not_enough_data";
  reason: string;
  trigger_ride_id: string | null;
  auto_applied: boolean;
  dismissed: boolean;
}

export type HrZoneModel = "max_hr" | "reserve" | "lthr";

export interface HrAlertSettings {
  enabled: boolean;
  low_zone: number;
  high_zone: number;
  alert_high: boolean;
  alert_low: boolean;
  grace_s: number;
  repeat_s: number;
  voice: boolean;
  volume: number;
}

export interface HrZone {
  zone: number;
  name: string;
  lo: number;
  hi: number | null;
}

export interface StravaStatus {
  configured: boolean;
  connected: boolean;
  client_id: string | null;
  athlete_name: string | null;
  auto_upload: boolean;
  api_base: string;
}

export interface StravaUploadEvent {
  ride_id: string;
  status: "uploading" | "processing" | "done" | "error";
  activity_id: number | null;
  error: string | null;
}

export type RouteSource = "generated" | "custom" | "gpx" | "builtin";
export type Terrain = "flat" | "rolling" | "hilly" | "mountainous" | "alpine";
export type Finish = "free" | "loop" | "summit";

export interface RoutePoint { d: number; ele: number }
export interface SegmentSpec { length_m: number; grade_pct: number }
export interface GenParams {
  distance_km: number;
  target_gain_m: number;
  max_grade_pct: number;
  terrain: Terrain;
  finish: Finish;
  seed: number;
  start_ele_m: number;
  smoothness: number;
}

export interface Route {
  id: string;
  name: string;
  description: string;
  source: RouteSource;
  created_at: string;
  points: RoutePoint[];
  is_loop: boolean;
  tags: string[];
  gen_params: GenParams | null;
  segments: SegmentSpec[] | null;
  favourite: boolean;
}

export interface Climb {
  start_d: number;
  end_d: number;
  length_m: number;
  gain_m: number;
  avg_grade_pct: number;
  max_grade_pct: number;
  start_ele: number;
  top_ele: number;
  score: number;
  category: string;
}

export interface RouteStats {
  distance_m: number;
  gain_m: number;
  loss_m: number;
  min_ele: number;
  max_ele: number;
  max_grade_pct: number;
  min_grade_pct: number;
  climb_density: number;
  climbing_share: number;
  climbs: Climb[];
  difficulty: number;
  difficulty_label: string;
}

export interface RouteSummary {
  id: string;
  name: string;
  description: string;
  source: RouteSource;
  created_at: string;
  is_loop: boolean;
  tags: string[];
  favourite: boolean;
  stats: RouteStats;
  thumbnail: [number, number][];
  est_time_s: number;
  best_time_s: number | null;
  attempts: number;
}

export interface RouteDetail {
  route: Route;
  stats: RouteStats;
  est_time_s: number;
  best_time_s: number | null;
  attempts: RideSummary[];
}

export type Block =
  | { type: "steady"; duration_s: number; power: number; cadence?: number | null; label?: string | null }
  | { type: "ramp"; duration_s: number; from: number; to: number; label?: string | null }
  | { type: "intervals"; repeats: number; on_s: number; on_power: number; off_s: number; off_power: number; on_cadence?: number | null }
  | { type: "free"; duration_s: number; grade_pct: number; label?: string | null; effort: boolean }
  | { type: "set"; repeats: number; blocks: Block[] };

export interface Workout {
  id: string;
  name: string;
  description: string;
  category: string;
  blocks: Block[];
  builtin: boolean;
  created_at: string;
}

export type StepTarget = { type: "erg"; from: number; to: number } | { type: "free"; grade_pct: number };

export interface Step {
  start_s: number;
  duration_s: number;
  target: StepTarget;
  label: string;
  effort: boolean;
  cadence: number | null;
}

export interface WorkoutEstimate {
  duration_s: number;
  avg_power: number;
  np: number;
  intensity_factor: number;
  tss: number;
  work_kj: number;
  zone_s: number[];
}

export interface WorkoutView {
  workout: Workout;
  steps: Step[];
  estimate: WorkoutEstimate;
  times_ridden: number;
}

export type TestKind = "ramp" | "ftp20" | "ftp8";

export interface TestInfo {
  kind: TestKind;
  name: string;
  summary: string;
  how: string;
  approx_minutes: number;
}

export interface TestResult {
  kind: TestKind;
  ftp: number;
  measured_w: number;
  factor: number;
  detail: string;
  max_power: number;
  valid: boolean;
}

export interface TestView {
  info: TestInfo;
  steps: Step[];
  last_result: RideSummary | null;
}

export type RideKind = "free" | "route" | "workout" | "test";

export interface RideSummary {
  id: string;
  name: string;
  kind: RideKind;
  started_at: string;
  ended_at: string;
  route_id: string | null;
  route_name: string | null;
  workout_id: string | null;
  workout_name: string | null;
  event_id: string | null;
  test: TestKind | null;
  laps: number;
  completed: boolean;
  route_time_s: number | null;
  personal_best: boolean;
  elapsed_s: number;
  moving_s: number;
  distance_m: number;
  elevation_gain_m: number;
  avg_speed_mps: number;
  max_speed_mps: number;
  avg_power: number;
  max_power: number;
  np: number;
  intensity_factor: number;
  tss: number;
  variability_index: number;
  work_kj: number;
  calories_kcal: number;
  calories_hr_kcal: number | null;
  avg_hr: number | null;
  max_hr: number | null;
  avg_cadence: number;
  max_cadence: number;
  ftp: number;
  weight_kg: number;
  power_bests: [number, number][];
  zone_s: number[];
  test_result: TestResult | null;
  notes: string;
  splits: Split[];
  route_modified: boolean;
  hr_zone_s: number[];
  strava_activity_id: number | null;
  simulated: boolean;
}

export interface Split {
  index: number;
  kind: "lap" | "route_lap" | "sector";
  name: string;
  start_s: number;
  duration_s: number;
  distance_m: number;
  elevation_gain_m: number;
  avg_power: number;
  max_power: number;
  np: number;
  avg_hr: number | null;
  avg_cadence: number;
  avg_speed_kmh: number;
  avg_grade_pct: number;
}

export interface RideSamples {
  t: number[];
  power: number[];
  cadence: number[];
  hr: number[];
  speed: number[];
  distance: number[];
  ele: number[];
  grade: number[];
  target: number[];
}

export interface RideRecord {
  summary: RideSummary;
  samples: RideSamples;
}

export interface GhostTrace {
  ride_id: string;
  started_at: string;
  total_s: number;
  t: number[];
  d: number[];
}

export type EventTarget =
  | { type: "route"; route_id: string; laps: number }
  | { type: "workout"; workout_id: string; route_id: string | null }
  | { type: "test"; test: TestKind };

export type Freq = "once" | "daily" | "weekly" | "every_n_days";

export interface Recurrence {
  freq: Freq;
  start_date: string;
  time: string;
  end_date: string | null;
  weekdays: number[];
  interval: number;
}

export interface RideEvent {
  id: string;
  name: string;
  description: string;
  target: EventTarget;
  recurrence: Recurrence;
  created_at: string;
  color: string;
  active: boolean;
}

export interface EventView {
  event: RideEvent;
  target_name: string;
  next: string | null;
  attempts: number;
  best_time_s: number | null;
  best_np: number | null;
  last_ride: RideSummary | null;
  streak: number;
}

export interface Occurrence {
  event_id: string;
  event_name: string;
  color: string;
  at: string;
}

export interface Totals {
  rides: number;
  distance_m: number;
  moving_s: number;
  elevation_m: number;
  work_kj: number;
  calories_kcal: number;
  tss: number;
}

export interface FitnessPoint { date: string; tss: number; ctl: number; atl: number; tsb: number }
export interface CurvePoint { duration_s: number; watts: number; wkg: number; ride_id: string; date: string }
export interface RecordRef { ride_id: string; ride_name: string; date: string; value: number }
export interface FtpEntry { date: string; ftp: number; source: string }

export interface AccountStats {
  all_time: Totals;
  this_week: Totals;
  last_week: Totals;
  this_month: Totals;
  this_year: Totals;
  weekly: { week_start: string; totals: Totals }[];
  fitness: FitnessPoint[];
  power_curve_all: CurvePoint[];
  power_curve_90d: CurvePoint[];
  records: {
    longest_distance: RecordRef | null;
    longest_time: RecordRef | null;
    most_elevation: RecordRef | null;
    most_work: RecordRef | null;
    highest_np: RecordRef | null;
    highest_tss: RecordRef | null;
  };
  ftp_history: FtpEntry[];
  zone_s_28d: number[];
  week_streak: number;
  ftp: number;
  wkg: number;
  weekly_goal_hours: number;
}

// ---- devices ----
export type DeviceKind = "trainer" | "heart_rate" | "power";
export interface DeviceInfo { id: string; name: string; rssi: number | null; kind: DeviceKind; ftms: boolean }
export type LinkState = "disconnected" | "connecting" | "connected" | "reconnecting";
export interface Link {
  state: LinkState;
  device_id: string | null;
  name: string | null;
  detail: string | null;
  power_range: [number, number] | null;
}
export interface DeviceStatus {
  available: boolean;
  error: string | null;
  scanning: boolean;
  trainer: Link;
  hrm: Link;
  simulator: boolean;
}
export interface LiveTelemetry {
  source: "simulator" | "bluetooth";
  connected: boolean;
  power_w: number;
  cadence_rpm: number;
  heart_rate: number | null;
}
export interface SimSettings { enabled: boolean; effort_w: number }

// ---- live ride ----
export type ControlMode = "sim" | "erg" | "level";
export type FreeMode = "gradient" | "power" | "level" | "heart_rate";

export interface FreeOptions {
  mode: FreeMode;
  grade_pct: number;
  watts: number;
  level_pct: number;
  target_hr: number;
  terrain: Terrain | null;
  seed: number;
}

export type Goal =
  | { type: "time"; seconds: number }
  | { type: "distance"; meters: number }
  | { type: "energy"; kj: number }
  | { type: "calories"; kcal: number }
  | { type: "elevation"; meters: number };

export type SectorKind = "climb" | "descent" | "flat" | "rollers" | "sprint" | "custom" | "terrain";

export interface SectorSpec {
  kind: SectorKind;
  name?: string | null;
  length_m: number;
  grade_pct: number;
  segments?: SegmentSpec[];
  terrain?: Terrain | null;
  in_m: number;
  at_end: boolean;
  rejoin: boolean;
  timed: boolean;
}

export interface ActiveSector {
  id: number;
  name: string;
  kind: SectorKind;
  start_total: number;
  end_total: number;
  start_d: number;
  end_d: number;
  length_m: number;
  avg_grade_pct: number;
  timed: boolean;
  started_idx: number | null;
  done: boolean;
  result: Split | null;
}

export interface SectorSnapshot {
  id: number;
  name: string;
  kind: SectorKind;
  distance_to_m: number;
  length_m: number;
  avg_grade_pct: number;
  progress: number;
  active: boolean;
  done: boolean;
  timed: boolean;
  elapsed_s: number;
  result: Split | null;
}

export interface GoalSnapshot {
  goal: Goal;
  label: string;
  progress: number;
  remaining: string;
  done: boolean;
}

export interface StepSnapshot {
  index: number;
  label: string;
  elapsed_s: number;
  remaining_s: number;
  target_w: number | null;
  grade_pct: number | null;
  effort: boolean;
  cadence: number | null;
  next_label: string | null;
  next_target_w: number | null;
}

export interface ClimbSnapshot {
  index: number;
  category: string;
  length_m: number;
  gain_m: number;
  avg_grade_pct: number;
  distance_to_m: number;
  progress: number;
  remaining_m: number;
  remaining_gain_m: number;
}

export interface Snapshot {
  kind: RideKind;
  plan_version: number;
  endless: boolean;
  route_modified: boolean;
  structured: boolean;
  free_mode: FreeMode;
  free_watts: number;
  free_level_pct: number;
  hr_target_bpm: number;
  hr_control_w: number;
  goal: GoalSnapshot | null;
  split_count: number;
  split_elapsed_s: number;
  split_distance_m: number;
  split_avg_power: number;
  recent_splits: Split[];
  sectors: SectorSnapshot[];
  can_undo_sector: boolean;
  notice: string | null;
  elapsed_s: number;
  moving_s: number;
  paused: boolean;
  finished: boolean;
  trainer_connected: boolean;
  distance_m: number;
  lap: number;
  laps: number;
  lap_distance_m: number;
  route_distance_m: number | null;
  route_progress: number | null;
  remaining_m: number | null;
  route_complete: boolean;
  route_time_s: number | null;
  ele_m: number;
  grade_pct: number;
  trainer_grade_pct: number;
  mode: ControlMode;
  target_w: number | null;
  trainer_difficulty: number;
  speed_kmh: number;
  power_w: number;
  power_3s: number;
  cadence_rpm: number;
  heart_rate: number | null;
  hr_zone: number | null;
  hr_zone_count: number;
  avg_power: number;
  max_power: number;
  np: number;
  work_kj: number;
  calories_kcal: number;
  elevation_gain_m: number;
  wkg_3s: number;
  tss: number;
  step: StepSnapshot | null;
  workout_t: number;
  workout_total_s: number | null;
  workout_complete: boolean;
  erg_bias_pct: number;
  erg_disabled: boolean;
  free_grade_pct: number;
  free_erg_w: number | null;
  ghost_gap_s: number | null;
  ghost_distance_m: number | null;
  current_climb: ClimbSnapshot | null;
  next_climb: ClimbSnapshot | null;
  test_status: string | null;
  test_done: boolean;
  virtual_shifting: boolean;
  shifting_active: boolean;
  gear: number;
  gear_count: number;
  gear_ratio: number;
  events: { kind: string; message: string }[];
}

export interface RideInfo {
  kind: RideKind;
  name: string;
  started_at: string;
  route: Route | null;
  route_stats: RouteStats | null;
  laps: number;
  steps: Step[];
  ftp: number;
  weight_kg: number;
  workout_name: string | null;
  test: TestKind | null;
  ghost: GhostTrace | null;
  event_id: string | null;
  free: FreeOptions | null;
  goal: Goal | null;
  sectors: ActiveSector[];
  plan_version: number;
  endless: boolean;
}

export type RideAction =
  | { type: "erg_bias"; delta_pct: number }
  | { type: "skip_step" }
  | { type: "toggle_erg" }
  | { type: "grade"; delta_pct: number }
  | { type: "free_erg"; watts: number | null }
  | { type: "difficulty"; delta_pct: number }
  | { type: "shift"; delta: number }
  | { type: "set_free_mode"; mode: FreeMode }
  | { type: "free_watts"; delta_w: number }
  | { type: "level"; delta_pct: number }
  | { type: "hr_target"; delta_bpm: number }
  | { type: "set_goal"; goal: Goal | null }
  | { type: "lap" }
  | { type: "add_laps"; delta: number }
  | { type: "add_sector"; sector: SectorSpec }
  | { type: "undo_sector" }
  | { type: "extend_step"; seconds: number }
  | { type: "adjust_step"; delta_pct: number }
  | { type: "insert_blocks"; blocks: Block[]; at_end: boolean }
  | { type: "remove_step"; index: number }
  | { type: "one_more_rep" }
  | { type: "skip_to_cooldown" }
  | { type: "end_workout" };

export interface StartRequest {
  kind: RideKind;
  route_id?: string | null;
  laps?: number;
  workout_id?: string | null;
  test?: TestKind | null;
  event_id?: string | null;
  ghost?: boolean;
  name?: string;
  free?: FreeOptions | null;
  goal?: Goal | null;
}
