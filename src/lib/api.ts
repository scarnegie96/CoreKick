import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import type * as T from "./types";

// Tauri 2 maps camelCase JS argument names to snake_case Rust parameters.
export const api = {
  getProfile: () => invoke<T.Profile>("get_profile"),
  saveProfile: (profile: T.Profile) => invoke<T.Profile>("save_profile", { profile }),
  setFtp: (ftp: number, source: string) => invoke<T.Profile>("set_ftp", { ftp, source }),

  listRoutes: () => invoke<T.RouteSummary[]>("list_routes"),
  getRoute: (id: string) => invoke<T.RouteDetail>("get_route", { id }),
  saveRoute: (route: T.Route) => invoke<T.Route>("save_route", { route }),
  deleteRoute: (id: string) => invoke<void>("delete_route", { id }),
  toggleFavouriteRoute: (id: string) => invoke<boolean>("toggle_favourite_route", { id }),
  generateRoute: (params: T.GenParams, name: string) => invoke<T.RouteDetail>("generate_route", { params, name }),
  buildRoute: (segments: T.SegmentSpec[], name: string, startEle: number) =>
    invoke<T.RouteDetail>("build_route", { segments, name, startEle }),
  importGpx: (text: string, name?: string) => invoke<T.RouteDetail>("import_gpx", { text, name: name ?? null }),
  suggestedGain: (terrain: T.Terrain, distanceKm: number) => invoke<number>("suggested_gain", { terrain, distanceKm }),

  listWorkouts: () => invoke<T.WorkoutView[]>("list_workouts"),
  saveWorkout: (workout: T.Workout) => invoke<T.WorkoutView>("save_workout", { workout }),
  deleteWorkout: (id: string) => invoke<void>("delete_workout", { id }),
  previewWorkout: (workout: T.Workout) => invoke<T.WorkoutView>("preview_workout", { workout }),
  parseWorkoutText: (text: string) => invoke<T.Block[]>("parse_workout_text", { text }),
  workoutToText: (blocks: T.Block[]) => invoke<string>("workout_to_text", { blocks }),
  importWorkout: (fileName: string, text: string) => invoke<T.WorkoutView>("import_workout", { fileName, text }),
  exportWorkout: (id: string, path: string) => invoke<string>("export_workout", { id, path }),
  listTests: () => invoke<T.TestView[]>("list_tests"),

  listEvents: () => invoke<T.EventView[]>("list_events"),
  saveEvent: (event: T.RideEvent) => invoke<T.EventView>("save_event", { event }),
  deleteEvent: (id: string) => invoke<void>("delete_event", { id }),
  upcomingEvents: (days: number) => invoke<T.Occurrence[]>("upcoming_events", { days }),
  eventRides: (id: string) => invoke<T.RideSummary[]>("event_rides", { id }),

  listRides: () => invoke<T.RideSummary[]>("list_rides"),
  getRide: (id: string) => invoke<T.RideRecord>("get_ride", { id }),
  deleteRide: (id: string) => invoke<void>("delete_ride", { id }),
  updateRide: (id: string, name: string, notes: string) => invoke<T.RideSummary>("update_ride", { id, name, notes }),
  exportRide: (id: string, format: "tcx" | "csv" | "fit", path: string) => invoke<string>("export_ride", { id, format, path }),
  accountStats: () => invoke<T.AccountStats>("account_stats"),

  bleStatus: () => invoke<T.DeviceStatus>("ble_status"),
  bleDevices: () => invoke<T.DeviceInfo[]>("ble_devices"),
  bleScan: (seconds = 20) => invoke<void>("ble_scan", { seconds }),
  bleStopScan: () => invoke<void>("ble_stop_scan"),
  bleConnect: (id: string, role: "trainer" | "hrm") => invoke<void>("ble_connect", { id, role }),
  bleDisconnect: (role: "trainer" | "hrm") => invoke<void>("ble_disconnect", { role }),
  simGet: () => invoke<T.SimSettings>("sim_get"),
  simSet: (settings: T.SimSettings) => invoke<T.SimSettings>("sim_set", { settings }),

  hrZones: () => invoke<T.HrZone[]>("hr_zones"),
  ftpAssessment: () => invoke<T.FtpAssessment | null>("ftp_assessment"),
  ftpReassess: () => invoke<T.FtpAssessment | null>("ftp_reassess"),
  ftpApplySuggestion: () => invoke<T.Profile>("ftp_apply_suggestion"),
  ftpDismiss: () => invoke<void>("ftp_dismiss"),
  stravaStatus: () => invoke<T.StravaStatus>("strava_status"),
  stravaSaveClient: (clientId: string, clientSecret: string) => invoke<T.StravaStatus>("strava_save_client", { clientId, clientSecret }),
  stravaConnect: () => invoke<T.StravaStatus>("strava_connect"),
  stravaDisconnect: () => invoke<T.StravaStatus>("strava_disconnect"),
  stravaSetAuto: (on: boolean) => invoke<T.StravaStatus>("strava_set_auto", { on }),
  stravaUpload: (id: string) => invoke<number>("strava_upload", { id }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),

  rideStart: (req: T.StartRequest) => invoke<T.RideInfo>("ride_start", { req }),
  rideInfo: () => invoke<T.RideInfo | null>("ride_info"),
  ridePause: () => invoke<void>("ride_pause"),
  rideResume: () => invoke<void>("ride_resume"),
  rideAction: (action: T.RideAction) => invoke<void>("ride_action", { action }),
  rideEnd: (save: boolean, saveRoute = false) => invoke<T.RideSummary | null>("ride_end", { save, saveRoute }),
};

/** Subscribe to a backend event for the component's lifetime. */
export function useTauriEvent<P>(name: string, handler: (payload: P) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    let un: UnlistenFn | undefined;
    let cancelled = false;
    listen<P>(name, (e) => ref.current(e.payload)).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [name]);
}

export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
