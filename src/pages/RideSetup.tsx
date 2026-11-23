import { useEffect, useMemo, useState } from "react";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad, useUnits } from "../lib/app";
import { dist, distUnit, dur, durWords, elev, elevUnit, difficultyColor } from "../lib/format";
import type { FreeMode, FreeOptions, Goal, RideKind, StartRequest, Terrain, TestKind } from "../lib/types";
import { ElevationProfile, WorkoutChart } from "../components/charts";
import { Btn, Card, Field, NumberInput, PageHeader, Pill, Segmented, Slider, Stat, Toggle } from "../components/ui";
import { IconBluetooth, IconBolt, IconGhost, IconHeart, IconMountain, IconPlay, IconRepeat, IconGauge } from "../components/icons";

type GoalType = "none" | Goal["type"];
const FREE_MODES: { mode: FreeMode; title: string; desc: string; icon: JSX.Element }[] = [
  { mode: "gradient", title: "Gradient", desc: "Ride a simulated slope. Shift gears to change effort – like riding outside.", icon: <IconMountain size={20} /> },
  { mode: "power", title: "Power (ERG)", desc: "The trainer holds a fixed wattage whatever your cadence.", icon: <IconBolt size={20} /> },
  { mode: "level", title: "Resistance level", desc: "Classic trainer feel: a fixed brake level, power rises with cadence.", icon: <IconGauge size={20} /> },
  { mode: "heart_rate", title: "Heart rate", desc: "Power adjusts automatically to hold your target heart rate. Needs a strap.", icon: <IconHeart size={20} /> },
];
import { useTrainerReady } from "../components/DeviceChip";

