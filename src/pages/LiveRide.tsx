import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errMsg, useTauriEvent } from "../lib/api";
import { useApp, useUnits } from "../lib/app";
import { categoryColor, dist, distUnit, dur, elev, elevUnit, gradeColor, hrZoneColor, num, short, speed, speedUnit, zoneColor, ZONE_NAMES, zoneIndex } from "../lib/format";
import type { FreeMode, Goal, HrAlertSettings, HrZone, RideAction, RideInfo, SectorKind, SectorSpec, SimSettings, Snapshot } from "../lib/types";
import { playCue, speak, unlockAudio } from "../lib/audio";
import { ElevationProfile, LineChart, WorkoutChart } from "../components/charts";
import { Btn, Field, Modal, NumberInput, Segmented, Slider, Stat, Toggle } from "../components/ui";
import { IconBell, IconBellOff, IconEdit, IconFlag, IconFlame, IconGhost, IconHeart, IconMinus, IconMountain, IconPause, IconPlay, IconPlus, IconSkip, IconStop, IconTrophy, IconX } from "../components/icons";
import { TEXT_HELP } from "./Workouts";

const HISTORY_S = 300;

const FREE_MODE_LABEL: Record<FreeMode, string> = { gradient: "Gradient", power: "ERG", level: "Level", heart_rate: "Heart rate" };

const DEFAULT_ALERTS: HrAlertSettings = { enabled: false, low_zone: 2, high_zone: 3, alert_high: true, alert_low: true, grace_s: 300, repeat_s: 30, voice: true, volume: 0.7 };

type AlertState = "in" | "high" | "low";

/**
 * Audio alerts when heart rate leaves the chosen zone range. High alerts fire
 * after 5 s out of range, low alerts after 10 s; both repeat every `repeat_s`
 * while still out, and a soft chime confirms you're back in range.
 */
function useHrAlerts(snap: Snapshot | null, zones: HrZone[], cfg: HrAlertSettings) {
  const st = useRef<{ state: AlertState; since: number; last: number; alerted: boolean }>({ state: "in", since: 0, last: 0, alerted: false });
  const [status, setStatus] = useState<AlertState>("in");
  useEffect(() => {
    const r = st.current;
    const reset = () => {
      r.state = "in";
      r.alerted = false;
      setStatus("in");
    };
    if (!snap || !cfg.enabled || snap.paused || snap.heart_rate == null || snap.hr_zone == null || snap.elapsed_s < cfg.grace_s) return reset();
    const now = snap.elapsed_s;
    const z = snap.hr_zone;
    let next: AlertState = z > cfg.high_zone ? "high" : z < cfg.low_zone ? "low" : "in";
    if ((next === "high" && !cfg.alert_high) || (next === "low" && !cfg.alert_low)) next = "in";
    if (next !== r.state) {
      if (next === "in" && r.alerted) {
        playCue("back", cfg.volume);
        if (cfg.voice) speak(`Back in zone ${z}`, cfg.volume);
      }
      r.state = next;
      r.since = now;
      r.alerted = false;
      setStatus(next);
    }
    if (next === "in") return;
    const wait = next === "high" ? 5 : 10;
    if (now - r.since >= wait && (!r.alerted || now - r.last >= cfg.repeat_s)) {
      r.alerted = true;
      r.last = now;
      playCue(next, cfg.volume);
      if (cfg.voice) {
        const name = zones.find((x) => x.zone === z)?.name ?? "";
        speak(next === "high" ? `Heart rate high. ${Math.round(snap.heart_rate)}, zone ${z} ${name}. Ease off.` : `Heart rate low. ${Math.round(snap.heart_rate)}. Push on.`, cfg.volume);
      }
    }
  }, [snap, zones, cfg]);
  return status;
}

/** Horizontal heart-rate zone gauge with the allowed range bracketed. */
function HrGauge({ hr, zones, alerts, status, onToggle }: { hr: number | null; zones: HrZone[]; alerts: HrAlertSettings; status: AlertState; onToggle: () => void }) {
  if (!zones.length) return null;
  const lo = Math.max(60, zones[0].lo - 10);
  const top = zones[zones.length - 1];
  const hi = (top.hi ?? top.lo + 15) + 5;
  const pct = (v: number) => Math.max(0, Math.min(100, ((v - lo) / (hi - lo)) * 100));
  const zLo = zones.find((z) => z.zone === alerts.low_zone);
  const zHi = zones.find((z) => z.zone === alerts.high_zone);
  const bandA = zLo ? pct(zLo.lo) : 0;
  const bandB = zHi ? pct(zHi.hi ?? hi) : 100;
  return (
    <div className={`hr-gauge ${status !== "in" ? "alert-" + status : ""}`}>
      <div className="hg-top">
        <span>Heart-rate zones</span>
        <button className={`hg-bell ${alerts.enabled ? "on" : ""}`} onClick={onToggle} title="Toggle heart-rate alerts">
          {alerts.enabled ? <IconBell size={13} /> : <IconBellOff size={13} />} {alerts.enabled ? `Z${alerts.low_zone}–Z${alerts.high_zone}` : "Alerts off"}
        </button>
      </div>
      <div className="hg-bar">
        {zones.map((z) => (
          <i key={z.zone} style={{ left: `${pct(z.lo)}%`, width: `${pct(z.hi ?? hi) - pct(z.lo)}%`, background: hrZoneColor(z.zone, zones.length) }} />
        ))}
        {alerts.enabled && <b className="hg-band" style={{ left: `${bandA}%`, width: `${bandB - bandA}%` }} />}
        {hr != null && <em style={{ left: `${pct(hr)}%` }} />}
      </div>
      <div className="hg-labels">
        {zones.map((z) => (
          <span key={z.zone} style={{ left: `${(pct(z.lo) + pct(z.hi ?? hi)) / 2}%` }}>Z{z.zone}</span>
        ))}
      </div>
    </div>
  );
}

