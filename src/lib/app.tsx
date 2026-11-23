import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, useTauriEvent } from "./api";
import type { DeviceStatus, FtpAssessment, Profile, StartRequest, StravaUploadEvent } from "./types";

export type Page =
  | "home"
  | "ride"
  | "routes"
  | "route"
  | "route-new"
  | "workouts"
  | "workout-edit"
  | "events"
  | "tests"
  | "activities"
  | "activity"
  | "stats"
  | "settings"
  | "devices"
  | "live";

export interface Nav {
  page: Page;
  params: Record<string, any>;
}

interface Toast {
  id: number;
  text: string;
  kind: "info" | "good" | "warn" | "bad";
}

interface Ctx {
  profile: Profile | null;
  setProfile: (p: Profile) => void;
  nav: Nav;
  go: (page: Page, params?: Record<string, any>) => void;
  back: () => void;
  toast: (text: string, kind?: Toast["kind"]) => void;
  toasts: Toast[];
  devices: DeviceStatus | null;
  refreshDevices: () => void;
  /** Launch the ride setup page pre-filled. */
  prepareRide: (req: StartRequest) => void;
  /** Latest automatic FTP assessment (eFTP). */
  ftpAssessment: FtpAssessment | null;
  setFtpAssessment: (a: FtpAssessment | null) => void;
}

const AppCtx = createContext<Ctx | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [profile, setProfile] = useState<Profile | null>(null);
  const [history, setHistory] = useState<Nav[]>([{ page: "home", params: {} }]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [devices, setDevices] = useState<DeviceStatus | null>(null);
  const [ftpAssessment, setFtpAssessment] = useState<FtpAssessment | null>(null);

  useEffect(() => {
    api.getProfile().then(setProfile).catch(console.error);
    api.ftpAssessment().then(setFtpAssessment).catch(console.error);
    api.rideInfo().then((info) => info && setHistory([{ page: "live", params: {} }]));
  }, []);

  const refreshDevices = useCallback(() => {
    api.bleStatus().then(setDevices).catch(console.error);
  }, []);
  useEffect(refreshDevices, [refreshDevices]);
  useTauriEvent<Omit<DeviceStatus, "simulator">>("ble://status", (s) =>
    setDevices((d) => ({ ...s, simulator: d?.simulator ?? false }))
  );

  const go = useCallback((page: Page, params: Record<string, any> = {}) => {
    setHistory((h) => [...h.slice(-30), { page, params }]);
  }, []);
  const back = useCallback(() => setHistory((h) => (h.length > 1 ? h.slice(0, -1) : h)), []);

  const toast = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-4), { id, text, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500);
  }, []);

  useTauriEvent<StravaUploadEvent>("strava://upload", (e) => {
    if (e.status === "uploading") toast("Uploading to Strava…");
    else if (e.status === "done") toast("Uploaded to Strava ✓", "good");
    else if (e.status === "error") toast(`Strava: ${e.error}`, "bad");
  });

  useTauriEvent<Profile>("profile://changed", setProfile);
  useTauriEvent<FtpAssessment>("ftp://assessment", (a) => {
    setFtpAssessment(a);
    if (a.auto_applied && a.suggested_ftp) toast(`FTP raised to ${a.suggested_ftp} W from your riding`, "good");
    else if (!a.dismissed && a.trigger_ride_id && (a.suggestion === "increase" || a.suggestion === "decrease"))
      toast(`Your riding suggests an FTP of ${a.suggested_ftp} W`, "info");
  });

  const prepareRide = useCallback((req: StartRequest) => go("ride", { req }), [go]);

  const value = useMemo<Ctx>(
    () => ({
      profile,
      setProfile,
      nav: history[history.length - 1],
      go,
      back,
      toast,
      toasts,
      devices,
      refreshDevices,
      prepareRide,
      ftpAssessment,
      setFtpAssessment,
    }),
    [profile, history, go, back, toast, toasts, devices, refreshDevices, prepareRide, ftpAssessment]
  );
  return <AppCtx.Provider value={value}>{children}</AppCtx.Provider>;
}

export function useApp(): Ctx {
  const c = useContext(AppCtx);
  if (!c) throw new Error("useApp outside provider");
  return c;
}

export function useUnits() {
  return useApp().profile?.units ?? "metric";
}

/** Load async data with reload support. */
export function useLoad<T>(fn: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let alive = true;
    fn()
      .then((d) => alive && (setData(d), setError(null)))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  return { data, error, reload: () => setTick((t) => t + 1), setData };
}