export default function RideSetup() {
  const { nav, go, toast, refreshDevices, profile } = useApp();
  const units = useUnits();
  const initial: StartRequest | undefined = nav.params.req;
  const [kind, setKind] = useState<RideKind>(initial?.kind ?? "route");
  const [routeId, setRouteId] = useState<string | null>(initial?.route_id ?? null);
  const [workoutId, setWorkoutId] = useState<string | null>(initial?.workout_id ?? null);
  const [test, setTest] = useState<TestKind | null>(initial?.test ?? null);
  const [laps, setLaps] = useState(initial?.laps ?? 1);
  const [ghost, setGhost] = useState(initial?.ghost ?? true);
  const [workoutRoute, setWorkoutRoute] = useState<string>("");
  const [category, setCategory] = useState("All");
  const [starting, setStarting] = useState(false);
  const eventId = initial?.event_id ?? null;
  const [free, setFree] = useState<FreeOptions>({ mode: "gradient", grade_pct: 0, watts: 0, level_pct: 30, target_hr: 0, terrain: null, seed: 0 });
  const [scenery, setScenery] = useState<string>("flat"); // flat | terrain:<t> | route:<id>
  const [goalType, setGoalType] = useState<GoalType>("none");
  const [goalValue, setGoalValue] = useState(60);
  const ready = useTrainerReady();

  const routes = useLoad(() => api.listRoutes());
  const workouts = useLoad(() => api.listWorkouts());
  const tests = useLoad(() => api.listTests());
  const events = useLoad(() => api.listEvents());
  const ev = events.data?.find((e) => e.event.id === eventId);

  // Events define their own target.
  useEffect(() => {
    if (!ev) return;
    const t = ev.event.target;
    if (t.type === "route") {
      setKind("route");
      setRouteId(t.route_id);
      setLaps(t.laps);
    } else if (t.type === "workout") {
      setKind("workout");
      setWorkoutId(t.workout_id);
      setWorkoutRoute(t.route_id ?? "");
    } else {
      setKind("test");
      setTest(t.test);
    }
  }, [ev]);

  useEffect(() => {
    if (!routeId && routes.data?.length) setRouteId(routes.data[0].id);
  }, [routes.data, routeId]);
  useEffect(() => {
    if (!workoutId && workouts.data?.length) setWorkoutId(workouts.data[0].workout.id);
  }, [workouts.data, workoutId]);
  useEffect(() => {
    if (!test && tests.data?.length) setTest(tests.data[0].info.kind);
  }, [tests.data, test]);

  const route = routes.data?.find((r) => r.id === routeId);
  const workout = workouts.data?.find((w) => w.workout.id === workoutId);
  const testV = tests.data?.find((t) => t.info.kind === test);
  const cats = useMemo(() => ["All", ...Array.from(new Set((workouts.data ?? []).map((w) => w.workout.category)))], [workouts.data]);

  const start = async () => {
    const req: StartRequest = { kind, event_id: eventId };
    if (kind === "route") Object.assign(req, { route_id: routeId, laps, ghost });
    if (kind === "workout") Object.assign(req, { workout_id: workoutId, route_id: workoutRoute || null });
    if (kind === "test") Object.assign(req, { test });
    if (kind === "free") {
      const terrain = scenery.startsWith("terrain:") ? (scenery.slice(8) as Terrain) : null;
      const routeId = scenery.startsWith("route:") ? scenery.slice(6) : null;
      const goal: Goal | null =
        goalType === "none" ? null
        : goalType === "time" ? { type: "time", seconds: goalValue * 60 }
        : goalType === "distance" ? { type: "distance", meters: goalValue * 1000 }
        : goalType === "energy" ? { type: "energy", kj: goalValue }
        : goalType === "calories" ? { type: "calories", kcal: goalValue }
        : { type: "elevation", meters: goalValue };
      Object.assign(req, { free: { ...free, terrain, seed: Math.floor(Math.random() * 1e9) }, route_id: routeId, goal });
    }
    setStarting(true);
    try {
      await api.rideStart(req);
      go("live");
    } catch (e) {
      toast(errMsg(e), "bad");
    } finally {
      setStarting(false);
    }
  };

  const useSim = async () => {
    await api.simSet({ enabled: true, effort_w: Math.round((profile?.ftp ?? 200) * 0.7) });
    refreshDevices();
    toast("Simulator on – use the effort slider during the ride", "good");
  };

  return (
    <div className="page">
      <PageHeader
        title="Start a ride"
        sub={ev ? <>Event: <b style={{ color: ev.event.color }}>{ev.event.name}</b> · {ev.target_name}</> : "Pick a route, a structured workout, a fitness test or just ride."}
        actions={
          <Segmented<RideKind>
            value={kind}
            onChange={setKind}
            options={[
              { value: "route", label: "Route" },
              { value: "workout", label: "Workout" },
              { value: "test", label: "Test" },
              { value: "free", label: "Free ride" },
            ]}
          />
        }
      />
      <div className="setup">
        <div className="setup-list">
          {kind === "route" &&
            routes.data?.map((r) => (
              <button key={r.id} className={`pick ${r.id === routeId ? "on" : ""}`} onClick={() => setRouteId(r.id)}>
                <div className="pick-t">
                  <b>{r.name}</b>
                  <span className="muted">
                    {dist(r.stats.distance_m, units)} {distUnit(units)} · {elev(r.stats.gain_m, units)} {elevUnit(units)} · max {r.stats.max_grade_pct.toFixed(0)}%
                  </span>
                </div>
                <div className="pick-p">
                  <ElevationProfile points={r.thumbnail} height={44} axis={false} interactive={false} labels={false} />
                </div>
                <Pill color={difficultyColor(r.stats.difficulty)}>{r.stats.difficulty_label}</Pill>
              </button>
            ))}
          {kind === "workout" && (
            <>
              <div className="row gap6 wrap mb8">
                {cats.map((c) => (
                  <button key={c} className={`chip ${c === category ? "on" : ""}`} onClick={() => setCategory(c)}>
                    {c}
                  </button>
                ))}
              </div>
              {workouts.data
                ?.filter((w) => category === "All" || w.workout.category === category)
                .map((w) => (
                  <button key={w.workout.id} className={`pick ${w.workout.id === workoutId ? "on" : ""}`} onClick={() => setWorkoutId(w.workout.id)}>
                    <div className="pick-t">
                      <b>{w.workout.name}</b>
                      <span className="muted">
                        {w.workout.category} · {durWords(w.estimate.duration_s)} · TSS {w.estimate.tss.toFixed(0)}
                      </span>
                    </div>
                    <div className="pick-p">
                      <WorkoutChart steps={w.steps} ftp={profile?.ftp ?? 200} height={40} />
                    </div>
                  </button>
                ))}
            </>
          )}
          {kind === "test" &&
            tests.data?.map((t) => (
              <button key={t.info.kind} className={`pick ${t.info.kind === test ? "on" : ""}`} onClick={() => setTest(t.info.kind)}>
                <div className="pick-t">
                  <b>{t.info.name}</b>
                  <span className="muted">~{t.info.approx_minutes} min · {t.info.summary}</span>
                </div>
              </button>
            ))}
          {kind === "free" && (
            <>
              <div className="mode-cards">
                {FREE_MODES.map((m) => (
                  <button key={m.mode} className={`mode-card ${free.mode === m.mode ? "on" : ""}`} onClick={() => setFree({ ...free, mode: m.mode })}>
                    <div className="mc-i">{m.icon}</div>
                    <b>{m.title}</b>
                    <span>{m.desc}</span>
                  </button>
                ))}
              </div>
              <Card className="mt12">
                <div className="form">
                  {free.mode === "gradient" && (
                    <Field label={scenery === "flat" ? "Starting gradient" : "Gradient offset (added to the terrain)"}>
                      <Slider value={free.grade_pct} min={-5} max={15} step={0.5} onChange={(v) => setFree({ ...free, grade_pct: v })} format={(v) => `${v > 0 ? "+" : ""}${v}%`} />
                    </Field>
                  )}
                  {free.mode === "power" && (
                    <Field label="Target power">
                      <Slider value={free.watts || Math.round((profile?.ftp ?? 200) * 0.65)} min={50} max={Math.round((profile?.ftp ?? 200) * 1.5)} step={5} onChange={(v) => setFree({ ...free, watts: v })} format={(v) => `${v} W · ${Math.round((v / (profile?.ftp ?? 200)) * 100)}%`} />
                    </Field>
                  )}
                  {free.mode === "level" && (
                    <Field label="Resistance level" hint="Percent of your trainer's resistance range.">
                      <Slider value={free.level_pct} min={0} max={100} step={1} onChange={(v) => setFree({ ...free, level_pct: v })} format={(v) => `${v}%`} />
                    </Field>
                  )}
                  {free.mode === "heart_rate" && (
                    <Field label="Target heart rate" hint="Starts from 55% FTP and adjusts every few seconds.">
                      <Slider
                        value={free.target_hr || Math.round((profile?.rest_hr ?? 55) + 0.65 * ((profile?.max_hr ?? 182) - (profile?.rest_hr ?? 55)))}
                        min={90}
                        max={(profile?.max_hr ?? 190) - 5}
                        step={1}
                        onChange={(v) => setFree({ ...free, target_hr: v })}
                        format={(v) => `${v} bpm`}
                      />
                    </Field>
                  )}
                  <Field label="Scenery" hint="Endless terrain is generated fresh every ride and never finishes.">
                    <select value={scenery} onChange={(e) => setScenery(e.target.value)}>
                      <option value="flat">Flat endless road</option>
                      <optgroup label="Endless generated terrain">
                        {(["rolling", "hilly", "mountainous", "alpine"] as Terrain[]).map((t) => (
                          <option key={t} value={`terrain:${t}`}>{t[0].toUpperCase() + t.slice(1)}</option>
                        ))}
                      </optgroup>
                      <optgroup label="Loop one of your routes">
                        {routes.data?.map((r) => <option key={r.id} value={`route:${r.id}`}>{r.name}</option>)}
                      </optgroup>
                    </select>
                  </Field>
                  <Field label="Goal">
                    <div className="row gap8 center wrap">
                      <Segmented<GoalType>
                        size="sm"
                        value={goalType}
                        onChange={(g) => {
                          setGoalType(g);
                          setGoalValue(g === "time" ? 60 : g === "distance" ? 30 : g === "energy" ? 600 : g === "calories" ? 500 : 500);
                        }}
                        options={[{ value: "none", label: "None" }, { value: "time", label: "Time" }, { value: "distance", label: "Distance" }, { value: "energy", label: "kJ" }, { value: "calories", label: "kcal" }, { value: "elevation", label: "Climbing" }]}
                      />
                      {goalType !== "none" && (
                        <NumberInput value={goalValue} onChange={setGoalValue} step={goalType === "time" ? 5 : goalType === "distance" ? 1 : 50} min={1} suffix={goalType === "time" ? "min" : goalType === "distance" ? "km" : goalType === "energy" ? "kJ" : goalType === "calories" ? "kcal" : "m"} width={120} />
                      )}
                    </div>
                  </Field>
                </div>
              </Card>
            </>
          )}
        </div>

        <aside className="setup-panel">
          <Card>
            {kind === "route" && route && (
              <>
                <h2>{route.name}</h2>
                {route.description && <p className="muted">{route.description}</p>}
                <ElevationProfile points={route.thumbnail} height={150} climbs={route.stats.climbs} units={units} />
                <div className="grid g3 mt8">
                  <Stat label="Distance" value={dist(route.stats.distance_m * laps, units)} unit={distUnit(units)} />
                  <Stat label="Elevation" value={elev(route.stats.gain_m * laps, units)} unit={elevUnit(units)} />
                  <Stat label="Est. time" value={dur(route.est_time_s * laps)} />
                </div>
                <div className="form mt12">
                  <Field label="Laps" inline>
                    <Segmented<number> size="sm" value={laps} onChange={setLaps} options={[1, 2, 3, 4, 5].map((n) => ({ value: n, label: n }))} />
                  </Field>
                  <Field label={<span className="row gap6 center"><IconGhost size={14} /> Race your best</span>} inline hint={route.best_time_s && laps === 1 ? `PB ${dur(route.best_time_s)}` : "No completed attempt yet"}>
                    <Toggle value={ghost} onChange={setGhost} />
                  </Field>
                </div>
              </>
            )}
            {kind === "workout" && workout && (
              <>
                <h2>{workout.workout.name}</h2>
                <p className="muted">{workout.workout.description}</p>
                <WorkoutChart steps={workout.steps} ftp={profile?.ftp ?? 200} height={120} axis />
                <div className="grid g4 mt8">
                  <Stat label="Duration" value={dur(workout.estimate.duration_s)} />
                  <Stat label="TSS" value={workout.estimate.tss.toFixed(0)} />
                  <Stat label="IF" value={workout.estimate.intensity_factor.toFixed(2)} />
                  <Stat label="Work" value={workout.estimate.work_kj.toFixed(0)} unit="kJ" />
                </div>
                <div className="form mt12">
                  <Field label="Scenery" hint="ERG controls power; the route only sets your virtual speed and elevation.">
                    <select value={workoutRoute} onChange={(e) => setWorkoutRoute(e.target.value)}>
                      <option value="">Flat virtual road</option>
                      {routes.data?.map((r) => (
                        <option key={r.id} value={r.id}>{r.name}</option>
                      ))}
                    </select>
                  </Field>
                </div>
              </>
            )}
            {kind === "test" && testV && (
              <>
                <h2>{testV.info.name}</h2>
                <p className="muted">{testV.info.how}</p>
                <WorkoutChart steps={testV.steps} ftp={profile?.ftp ?? 200} height={110} axis />
                {testV.last_result?.test_result && (
                  <p className="mt8">
                    Last result: <b>{testV.last_result.test_result.ftp} W</b> <span className="muted">({new Date(testV.last_result.started_at).toLocaleDateString()})</span>
                  </p>
                )}
              </>
            )}
            {kind === "free" && (
              <>
                <h2>Free ride</h2>
                <p className="muted">
                  {FREE_MODES.find((m) => m.mode === free.mode)?.title} mode
                  {scenery === "flat" ? " on a flat road" : scenery.startsWith("terrain:") ? ` over endless ${scenery.slice(8)} terrain` : ` looping ${routes.data?.find((r) => `route:${r.id}` === scenery)?.name ?? "a route"}`}
                  {goalType !== "none" ? ` · goal ${goalValue} ${goalType === "time" ? "min" : goalType === "distance" ? "km" : goalType === "energy" ? "kJ" : goalType === "calories" ? "kcal" : "m climbing"}` : ""}.
                </p>
                <p className="muted small mt8">Everything is editable while you ride: switch modes, add intervals, drop climbs into the road ahead, take laps.</p>
              </>
            )}

            {eventId && (
              <div className="banner mt12">
                <IconRepeat size={14} /> Counts towards <b>{ev?.event.name}</b>
              </div>
            )}
            {!ready && (
              <div className="banner warn mt12">
                <IconBluetooth size={14} /> No trainer connected.
                <div className="row gap8 mt8">
                  <Btn size="sm" onClick={() => go("devices")}>Connect KICKR</Btn>
                  <Btn size="sm" kind="ghost" onClick={useSim}>Use simulator</Btn>
                </div>
              </div>
            )}
            <Btn kind="primary" size="lg" className="w100 mt16" onClick={start} disabled={starting}>
              <IconPlay size={16} /> {starting ? "Starting…" : "Start ride"}
            </Btn>
          </Card>
        </aside>
      </div>
    </div>
  );
}