export default function LiveRide() {
  const { go, toast, profile, devices, refreshDevices } = useApp();
  const units = useUnits();
  const [info, setInfo] = useState<RideInfo | null>(null);
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [ending, setEnding] = useState(false);
  const [saveRoute, setSaveRoute] = useState(false);
  const [saving, setSaving] = useState(false);
  const [sim, setSim] = useState<SimSettings | null>(null);
  const [drawer, setDrawer] = useState(false);
  const hist = useRef<{ t: number[]; p: number[]; target: (number | null)[]; hr: (number | null)[] }>({ t: [], p: [], target: [], hr: [] });
  const [histVer, setHistVer] = useState(0);
  const [banner, setBanner] = useState<string | null>(null);
  const planVer = useRef(0);
  const [zones, setZones] = useState<HrZone[]>([]);
  const [alerts, setAlerts] = useState<HrAlertSettings>(profile?.hr_alerts ?? DEFAULT_ALERTS);
  const alertsInit = useRef(false);
  useEffect(() => {
    if (profile && !alertsInit.current) {
      alertsInit.current = true;
      setAlerts(profile.hr_alerts ?? DEFAULT_ALERTS);
    }
  }, [profile]);
  useEffect(() => {
    api.hrZones().then(setZones);
  }, [profile]);
  // Browsers only allow audio after a user gesture.
  useEffect(() => {
    const u = () => unlockAudio();
    window.addEventListener("pointerdown", u);
    window.addEventListener("keydown", u);
    return () => {
      window.removeEventListener("pointerdown", u);
      window.removeEventListener("keydown", u);
    };
  }, []);

  const loadInfo = useCallback(() => {
    api.rideInfo().then((i) => {
      if (!i) go("home");
      else {
        planVer.current = i.plan_version;
        setInfo(i);
      }
    });
  }, [go]);

  useEffect(() => {
    loadInfo();
    api.simGet().then(setSim);
    refreshDevices();
  }, [loadInfo, refreshDevices]);

  useTauriEvent<Snapshot>("ride://tick", (s) => {
    setSnap(s);
    if (s.plan_version !== planVer.current) {
      planVer.current = s.plan_version;
      loadInfo();
    }
    const h = hist.current;
    if (!s.paused) {
      h.t.push(s.elapsed_s);
      h.p.push(s.power_w);
      h.target.push(s.target_w);
      h.hr.push(s.heart_rate);
      const cut = s.elapsed_s - HISTORY_S;
      let i = 0;
      while (i < h.t.length && h.t[i] < cut) i++;
      if (i > 0) {
        h.t.splice(0, i);
        h.p.splice(0, i);
        h.target.splice(0, i);
        h.hr.splice(0, i);
      }
      setHistVer((v) => v + 1);
    }
    for (const e of s.events) {
      const kind = ["route_complete", "workout_complete", "test_complete", "goal"].includes(e.kind) ? "good" : e.kind === "climb" ? "warn" : "info";
      toast(e.message, kind);
      if (kind === "good") setBanner(e.message);
    }
  });

  const act = useCallback((a: RideAction) => api.rideAction(a).catch((e) => toast(errMsg(e), "bad")), [toast]);
  const togglePause = useCallback(() => {
    if (!snap) return;
    (snap.paused ? api.rideResume() : api.ridePause()).catch((e) => toast(errMsg(e), "bad"));
  }, [snap, toast]);

  const structured = snap ? snap.structured : (info?.kind === "workout" || info?.kind === "test");
  const hrStatus = useHrAlerts(snap, zones, alerts);
  const toggleAlerts = () => {
    unlockAudio();
    setAlerts((a) => ({ ...a, enabled: !a.enabled }));
    toast(alerts.enabled ? "Heart-rate alerts off" : `Heart-rate alerts on: Z${alerts.low_zone}–Z${alerts.high_zone}`);
  };
  const isFree = info?.kind === "free";

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (["]", "=", "+", "PageUp"].includes(e.key)) {
        e.preventDefault();
        act({ type: "shift", delta: 1 });
        return;
      }
      if (["[", "-", "_", "PageDown"].includes(e.key)) {
        e.preventDefault();
        act({ type: "shift", delta: -1 });
        return;
      }
      if (e.code === "Space") {
        e.preventDefault();
        togglePause();
      } else if (e.key === "h" || e.key === "H") {
        toggleAlerts();
      } else if (e.key === "l" || e.key === "L") {
        act({ type: "lap" });
      } else if (e.key === "e" || e.key === "E") {
        setDrawer((d) => !d);
      } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
        e.preventDefault();
        const up = e.key === "ArrowUp" ? 1 : -1;
        if (structured) act({ type: "erg_bias", delta_pct: up });
        else if (isFree) {
          const m = snap?.free_mode ?? "gradient";
          if (m === "power") act({ type: "free_watts", delta_w: up * 5 });
          else if (m === "level") act({ type: "level", delta_pct: up * 2 });
          else if (m === "heart_rate") act({ type: "hr_target", delta_bpm: up });
          else act({ type: "grade", delta_pct: up * 0.5 });
        } else act({ type: "difficulty", delta_pct: up * 5 });
      } else if (e.key === "ArrowRight" && structured) {
        act({ type: "skip_step" });
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [togglePause, act, structured, isFree, snap, alerts]);

  const finish = async (save: boolean) => {
    setSaving(true);
    try {
      const s = await api.rideEnd(save, saveRoute);
      if (s) {
        toast(s.personal_best ? "New personal best! 🏆" : "Ride saved", "good");
        go("activity", { id: s.id, fresh: true });
      } else {
        toast(save ? "Ride too short to save" : "Ride discarded");
        go("home");
      }
    } catch (e) {
      toast(errMsg(e), "bad");
      setSaving(false);
    }
  };

  const setEffort = (w: number) => {
    if (!sim) return;
    const next = { ...sim, effort_w: w };
    setSim(next);
    api.simSet(next);
  };

  const ftp = info?.ftp ?? profile?.ftp ?? 200;
  const route = info?.route;
  const pts = route?.points ?? [];
  const routeLen = snap?.route_distance_m ?? 0;
  const lapD = snap?.lap_distance_m ?? 0;
  const ghostLapD = snap?.ghost_distance_m != null && routeLen ? snap.ghost_distance_m % routeLen : null;
  const lookWin: [number, number] | undefined = route ? [Math.max(0, Math.min(lapD - 250, routeLen - 2250)), Math.min(routeLen, Math.max(lapD - 250, 0) + 2250)] : undefined;
  const sectorBands = useMemo(() => (info?.sectors ?? []).map((s) => ({ start: s.start_d, end: s.end_d, label: s.name, done: s.done })), [info]);
  // For long endless routes show a 20 km window around the rider instead of the whole loop.
  const fullWin: [number, number] | undefined = route && snap?.endless && routeLen > 25000 ? [Math.max(0, Math.min(lapD - 5000, routeLen - 20000)), Math.min(routeLen, Math.max(lapD - 5000, 0) + 20000)] : undefined;

  const powerChart = useMemo(() => {
    const h = hist.current;
    return {
      x: h.t.slice(),
      series: [
        { name: "Power", values: h.p.slice(), color: "var(--accent)", area: true, width: 1.8 },
        ...(h.target.some((v) => v != null) ? [{ name: "Target", values: h.target.slice(), color: "var(--ink)", dashed: true, width: 1.4 }] : []),
        ...(h.hr.some((v) => v != null) ? [{ name: "HR", values: h.hr.slice(), color: "#E5466B", width: 1.2 }] : []),
      ],
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [histVer]);

  if (!info) return <div className="live loading">Loading ride…</div>;
  const s = snap;
  const p3 = s?.power_3s ?? 0;
  const zi = zoneIndex(p3, ftp);
  const pColor = zoneColor(p3, ftp);
  const gap = s?.ghost_gap_s;
  const step = s?.step;
  const usingSim = devices?.simulator ?? sim?.enabled;
  const showWorkout = info.steps.length > 0 && (structured || info.kind === "workout" || info.kind === "test");
  const upcomingSectors = (s?.sectors ?? []).filter((x) => !x.done);
  const shownSector = upcomingSectors.find((x) => x.active) ?? [...upcomingSectors].sort((a, b) => a.distance_to_m - b.distance_to_m)[0];
  const modeChip = s?.shifting_active
    ? `SIM ${(s?.trainer_grade_pct ?? 0).toFixed(1)}% · Gear ${s.gear}`
    : s?.mode === "erg"
      ? `ERG ${num(s?.target_w)} W`
      : s?.mode === "level"
        ? `LEVEL ${num(s?.free_level_pct)}%`
        : `SIM ${(s?.trainer_grade_pct ?? 0).toFixed(1)}%`;

  return (
    <div className={`live ${s?.paused ? "paused" : ""} ${drawer ? "with-drawer" : ""}`}>
      <header className="live-top" data-tauri-drag-region>
        <div className="live-title">
          <h2>{info.name}</h2>
          <div className="row gap8 center">
            <span className={`mode-chip ${s?.mode ?? "sim"}`}>{modeChip}</span>
            {info.kind === "route" && (s?.laps ?? 1) > 1 && <span className="muted">Lap {s?.lap ?? 1}/{s?.laps}</span>}
            {s?.route_modified && <span className="muted small">edited route</span>}
            <span className={`conn ${s?.trainer_connected ? "ok" : "bad"}`}>{s?.trainer_connected ? (usingSim ? "Simulator" : "Trainer live") : "No trainer data"}</span>
          </div>
        </div>
        <div className="live-clock">{dur(s?.elapsed_s ?? 0, { hours: (s?.elapsed_s ?? 0) >= 3600 })}</div>
        <div className="row gap8">
          <Btn kind={drawer ? "primary" : "default"} onClick={() => setDrawer((d) => !d)} title="Edit the ride (E)">
            <IconEdit size={14} /> Edit ride
          </Btn>
          <Btn kind={s?.paused ? "primary" : "default"} onClick={togglePause}>
            {s?.paused ? <><IconPlay size={14} /> Resume</> : <><IconPause size={14} /> Pause</>}
          </Btn>
          <Btn kind="danger" onClick={() => setEnding(true)}>
            <IconStop size={14} /> End
          </Btn>
        </div>
      </header>

      {s?.goal && (
        <div className={`goal-bar ${s.goal.done ? "done" : ""}`}>
          <span><IconFlag size={13} /> Goal {s.goal.label}</span>
          <div className="gb-track"><i style={{ width: `${s.goal.progress * 100}%` }} /></div>
          <span className="num">{s.goal.done ? "Done! 🎉" : s.goal.remaining}</span>
        </div>
      )}

      <div className="live-body">
        <section className="live-metrics">
          <div className="power-hero" style={{ ["--zc" as any]: pColor }}>
            <div className="ph-l">Power · 3s</div>
            <div className="ph-v">{Math.round(p3)}<span>W</span></div>
            <div className="ph-z">
              Z{zi + 1} {ZONE_NAMES[zi]} · {(s?.wkg_3s ?? 0).toFixed(2)} W/kg
            </div>
            {s?.target_w != null && (
              <div className="ph-t">
                {s.free_mode === "heart_rate" && isFree && !structured ? <>HR control <b>{Math.round(s.target_w)} W</b></> : <>Target <b>{Math.round(s.target_w)} W</b></>}
                <span className={Math.abs(p3 - s.target_w) / Math.max(1, s.target_w) < 0.05 ? "ok" : "off"}>{p3 - s.target_w >= 0 ? "+" : ""}{Math.round(p3 - s.target_w)}</span>
              </div>
            )}
          </div>
          {s?.notice && <div className="banner warn small">{s.notice}</div>}
          <HrGauge hr={s?.heart_rate ?? null} zones={zones} alerts={alerts} status={hrStatus} onToggle={toggleAlerts} />
          <div className="mgrid">
            <Stat label="Cadence" value={Math.round(s?.cadence_rpm ?? 0)} unit="rpm" color={step?.cadence && Math.abs((s?.cadence_rpm ?? 0) - step.cadence) > 8 ? "var(--warn)" : undefined} sub={step?.cadence ? `target ${step.cadence}` : undefined} />
            <Stat
              label={<><IconHeart size={12} /> Heart rate</>}
              value={s?.heart_rate ? Math.round(s.heart_rate) : "–"}
              unit="bpm"
              color={s?.hr_zone ? hrZoneColor(s.hr_zone, s.hr_zone_count) : "#E5466B"}
              sub={
                isFree && s?.free_mode === "heart_rate"
                  ? `target ${Math.round(s.hr_target_bpm)}`
                  : s?.hr_zone
                    ? `Z${s.hr_zone} ${zones.find((z) => z.zone === s.hr_zone)?.name ?? ""}`
                    : "no sensor"
              }
            />
            <Stat label="Speed" value={speed(s?.speed_kmh ?? 0, units)} unit={speedUnit(units)} />
            <Stat label="Grade" value={(s?.grade_pct ?? 0).toFixed(1)} unit="%" color={gradeColor(s?.grade_pct ?? 0)} />
            <Stat label="Distance" value={dist(s?.distance_m ?? 0, units, 2)} unit={distUnit(units)} sub={s?.remaining_m != null ? `${short(s.remaining_m, units)} to go` : undefined} />
            <Stat label={<><IconMountain size={12} /> Climbed</>} value={elev(s?.elevation_gain_m ?? 0, units)} unit={elevUnit(units)} />
            <Stat label="Avg power" value={Math.round(s?.avg_power ?? 0)} unit="W" sub={`NP ${Math.round(s?.np ?? 0)} W`} />
            <Stat label="Energy" value={Math.round(s?.work_kj ?? 0)} unit="kJ" sub={<><IconFlame size={11} /> {Math.round(s?.calories_kcal ?? 0)} kcal</>} />
            <Stat
              label={`Lap ${(s?.split_count ?? 0) + 1}`}
              value={dur(s?.split_elapsed_s ?? 0)}
              sub={`${short(s?.split_distance_m ?? 0, units)} · ${Math.round(s?.split_avg_power ?? 0)} W`}
            />
            {gap != null ? (
              <Stat label={<><IconGhost size={12} /> vs best</>} value={`${gap > 0 ? "+" : "−"}${dur(Math.abs(gap))}`} color={gap > 0 ? "var(--bad)" : "var(--accent)"} sub={gap > 0 ? "behind" : "ahead"} />
            ) : (
              <Stat label="TSS" value={Math.round(s?.tss ?? 0)} sub={`IF ${((s?.np ?? 0) / ftp).toFixed(2)}`} />
            )}
          </div>
        </section>

        <section className="live-main">
          {banner && (
            <div className="done-banner">
              <IconTrophy size={18} /> {banner}
              <div className="grow" />
              <Btn size="sm" kind="primary" onClick={() => setEnding(true)}>Finish & save</Btn>
              <Btn size="sm" kind="ghost" onClick={() => setBanner(null)}>Keep riding</Btn>
            </div>
          )}

          {showWorkout && (
            <div className="panel interval">
              <div className="iv-head">
                <div>
                  <div className="iv-l">{step?.label ?? (s?.workout_complete ? "Plan complete – riding free" : "")}</div>
                  <div className="iv-t">{step ? dur(step.remaining_s) : "–"}</div>
                </div>
                <div className="iv-target">
                  {step?.target_w != null ? (
                    <>
                      <b style={{ color: zoneColor(step.target_w, ftp) }}>{Math.round(step.target_w)} W</b>
                      <span>{Math.round((step.target_w / ftp) * 100)}% FTP{s?.erg_disabled ? " · resistance mode" : ""}</span>
                    </>
                  ) : step ? (
                    <>
                      <b>Ride by feel</b>
                      <span>{step.effort ? "All out – this counts!" : "Easy"} · {step.grade_pct?.toFixed(0)}% virtual grade</span>
                    </>
                  ) : null}
                </div>
                <div className="iv-next">
                  {step?.next_label && (
                    <>
                      <span className="muted">Next</span>
                      <b>{step.next_label}</b>
                      <span>{step.next_target_w != null ? `${Math.round(step.next_target_w)} W` : "free"}</span>
                    </>
                  )}
                </div>
              </div>
              <WorkoutChart steps={info.steps} ftp={ftp} t={s?.workout_t} height={route ? 90 : 150} axis bias={(s?.erg_bias_pct ?? 100) / 100} />
              <div className="row between mt8 muted small">
                <span>{dur(s?.workout_t ?? 0)} / {dur(s?.workout_total_s ?? 0)}</span>
                {s?.test_status && <span className="accent">{s.test_status}</span>}
                <span>Intensity {Math.round(s?.erg_bias_pct ?? 100)}%</span>
              </div>
            </div>
          )}

          {route && (
            <div className="panel">
              <div className="row between center mb4">
                <span className="panel-t">{route.name}</span>
                <span className="muted small">
                  {snap?.endless ? `${short(s?.distance_m ?? 0, units)} ridden` : `${short(lapD, units)} / ${short(routeLen, units)}`}
                  {s?.route_progress != null && ` · ${Math.round(s.route_progress * 100)}%`}
                </span>
              </div>
              <ElevationProfile points={pts} window={fullWin} height={showWorkout ? 120 : 170} position={lapD} ghost={ghostLapD} climbs={info.route_stats?.climbs} sectors={sectorBands} units={units} progress />
            </div>
          )}

          {route && !showWorkout && (
            <div className="look">
              <div className="panel look-p">
                <div className="row between center mb4">
                  <span className="panel-t">Next 2 km</span>
                  <span className="grade-big" style={{ color: gradeColor(s?.grade_pct ?? 0) }}>
                    {(s?.grade_pct ?? 0).toFixed(1)}%
                  </span>
                </div>
                <ElevationProfile points={pts} window={lookWin} height={130} position={lapD} ghost={ghostLapD} sectors={sectorBands} units={units} minRange={25} progress axis interactive={false} labels={false} />
              </div>
              <div className="panel climb-card">
                {shownSector && (shownSector.active || shownSector.distance_to_m < 2000) ? (
                  <SectorCard sec={shownSector} units={units} />
                ) : s?.current_climb ? (
                  <>
                    <div className="cc-cat" style={{ color: categoryColor(s.current_climb.category) }}>{s.current_climb.category === "Uncategorised" ? "Climb" : s.current_climb.category}</div>
                    <div className="cc-big">{short(s.current_climb.remaining_m, units)}</div>
                    <div className="muted">to the top · {elev(s.current_climb.remaining_gain_m, units)} {elevUnit(units)} to go</div>
                    <div className="cc-bar"><i style={{ width: `${s.current_climb.progress * 100}%`, background: categoryColor(s.current_climb.category) }} /></div>
                    <div className="muted small">avg {s.current_climb.avg_grade_pct.toFixed(1)}% · {short(s.current_climb.length_m, units)}</div>
                  </>
                ) : s?.next_climb ? (
                  <>
                    <div className="muted small">Next climb in</div>
                    <div className="cc-big">{short(s.next_climb.distance_to_m, units)}</div>
                    <div className="cc-cat" style={{ color: categoryColor(s.next_climb.category) }}>
                      {s.next_climb.category === "Uncategorised" ? "Short climb" : s.next_climb.category}
                    </div>
                    <div className="muted small">
                      {short(s.next_climb.length_m, units)} at {s.next_climb.avg_grade_pct.toFixed(1)}% · {elev(s.next_climb.gain_m, units)} {elevUnit(units)}
                    </div>
                  </>
                ) : (
                  <>
                    <div className="muted small">{s?.route_complete ? "Route complete" : snap?.endless ? "Open road" : "No more climbs"}</div>
                    <div className="cc-big">{s?.route_complete ? "🏁" : snap?.endless ? "∞" : short(s?.remaining_m ?? 0, units)}</div>
                    {!s?.route_complete && !snap?.endless && <div className="muted">to the finish</div>}
                    <Btn size="sm" kind="subtle" className="mt8" onClick={() => setDrawer(true)}><IconPlus size={12} /> Add a climb</Btn>
                  </>
                )}
              </div>
            </div>
          )}

          {(s?.recent_splits.length ?? 0) > 0 && (
            <div className="panel">
              <div className="row between center mb4">
                <span className="panel-t">Splits</span>
                <span className="muted small">press L for a lap</span>
              </div>
              <table className="tbl compact splits">
                <tbody>
                  {s!.recent_splits.slice(0, 4).map((sp, i) => (
                    <tr key={i}>
                      <td><span className={`split-k ${sp.kind}`}>{sp.kind === "sector" ? "Sector" : sp.kind === "lap" ? "Lap" : "Route lap"}</span> {sp.name}</td>
                      <td className="num">{dur(sp.duration_s)}</td>
                      <td className="num">{short(sp.distance_m, units)}</td>
                      <td className="num">{Math.round(sp.avg_power)} W</td>
                      <td className="num muted">{sp.avg_hr ? `${Math.round(sp.avg_hr)} bpm` : ""}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          <div className="panel grow-panel">
            <div className="row between center mb4">
              <span className="panel-t">Last 5 minutes</span>
              <span className="muted small">avg {Math.round(s?.avg_power ?? 0)} W · max {Math.round(s?.max_power ?? 0)} W</span>
            </div>
            <LineChart x={powerChart.x} series={powerChart.series} height={route ? (showWorkout ? 130 : 150) : showWorkout ? 300 : 400} xFormat={(v) => dur(v)} yMin={0} />
          </div>
        </section>
      </div>

      <footer className="live-controls">
        {s?.virtual_shifting && (
          <div className={`ctl gear-ctl ${s.shifting_active ? "" : "idle"}`} title={s.shifting_active ? "Virtual shifting" : "Gears are inactive while ERG controls power"}>
            <span className="ctl-l">Gear</span>
            <button className="ctl-b" onClick={() => act({ type: "shift", delta: -1 })} aria-label="Shift down"><IconMinus size={16} /></button>
            <div className="gear-v">
              <b>{s.gear}</b>
              <span>/{s.gear_count}</span>
            </div>
            <button className="ctl-b" onClick={() => act({ type: "shift", delta: 1 })} aria-label="Shift up"><IconPlus size={16} /></button>
          </div>
        )}
        {structured && (
          <>
            <Ctl label="Intensity" value={`${Math.round(s?.erg_bias_pct ?? 100)}%`} onMinus={() => act({ type: "erg_bias", delta_pct: -1 })} onPlus={() => act({ type: "erg_bias", delta_pct: 1 })} />
            <Btn onClick={() => act({ type: "skip_step" })}>
              <IconSkip size={14} /> Skip
            </Btn>
            <Btn kind={s?.erg_disabled ? "primary" : "default"} onClick={() => act({ type: "toggle_erg" })}>
              {s?.erg_disabled ? "Back to ERG" : "Resistance"}
            </Btn>
          </>
        )}
        {!structured && isFree && s && (
          <>
            <Segmented<FreeMode>
              size="sm"
              value={s.free_mode}
              onChange={(m) => act({ type: "set_free_mode", mode: m })}
              options={(["gradient", "power", "level", "heart_rate"] as FreeMode[]).map((m) => ({ value: m, label: FREE_MODE_LABEL[m] }))}
            />
            {s.free_mode === "gradient" && (
              <Ctl label={route ? "Offset" : "Gradient"} value={`${s.free_grade_pct > 0 && route ? "+" : ""}${s.free_grade_pct.toFixed(1)}%`} onMinus={() => act({ type: "grade", delta_pct: -0.5 })} onPlus={() => act({ type: "grade", delta_pct: 0.5 })} />
            )}
            {s.free_mode === "power" && <Ctl label="Power" value={`${Math.round(s.free_watts)} W`} onMinus={() => act({ type: "free_watts", delta_w: -5 })} onPlus={() => act({ type: "free_watts", delta_w: 5 })} />}
            {s.free_mode === "level" && <Ctl label="Level" value={`${Math.round(s.free_level_pct)}%`} onMinus={() => act({ type: "level", delta_pct: -2 })} onPlus={() => act({ type: "level", delta_pct: 2 })} />}
            {s.free_mode === "heart_rate" && <Ctl label="Target HR" value={`${Math.round(s.hr_target_bpm)}`} onMinus={() => act({ type: "hr_target", delta_bpm: -1 })} onPlus={() => act({ type: "hr_target", delta_bpm: 1 })} />}
          </>
        )}
        {!structured && info.kind === "route" && (
          <Ctl label="Difficulty" value={`${Math.round((s?.trainer_difficulty ?? 1) * 100)}%`} onMinus={() => act({ type: "difficulty", delta_pct: -5 })} onPlus={() => act({ type: "difficulty", delta_pct: 5 })} />
        )}
        <Btn kind={alerts.enabled ? "subtle" : "default"} onClick={toggleAlerts} title="Heart-rate alerts (H)">
          {alerts.enabled ? <IconBell size={14} /> : <IconBellOff size={14} />} HR alerts
        </Btn>
        <Btn onClick={() => act({ type: "lap" })} title="Lap (L)">
          <IconFlag size={14} /> Lap
        </Btn>
        <div className="grow" />
        {usingSim && sim && (
          <div className="ctl sim-ctl">
            <span className="ctl-l">Simulated rider</span>
            <Slider value={sim.effort_w} min={0} max={Math.round(ftp * 1.8)} step={5} onChange={setEffort} format={(v) => `${v} W`} />
          </div>
        )}
      </footer>

      {drawer && s && <EditDrawer info={info} snap={s} act={act} onClose={() => setDrawer(false)} zones={zones} alerts={alerts} setAlerts={setAlerts} />}

      {s?.paused && !ending && (
        <div className="pause-veil" onClick={togglePause}>
          <div>
            <IconPause size={42} />
            <h2>Paused</h2>
            <p>Press space or click to resume</p>
          </div>
        </div>
      )}

      {ending && (
        <Modal
          title="Finish ride?"
          onClose={() => setEnding(false)}
          footer={
            <>
              <Btn kind="ghost" onClick={() => finish(false)} disabled={saving}>Discard</Btn>
              <div className="grow" />
              <Btn onClick={() => setEnding(false)} disabled={saving}>Keep riding</Btn>
              <Btn kind="primary" onClick={() => finish(true)} disabled={saving}>{saving ? "Saving…" : "Save ride"}</Btn>
            </>
          }
        >
          <div className="grid g3">
            <Stat label="Time" value={dur(s?.elapsed_s ?? 0)} />
            <Stat label="Distance" value={dist(s?.distance_m ?? 0, units)} unit={distUnit(units)} />
            <Stat label="Climbed" value={elev(s?.elevation_gain_m ?? 0, units)} unit={elevUnit(units)} />
            <Stat label="Avg power" value={Math.round(s?.avg_power ?? 0)} unit="W" />
            <Stat label="Energy" value={Math.round(s?.work_kj ?? 0)} unit="kJ" />
            <Stat label="Calories" value={Math.round(s?.calories_kcal ?? 0)} unit="kcal" />
          </div>
          {info.kind === "route" && !s?.route_complete && !s?.route_modified && <p className="muted mt12">You haven't finished the route, so this won't count as a timed attempt.</p>}
          {s?.route_modified && route && route.name !== "Virtual road" && (
            <div className="banner mt12">
              <Toggle value={saveRoute} onChange={setSaveRoute} label={<>Save the edited course as a new route (“{route.name.startsWith("Endless ") ? `${route.name.slice(8, 9).toUpperCase()}${route.name.slice(9)} loop` : route.name} (edited)”)</>} />
              <div className="muted small mt8">Edited courses don't count towards the original route's leaderboard.</div>
            </div>
          )}
        </Modal>
      )}
    </div>
  );
}

function SectorCard({ sec, units }: { sec: Snapshot["sectors"][number]; units: "metric" | "imperial" }) {
  return (
    <>
      <div className="cc-cat" style={{ color: gradeColor(sec.avg_grade_pct) }}>{sec.active ? "Sector · go!" : "Sector ahead"}</div>
      <div className="cc-big">{sec.active ? dur(sec.elapsed_s) : short(sec.distance_to_m, units)}</div>
      <div className="muted">{sec.name}</div>
      <div className="cc-bar"><i style={{ width: `${sec.progress * 100}%`, background: gradeColor(sec.avg_grade_pct) }} /></div>
      <div className="muted small">{short(sec.length_m, units)} · avg {sec.avg_grade_pct.toFixed(1)}%{sec.timed ? " · timed" : ""}</div>
    </>
  );
}

function Ctl({ label, value, onMinus, onPlus }: { label: string; value: string; onMinus: () => void; onPlus: () => void }) {
  return (
    <div className="ctl">
      <span className="ctl-l">{label}</span>
      <button className="ctl-b" onClick={onMinus} aria-label={`${label} down`}><IconMinus size={16} /></button>
      <b className="ctl-v">{value}</b>
      <button className="ctl-b" onClick={onPlus} aria-label={`${label} up`}><IconPlus size={16} /></button>
    </div>
  );
}

// ===========================================================================
// In-ride editor
// ===========================================================================

const SECTOR_PRESETS: { label: string; spec: Partial<SectorSpec> }[] = [
  { label: "Short climb · 1 km @ 6%", spec: { kind: "climb", length_m: 1000, grade_pct: 6 } },
  { label: "Steep wall · 400 m @ 11%", spec: { kind: "climb", length_m: 400, grade_pct: 11, name: "The Wall" } },
  { label: "Long climb · 4 km @ 7%", spec: { kind: "climb", length_m: 4000, grade_pct: 7 } },
  { label: "Mountain pass · 10 km @ 6.5%", spec: { kind: "climb", length_m: 10000, grade_pct: 6.5, name: "Mountain pass" } },
  { label: "Rollers · 3 km", spec: { kind: "rollers", length_m: 3000, grade_pct: 4 } },
  { label: "Flat · 2 km", spec: { kind: "flat", length_m: 2000, grade_pct: 0 } },
  { label: "Descent · 2 km @ −6%", spec: { kind: "descent", length_m: 2000, grade_pct: 6 } },
  { label: "Sprint · 300 m", spec: { kind: "sprint", length_m: 300, grade_pct: 0, name: "Sprint" } },
  { label: "Hilly terrain · 5 km", spec: { kind: "terrain", length_m: 5000, terrain: "hilly" } },
];

const INTERVAL_PRESETS: { label: string; text: string }[] = [
  { label: "5 × 1 min @ 120%", text: "5x 1m @120% / 1m @50%" },
  { label: "3 × 30 s sprints", text: "3x 30s @160% / 90s @50%" },
  { label: "4 × 4 min VO2", text: "4x 4m @115% / 3m @55%" },
  { label: "10 min sweet spot", text: "10m @90%" },
  { label: "20 min tempo", text: "20m @82%" },
  { label: "Tabata", text: "8x 20s @170% / 10s @40%" },
  { label: "2 min all-out", text: "2m effort @2%" },
  { label: "5 min easy", text: "5m @55%" },
];

function sectorPreview(spec: SectorSpec): [number, number][] {
  const L = Math.max(50, spec.length_m || 0);
  const g = spec.grade_pct || 0;
  let segs: { l: number; g: number }[];
  switch (spec.kind) {
    case "climb": segs = [{ l: L, g: Math.abs(g) }]; break;
    case "descent": segs = [{ l: L, g: -Math.abs(g) }]; break;
    case "rollers": {
      const n = Math.max(2, Math.round(L / 400) & ~1);
      const a = Math.abs(g) < 0.5 ? 4 : Math.abs(g);
      segs = Array.from({ length: n }, (_, i) => ({ l: L / n, g: i % 2 ? -a : a }));
      break;
    }
    default: segs = [{ l: L, g: spec.kind === "flat" ? Math.max(-1, Math.min(1, g)) : 0 }];
  }
  const net = segs.reduce((a, s) => a + (s.l * s.g) / 100, 0);
  if (spec.rejoin && Math.abs(net) >= 2) {
    const l = Math.max(Math.abs(net) / 0.05, 200);
    segs.push({ l, g: (-net / l) * 100 });
  }
  const pts: [number, number][] = [[0, 0], [150, 0]];
  let d = 150, e = 0;
  for (const s of segs) {
    d += s.l;
    e += (s.l * s.g) / 100;
    pts.push([d, e]);
  }
  pts.push([d + 150, e]);
  return pts;
}

function EditDrawer({ info, snap, act, onClose, zones, alerts, setAlerts }: {
  info: RideInfo;
  snap: Snapshot;
  act: (a: RideAction) => void;
  onClose: () => void;
  zones: HrZone[];
  alerts: HrAlertSettings;
  setAlerts: (a: HrAlertSettings) => void;
}) {
  const { profile, setProfile, toast } = useApp();
  const units = useUnits();
  const hasPlan = info.steps.length > 0;
  const [tab, setTab] = useState<"road" | "workout" | "ride">(snap.structured ? "workout" : "road");
  const [spec, setSpec] = useState<SectorSpec>({ kind: "climb", name: "", length_m: 1500, grade_pct: 7, in_m: 300, at_end: false, rejoin: true, timed: true });
  const [text, setText] = useState("");
  const [textErr, setTextErr] = useState<string | null>(null);
  const ftp = info.ftp;
  const cur = snap.step?.index ?? -1;

  const addSector = (s: SectorSpec) => act({ type: "add_sector", sector: s });
  const insertText = async (t: string, atEnd: boolean) => {
    try {
      const blocks = await api.parseWorkoutText(t);
      act({ type: "insert_blocks", blocks, at_end: atEnd });
      setTextErr(null);
      setText("");
    } catch (e) {
      setTextErr(errMsg(e));
    }
  };

  const goalOf = (type: Goal["type"], v: number): Goal =>
    type === "time" ? { type, seconds: v * 60 } : type === "distance" ? { type, meters: v * 1000 } : type === "energy" ? { type, kj: v } : type === "calories" ? { type, kcal: v } : { type: "elevation", meters: v };

  return (
    <aside className="drawer">
      <header className="drawer-h">
        <h3>Edit ride</h3>
        <Segmented
          size="sm"
          value={tab}
          onChange={setTab}
          options={[
            { value: "road", label: "Road ahead" },
            { value: "workout", label: hasPlan ? "Workout" : "Intervals" },
            { value: "ride", label: "Ride" },
          ]}
        />
        <button className="icon-btn" onClick={onClose} aria-label="Close"><IconX size={16} /></button>
      </header>
      <div className="drawer-b">
        {tab === "road" && (
          <>
            <p className="muted small">Splice new road in ahead of you. Timed sectors record a split you can compare later.{snap.ghost_distance_m != null ? " Editing the road removes the ghost." : ""}</p>
            <h4 className="mt12">Quick add</h4>
            <div className="preset-grid">
              {SECTOR_PRESETS.map((p) => (
                <button key={p.label} className="preset" onClick={() => addSector({ ...spec, name: null, terrain: null, segments: [], ...p.spec } as SectorSpec)}>
                  {p.label}
                </button>
              ))}
            </div>
            <h4 className="mt16">Custom sector</h4>
            <div className="form">
              <Segmented<SectorKind>
                size="sm"
                value={spec.kind}
                onChange={(k) => setSpec({ ...spec, kind: k, grade_pct: k === "flat" || k === "sprint" ? 0 : k === "rollers" ? 4 : spec.grade_pct || 6 })}
                options={(["climb", "descent", "rollers", "flat", "sprint"] as SectorKind[]).map((k) => ({ value: k, label: k[0].toUpperCase() + k.slice(1) }))}
              />
              <div className="row gap12 wrap">
                <Field label="Length"><NumberInput value={+(spec.length_m / 1000).toFixed(2)} step={0.1} min={0.05} suffix="km" width={110} onChange={(v) => setSpec({ ...spec, length_m: Math.max(50, v * 1000) })} /></Field>
                {(spec.kind === "climb" || spec.kind === "descent" || spec.kind === "rollers") && (
                  <Field label={spec.kind === "rollers" ? "Steepness" : "Gradient"}><NumberInput value={spec.grade_pct} step={0.5} min={0.5} max={20} suffix="%" width={90} onChange={(v) => setSpec({ ...spec, grade_pct: v })} /></Field>
                )}
                <Field label="Starts in"><NumberInput value={spec.in_m} step={100} min={50} suffix="m" width={100} onChange={(v) => setSpec({ ...spec, in_m: v })} /></Field>
              </div>
              <Field label="Name (optional)"><input value={spec.name ?? ""} placeholder="e.g. Tuesday KOM" onChange={(e) => setSpec({ ...spec, name: e.target.value })} /></Field>
              <div className="row gap16 wrap">
                <Toggle value={spec.rejoin} onChange={(v) => setSpec({ ...spec, rejoin: v })} label="Return to route elevation" />
                <Toggle value={spec.timed} onChange={(v) => setSpec({ ...spec, timed: v })} label="Timed" />
                {info.kind === "route" && <Toggle value={spec.at_end} onChange={(v) => setSpec({ ...spec, at_end: v })} label="Add at the end instead" />}
              </div>
              <div className="sector-prev">
                <ElevationProfile points={sectorPreview(spec)} height={80} axis={false} interactive={false} minRange={30} labels={false} />
              </div>
              <Btn kind="primary" onClick={() => addSector(spec)}><IconPlus size={14} /> Add to the road</Btn>
            </div>
            {(snap.sectors.length > 0 || snap.can_undo_sector) && (
              <>
                <h4 className="mt16">Your sectors</h4>
                <ul className="sector-list">
                  {snap.sectors.map((x) => (
                    <li key={x.id}>
                      <i style={{ background: gradeColor(x.avg_grade_pct) }} />
                      <div className="grow">
                        <b>{x.name}</b>
                        <span className="muted small">
                          {x.done ? (x.result ? `${dur(x.result.duration_s)} · ${Math.round(x.result.avg_power)} W` : "done") : x.active ? `riding · ${dur(x.elapsed_s)}` : `in ${short(x.distance_to_m, units)}`}
                        </span>
                      </div>
                    </li>
                  ))}
                </ul>
                {snap.can_undo_sector && <Btn size="sm" kind="ghost" onClick={() => act({ type: "undo_sector" })}>Undo last sector</Btn>}
              </>
            )}
            {info.kind === "route" && (
              <>
                <h4 className="mt16">Laps</h4>
                <div className="row gap8 center">
                  <Btn size="sm" onClick={() => act({ type: "add_laps", delta: -1 })} disabled={snap.laps <= snap.lap}><IconMinus size={12} /></Btn>
                  <b>{snap.laps} lap{snap.laps === 1 ? "" : "s"}</b>
                  <Btn size="sm" onClick={() => act({ type: "add_laps", delta: 1 })}><IconPlus size={12} /></Btn>
                  <span className="muted small">{short((snap.route_distance_m ?? 0) * snap.laps, units)} total</span>
                </div>
              </>
            )}
          </>
        )}

        {tab === "workout" && (
          <>
            {snap.structured && snap.step && (
              <>
                <h4>Now · {snap.step.label}</h4>
                <div className="row gap6 wrap">
                  <Btn size="sm" onClick={() => act({ type: "extend_step", seconds: -30 })}>−30 s</Btn>
                  <Btn size="sm" onClick={() => act({ type: "extend_step", seconds: 30 })}>+30 s</Btn>
                  <Btn size="sm" onClick={() => act({ type: "extend_step", seconds: 60 })}>+1 min</Btn>
                  <Btn size="sm" onClick={() => act({ type: "extend_step", seconds: 300 })}>+5 min</Btn>
                  {snap.step.target_w != null && (
                    <>
                      <Btn size="sm" onClick={() => act({ type: "adjust_step", delta_pct: -5 })}>−5% target</Btn>
                      <Btn size="sm" onClick={() => act({ type: "adjust_step", delta_pct: 5 })}>+5% target</Btn>
                    </>
                  )}
                </div>
                <div className="row gap6 wrap mt8">
                  <Btn size="sm" kind="subtle" onClick={() => act({ type: "one_more_rep" })}>One more rep</Btn>
                  <Btn size="sm" onClick={() => act({ type: "skip_step" })}><IconSkip size={12} /> Skip step</Btn>
                  <Btn size="sm" onClick={() => act({ type: "skip_to_cooldown" })}>Skip to cool-down</Btn>
                  <Btn size="sm" kind="ghost" onClick={() => act({ type: "end_workout" })}>End plan, ride free</Btn>
                </div>
                <h4 className="mt16">Coming up</h4>
                <ul className="step-list">
                  {info.steps.slice(cur + 1, cur + 30).map((st, k) => {
                    const idx = cur + 1 + k;
                    return (
                      <li key={idx}>
                        <i style={{ background: st.target.type === "erg" ? zoneColor(((st.target.from + st.target.to) / 2) * ftp, ftp) : "var(--muted-2)" }} />
                        <span className="grow">{st.label}</span>
                        <span className="num muted">{dur(st.duration_s)}</span>
                        <span className="num">{st.target.type === "erg" ? `${Math.round(st.target.from * ftp)}${st.target.to !== st.target.from ? `→${Math.round(st.target.to * ftp)}` : ""} W` : st.effort ? "all out" : "free"}</span>
                        <button className="icon-btn" onClick={() => act({ type: "remove_step", index: idx })} title="Remove"><IconX size={12} /></button>
                      </li>
                    );
                  })}
                  {info.steps.length - cur - 1 > 29 && <li className="muted small">…and {info.steps.length - cur - 30} more</li>}
                </ul>
              </>
            )}
            {!snap.structured && (
              <p className="muted small">{hasPlan ? "The plan is finished – add more and it'll start straight away." : "Drop a structured set into this ride. The trainer switches to ERG for it, then hands control back."}</p>
            )}
            <h4 className="mt16">{snap.structured ? "Insert after this step" : "Add intervals"}</h4>
            <div className="preset-grid">
              {INTERVAL_PRESETS.map((p) => (
                <button key={p.label} className="preset" onClick={() => insertText(p.text, false)}>{p.label}</button>
              ))}
            </div>
            <div className="form mt12">
              <textarea className="code-in" rows={3} placeholder="e.g. 3x (2m @105%, 1m @60%)" value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} />
              {textErr ? <div className="banner bad small">{textErr}</div> : <div className="field-h">{TEXT_HELP}</div>}
              <div className="row gap8">
                <Btn kind="primary" size="sm" disabled={!text.trim()} onClick={() => insertText(text, false)}>{snap.structured ? "Insert next" : "Start now"}</Btn>
                {snap.structured && <Btn size="sm" disabled={!text.trim()} onClick={() => insertText(text, true)}>Add at end</Btn>}
              </div>
            </div>
          </>
        )}

        {tab === "ride" && (
          <>
            {info.kind === "free" && (
              <>
                <h4>Mode</h4>
                <Segmented<FreeMode>
                  value={snap.free_mode}
                  onChange={(m) => act({ type: "set_free_mode", mode: m })}
                  options={(["gradient", "power", "level", "heart_rate"] as FreeMode[]).map((m) => ({ value: m, label: FREE_MODE_LABEL[m] }))}
                />
                <p className="muted small mt8">
                  {snap.free_mode === "gradient" && "SIM mode – ride the simulated gradient and shift to change effort."}
                  {snap.free_mode === "power" && "ERG – the trainer holds your target power regardless of cadence."}
                  {snap.free_mode === "level" && "Fixed resistance level. Power rises with cadence, like a classic trainer."}
                  {snap.free_mode === "heart_rate" && `Power adjusts every few seconds to hold ${Math.round(snap.hr_target_bpm)} bpm (now ${Math.round(snap.hr_control_w)} W).`}
                </p>
              </>
            )}
            <h4 className={info.kind === "free" ? "mt16" : ""}>Heart-rate alerts</h4>
            <div className="form">
              <Toggle value={alerts.enabled} onChange={(v) => { unlockAudio(); setAlerts({ ...alerts, enabled: v }); }} label={alerts.enabled ? "On – I'll beep when you leave the range" : "Off"} />
              <div className="row gap12 wrap center">
                <Field label="Stay between">
                  <div className="row gap6 center">
                    <select value={alerts.low_zone} onChange={(e) => setAlerts({ ...alerts, low_zone: Math.min(+e.target.value, alerts.high_zone) })}>
                      {zones.map((z) => <option key={z.zone} value={z.zone}>Z{z.zone} {z.name}</option>)}
                    </select>
                    <span className="muted">and</span>
                    <select value={alerts.high_zone} onChange={(e) => setAlerts({ ...alerts, high_zone: Math.max(+e.target.value, alerts.low_zone) })}>
                      {zones.map((z) => <option key={z.zone} value={z.zone}>Z{z.zone} {z.name}</option>)}
                    </select>
                  </div>
                </Field>
              </div>
              <div className="muted small">
                {(() => {
                  const a = zones.find((z) => z.zone === alerts.low_zone);
                  const b = zones.find((z) => z.zone === alerts.high_zone);
                  return a && b ? `${Math.round(a.lo)}–${b.hi ? Math.round(b.hi) : "max"} bpm` : "";
                })()}
                {snap.elapsed_s < alerts.grace_s && alerts.enabled ? ` · alerts start after ${Math.round(alerts.grace_s / 60)} min warm-up` : ""}
              </div>
              <div className="row gap16 wrap">
                <Toggle value={alerts.alert_high} onChange={(v) => setAlerts({ ...alerts, alert_high: v })} label="Too high" />
                <Toggle value={alerts.alert_low} onChange={(v) => setAlerts({ ...alerts, alert_low: v })} label="Too low" />
                <Toggle value={alerts.voice} onChange={(v) => setAlerts({ ...alerts, voice: v })} label="Voice" />
              </div>
              <div className="row gap8 wrap">
                <Btn size="sm" onClick={() => { unlockAudio(); playCue("high", alerts.volume); }}>Test sound</Btn>
                <Btn size="sm" kind="ghost" onClick={() => setAlerts({ ...alerts, grace_s: 0 })} disabled={alerts.grace_s === 0}>Skip warm-up delay</Btn>
                <Btn
                  size="sm"
                  kind="ghost"
                  onClick={async () => {
                    if (!profile) return;
                    const p = await api.saveProfile({ ...profile, hr_alerts: alerts });
                    setProfile(p);
                    toast("Saved as your default alert settings", "good");
                  }}
                >
                  Save as default
                </Btn>
              </div>
              {snap.heart_rate == null && <div className="banner warn small">No heart rate yet – connect a strap or start your watch's HR broadcast in Devices.</div>}
            </div>
            <h4 className="mt16">Goal</h4>
            <div className="preset-grid">
              {[
                ["30 min", goalOf("time", 30)],
                ["1 hour", goalOf("time", 60)],
                ["2 hours", goalOf("time", 120)],
                ["20 km", goalOf("distance", 20)],
                ["40 km", goalOf("distance", 40)],
                ["500 kJ", goalOf("energy", 500)],
                ["1000 kJ", goalOf("energy", 1000)],
                ["500 kcal", goalOf("calories", 500)],
                ["500 m climbing", goalOf("elevation", 500)],
              ].map(([l, g]) => (
                <button key={l as string} className="preset" onClick={() => { act({ type: "set_goal", goal: g as Goal }); toast(`Goal set: ${l}`); }}>{l as string}</button>
              ))}
            </div>
            {snap.goal && <Btn size="sm" kind="ghost" className="mt8" onClick={() => act({ type: "set_goal", goal: null })}>Clear goal ({snap.goal.label})</Btn>}
            <h4 className="mt16">Laps & trainer</h4>
            <div className="row gap8 wrap">
              <Btn size="sm" onClick={() => act({ type: "lap" })}><IconFlag size={12} /> Lap now</Btn>
              <Btn size="sm" onClick={() => act({ type: "difficulty", delta_pct: -10 })}>Difficulty −10%</Btn>
              <Btn size="sm" onClick={() => act({ type: "difficulty", delta_pct: 10 })}>Difficulty +10%</Btn>
              <span className="muted small">now {Math.round(snap.trainer_difficulty * 100)}%</span>
            </div>
            {profile?.virtual_shifting && <p className="muted small mt8">Virtual shifting: <kbd>[</kbd> <kbd>]</kbd> or Page Up / Page Down.</p>}
            <p className="muted small mt16">Shortcuts: <kbd>Space</kbd> pause · <kbd>L</kbd> lap · <kbd>H</kbd> HR alerts · <kbd>E</kbd> this panel · <kbd>↑</kbd><kbd>↓</kbd> adjust · <kbd>→</kbd> skip step</p>
          </>
        )}
      </div>
    </aside>
  );
}
