import { useEffect, useMemo, useRef, useState } from "react";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad, useUnits } from "../lib/app";
import { categoryColor, dateShort, difficultyColor, dist, distUnit, dur, elev, elevUnit, short } from "../lib/format";
import type { Finish, GenParams, RouteDetail, RouteSummary, SegmentSpec, Terrain } from "../lib/types";
import { ElevationProfile } from "../components/charts";
import { Btn, Card, Empty, Field, NumberInput, PageHeader, Pill, Segmented, Slider, Stat, confirmAsk } from "../components/ui";
import { IconDice, IconEdit, IconGhost, IconPlay, IconPlus, IconRoute, IconStar, IconTrash, IconTrophy, IconUpload, IconX } from "../components/icons";

type Sort = "new" | "dist" | "elev" | "diff";

export default function Routes() {
  const { go } = useApp();
  const units = useUnits();
  const { data, reload } = useLoad(() => api.listRoutes());
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<Sort>("new");

  const list = useMemo(() => {
    let v = (data ?? []).filter((r) => (r.name + " " + r.tags.join(" ")).toLowerCase().includes(q.toLowerCase()));
    const key: Record<Sort, (r: RouteSummary) => number> = {
      new: () => 0,
      dist: (r) => -r.stats.distance_m,
      elev: (r) => -r.stats.gain_m,
      diff: (r) => -r.stats.difficulty,
    };
    if (sort !== "new") v = [...v].sort((a, b) => key[sort](a) - key[sort](b));
    return v;
  }, [data, q, sort]);

  return (
    <div className="page">
      <PageHeader
        title="Routes"
        sub="Generated, hand-built and imported courses. Every route records your best time as a ghost to race."
        actions={
          <>
            <Btn onClick={() => go("route-new", { tab: "gpx" })}>
              <IconUpload size={14} /> Import GPX
            </Btn>
            <Btn kind="primary" onClick={() => go("route-new", { tab: "gen" })}>
              <IconPlus size={14} /> New route
            </Btn>
          </>
        }
      />
      <div className="row gap12 center mb16">
        <input className="search" placeholder="Search routes…" value={q} onChange={(e) => setQ(e.target.value)} />
        <Segmented<Sort>
          size="sm"
          value={sort}
          onChange={setSort}
          options={[
            { value: "new", label: "Newest" },
            { value: "dist", label: "Longest" },
            { value: "elev", label: "Hilliest" },
            { value: "diff", label: "Hardest" },
          ]}
        />
      </div>
      {data && list.length === 0 && (
        <Empty icon={<IconRoute size={30} />} title="No routes match">
          Create one with the generator or import a GPX file.
        </Empty>
      )}
      <div className="cards">
        {list.map((r) => (
          <RouteCard key={r.id} r={r} units={units} onOpen={() => go("route", { id: r.id })} onFav={async () => { await api.toggleFavouriteRoute(r.id); reload(); }} />
        ))}
      </div>
    </div>
  );
}

function RouteCard({ r, units, onOpen, onFav }: { r: RouteSummary; units: "metric" | "imperial"; onOpen: () => void; onFav: () => void }) {
  const { prepareRide } = useApp();
  return (
    <article className="rcard" onClick={onOpen}>
      <header>
        <div>
          <h3>{r.name}</h3>
          <div className="row gap6 wrap">
            <Pill color={difficultyColor(r.stats.difficulty)}>{r.stats.difficulty_label} · {r.stats.difficulty.toFixed(1)}</Pill>
            {r.is_loop && <Pill>Loop</Pill>}
            {r.source === "gpx" && <Pill>GPX</Pill>}
            {r.best_time_s && <Pill color="#7259E8"><IconGhost size={11} /> {dur(r.best_time_s)}</Pill>}
          </div>
        </div>
        <button className={`star ${r.favourite ? "on" : ""}`} onClick={(e) => { e.stopPropagation(); onFav(); }} aria-label="Favourite">
          <IconStar size={18} filled={r.favourite} />
        </button>
      </header>
      <ElevationProfile points={r.thumbnail} height={92} axis={false} interactive={false} climbs={r.stats.climbs} labels={false} />
      <footer>
        <span><b>{dist(r.stats.distance_m, units)}</b> {distUnit(units)}</span>
        <span><b>{elev(r.stats.gain_m, units)}</b> {elevUnit(units)}</span>
        <span><b>{r.stats.max_grade_pct.toFixed(0)}</b>% max</span>
        <span className="muted">~{dur(r.est_time_s)}</span>
        <button className="play-mini" onClick={(e) => { e.stopPropagation(); prepareRide({ kind: "route", route_id: r.id }); }} aria-label="Ride">
          <IconPlay size={14} />
        </button>
      </footer>
    </article>
  );
}

// ---------------------------------------------------------------------------
// Route detail
// ---------------------------------------------------------------------------

export function RouteDetailPage({ id }: { id: string }) {
  const { go, back, toast, prepareRide } = useApp();
  const units = useUnits();
  const { data, reload } = useLoad(() => api.getRoute(id), [id]);
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");

  if (!data) return <div className="page"><PageHeader title="Loading…" back={back} /></div>;
  const { route, stats } = data;
  const attempts = [...data.attempts].sort((a, b) => (a.route_time_s ?? 1e12) - (b.route_time_s ?? 1e12));

  const save = async () => {
    try {
      await api.saveRoute({ ...route, name, description: desc });
      setEditing(false);
      reload();
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };
  const del = async () => {
    if (!(await confirmAsk(`Delete “${route.name}”? Ride history is kept.`))) return;
    await api.deleteRoute(route.id);
    toast("Route deleted");
    go("routes");
  };

  return (
    <div className="page">
      <PageHeader
        back={back}
        title={
          editing ? <input className="title-in" value={name} onChange={(e) => setName(e.target.value)} autoFocus /> : route.name
        }
        sub={
          editing ? (
            <input className="sub-in" value={desc} placeholder="Description" onChange={(e) => setDesc(e.target.value)} />
          ) : (
            route.description || <span className="muted">{route.tags.join(" · ")}</span>
          )
        }
        actions={
          editing ? (
            <>
              <Btn onClick={() => setEditing(false)}>Cancel</Btn>
              <Btn kind="primary" onClick={save}>Save</Btn>
            </>
          ) : (
            <>
              <Btn kind="ghost" onClick={async () => { await api.toggleFavouriteRoute(route.id); reload(); }}>
                <IconStar size={14} filled={route.favourite} /> {route.favourite ? "Favourited" : "Favourite"}
              </Btn>
              <Btn kind="ghost" onClick={() => { setName(route.name); setDesc(route.description); setEditing(true); }}>
                <IconEdit size={14} /> Rename
              </Btn>
              {(route.gen_params || route.segments) && (
                <Btn kind="ghost" onClick={() => go("route-new", { tab: route.gen_params ? "gen" : "seg", from: route })}>
                  <IconDice size={14} /> Make a variant
                </Btn>
              )}
              <Btn kind="ghost" onClick={del}><IconTrash size={14} /></Btn>
              <Btn kind="primary" onClick={() => prepareRide({ kind: "route", route_id: route.id })}>
                <IconPlay size={14} /> Ride this route
              </Btn>
            </>
          )
        }
      />
      <Card pad>
        <ElevationProfile points={route.points} height={280} climbs={stats.climbs} units={units} />
      </Card>
      <div className="grid g6 mt16">
        <Card><Stat label="Distance" value={dist(stats.distance_m, units)} unit={distUnit(units)} /></Card>
        <Card><Stat label="Elevation gain" value={elev(stats.gain_m, units)} unit={elevUnit(units)} sub={`${elev(stats.loss_m, units)} ${elevUnit(units)} descent`} /></Card>
        <Card><Stat label="Max gradient" value={stats.max_grade_pct.toFixed(1)} unit="%" sub={`steepest descent ${stats.min_grade_pct.toFixed(1)}%`} /></Card>
        <Card><Stat label="Climbing density" value={stats.climb_density.toFixed(0)} unit={`m/km`} sub={`${Math.round(stats.climbing_share * 100)}% of route ≥ 3%`} /></Card>
        <Card><Stat label="Difficulty" value={stats.difficulty.toFixed(1)} unit="/10" color={difficultyColor(stats.difficulty)} sub={stats.difficulty_label} /></Card>
        <Card><Stat label="Est. time" value={dur(data.est_time_s)} sub="at 70% FTP" /></Card>
      </div>
      <div className="grid g2 mt16">
        <Card title={`Climbs (${stats.climbs.length})`}>
          {stats.climbs.length === 0 ? (
            <div className="empty-inline">No significant climbs – a fast course.</div>
          ) : (
            <table className="tbl compact">
              <thead>
                <tr><th>Cat</th><th>Starts</th><th className="num">Length</th><th className="num">Gain</th><th className="num">Avg</th><th className="num">Max</th></tr>
              </thead>
              <tbody>
                {stats.climbs.map((c, i) => (
                  <tr key={i}>
                    <td><Pill color={categoryColor(c.category)}>{c.category === "Uncategorised" ? "–" : c.category}</Pill></td>
                    <td>{short(c.start_d, units)}</td>
                    <td className="num">{short(c.length_m, units)}</td>
                    <td className="num">{elev(c.gain_m, units)} {elevUnit(units)}</td>
                    <td className="num">{c.avg_grade_pct.toFixed(1)}%</td>
                    <td className="num">{c.max_grade_pct.toFixed(1)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
        <Card title={<span className="row gap6 center"><IconTrophy size={15} /> Your attempts</span>}>
          {attempts.length === 0 ? (
            <div className="empty-inline">Not ridden yet. Finish it once and you'll race your ghost next time.</div>
          ) : (
            <table className="tbl compact">
              <thead><tr><th>#</th><th>Date</th><th className="num">Time</th><th className="num">Avg W</th><th className="num">NP</th><th className="num">Laps</th></tr></thead>
              <tbody>
                {attempts.map((a, i) => (
                  <tr key={a.id} onClick={() => go("activity", { id: a.id })}>
                    <td>{a.completed ? i + 1 : "–"}</td>
                    <td>{dateShort(a.started_at)} {a.personal_best && <Pill color="#E3A600">PB</Pill>}</td>
                    <td className="num">{a.route_time_s ? dur(a.route_time_s) : <span className="muted">DNF</span>}</td>
                    <td className="num">{Math.round(a.avg_power)}</td>
                    <td className="num">{Math.round(a.np)}</td>
                    <td className="num">{a.laps}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Builder: generator / segments / GPX
// ---------------------------------------------------------------------------

const PER_KM: Record<Terrain, number> = { flat: 3, rolling: 10, hilly: 18, mountainous: 28, alpine: 40 };
const MAXG: Record<Terrain, number> = { flat: 3, rolling: 7, hilly: 11, mountainous: 12, alpine: 12 };

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

export function RouteBuilder({ initialTab }: { initialTab?: "gen" | "seg" | "gpx" }) {
  const { nav, back, go, toast } = useApp();
  const units = useUnits();
  const from = nav.params.from as RouteDetail["route"] | undefined;
  const [tab, setTab] = useState<"gen" | "seg" | "gpx">(initialTab ?? "gen");
  const [name, setName] = useState(from ? `${from.name} (variant)` : "");
  const [params, setParams] = useState<GenParams>(
    from?.gen_params
      ? { ...from.gen_params, seed: from.gen_params.seed + 1 }
      : { distance_km: 30, target_gain_m: 450, max_grade_pct: 9, terrain: "hilly", finish: "loop", seed: Math.floor(Math.random() * 1e6), start_ele_m: 120, smoothness: 0.5 }
  );
  const [segs, setSegs] = useState<SegmentSpec[]>(
    from?.segments ?? [
      { length_m: 2000, grade_pct: 0 },
      { length_m: 1500, grade_pct: 5 },
      { length_m: 800, grade_pct: 8 },
      { length_m: 2300, grade_pct: -5.5 },
      { length_m: 3000, grade_pct: 1 },
    ]
  );
  const [startEle, setStartEle] = useState(100);
  const [preview, setPreview] = useState<RouteDetail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const dParams = useDebounced(params, 220);
  const dSegs = useDebounced(segs, 300);
  const fileRef = useRef<HTMLInputElement>(null);
  const [gpxText, setGpxText] = useState<string | null>(null);
  const [gpxFile, setGpxFile] = useState("");

  useEffect(() => {
    if (tab !== "gen") return;
    api.generateRoute(dParams, name || "Generated route").then((d) => { setPreview(d); setErr(null); }).catch((e) => setErr(errMsg(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dParams, tab]);
  useEffect(() => {
    if (tab !== "seg") return;
    if (!dSegs.length) return;
    api.buildRoute(dSegs, name || "Custom route", startEle).then((d) => { setPreview(d); setErr(null); }).catch((e) => setErr(errMsg(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dSegs, startEle, tab]);
  useEffect(() => {
    if (tab === "gpx") setPreview(null);
  }, [tab]);

  const set = <K extends keyof GenParams>(k: K, v: GenParams[K]) => setParams((p) => ({ ...p, [k]: v }));
  const setTerrain = (t: Terrain) =>
    setParams((p) => ({ ...p, terrain: t, target_gain_m: Math.round((PER_KM[t] * p.distance_km) / 10) * 10, max_grade_pct: MAXG[t] }));
  const maxGainForGrade = Math.round(params.distance_km * 1000 * (params.max_grade_pct / 100) * 0.55);

  const save = async () => {
    if (!preview) return;
    try {
      const r = await api.saveRoute({ ...preview.route, name: name.trim() || preview.route.name });
      toast("Route saved", "good");
      go("route", { id: r.id });
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const importGpx = async () => {
    if (!gpxText) return;
    try {
      const d = await api.importGpx(gpxText, name.trim() || undefined);
      toast(`Imported “${d.route.name}”`, "good");
      go("route", { id: d.route.id });
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const u = units === "imperial";
  const kmLabel = (km: number) => (u ? `${(km / 1.609344).toFixed(1)} mi` : `${km} km`);
  const mLabel = (m: number) => (u ? `${Math.round(m * 3.28084).toLocaleString()} ft` : `${m.toLocaleString()} m`);

  return (
    <div className="page">
      <PageHeader
        back={back}
        title="New route"
        sub="Shape a course by feel, define it segment by segment, or import a real ride."
        actions={
          <Segmented
            value={tab}
            onChange={setTab}
            options={[
              { value: "gen", label: "Generator" },
              { value: "seg", label: "Segments" },
              { value: "gpx", label: "Import GPX" },
            ]}
          />
        }
      />
      <div className="builder">
        <Card className="builder-form">
          <Field label="Name">
            <input value={name} placeholder={tab === "gpx" ? "Use name from file" : "My route"} onChange={(e) => setName(e.target.value)} />
          </Field>

          {tab === "gen" && (
            <div className="form">
              <Field label="Terrain">
                <Segmented<Terrain>
                  size="sm"
                  value={params.terrain}
                  onChange={setTerrain}
                  options={(["flat", "rolling", "hilly", "mountainous", "alpine"] as Terrain[]).map((t) => ({ value: t, label: t[0].toUpperCase() + t.slice(1) }))}
                />
              </Field>
              <Field label="Distance">
                <Slider value={params.distance_km} min={5} max={200} step={1} onChange={(v) => set("distance_km", v)} format={kmLabel} />
              </Field>
              <Field label="Elevation gain" hint={params.target_gain_m > maxGainForGrade ? `Capped by max gradient – raise it or shorten the climb target (~${mLabel(maxGainForGrade)} achievable).` : undefined}>
                <Slider value={params.target_gain_m} min={0} max={Math.max(500, Math.min(6000, params.distance_km * 60))} step={10} onChange={(v) => set("target_gain_m", v)} format={mLabel} />
              </Field>
              <Field label="Max gradient">
                <Slider value={params.max_grade_pct} min={2} max={20} step={0.5} onChange={(v) => set("max_grade_pct", v)} format={(v) => `${v}%`} />
              </Field>
              <Field label="Smoothness">
                <Slider value={params.smoothness} min={0} max={1} step={0.05} onChange={(v) => set("smoothness", v)} format={(v) => (v < 0.3 ? "Choppy" : v < 0.7 ? "Natural" : "Smooth")} />
              </Field>
              <Field label="Finish">
                <Segmented<Finish>
                  size="sm"
                  value={params.finish}
                  onChange={(v) => set("finish", v)}
                  options={[
                    { value: "loop", label: "Loop" },
                    { value: "free", label: "Point to point" },
                    { value: "summit", label: "Summit finish" },
                  ]}
                />
              </Field>
              <div className="row gap12">
                <Field label="Start elevation">
                  <NumberInput value={params.start_ele_m} onChange={(v) => set("start_ele_m", v)} step={10} suffix="m" />
                </Field>
                <Field label="Seed">
                  <div className="row gap6">
                    <NumberInput value={params.seed} onChange={(v) => set("seed", Math.max(0, Math.round(v)))} />
                    <Btn onClick={() => set("seed", Math.floor(Math.random() * 1e6))} title="New random course">
                      <IconDice size={16} />
                    </Btn>
                  </div>
                </Field>
              </div>
            </div>
          )}

          {tab === "seg" && (
            <div className="form">
              <div className="seg-list">
                <div className="seg-row head">
                  <span>#</span><span>Length (km)</span><span>Gradient (%)</span><span />
                </div>
                {segs.map((sg, i) => (
                  <div className="seg-row" key={i}>
                    <span className="muted">{i + 1}</span>
                    <NumberInput value={+(sg.length_m / 1000).toFixed(2)} step={0.1} min={0.05} onChange={(v) => setSegs((s) => s.map((x, j) => (j === i ? { ...x, length_m: Math.max(10, v * 1000) } : x)))} />
                    <NumberInput value={sg.grade_pct} step={0.5} min={-20} max={20} onChange={(v) => setSegs((s) => s.map((x, j) => (j === i ? { ...x, grade_pct: v } : x)))} />
                    <button className="icon-btn" onClick={() => setSegs((s) => s.filter((_, j) => j !== i))} aria-label="Remove"><IconX size={14} /></button>
                  </div>
                ))}
              </div>
              <div className="row gap8 wrap">
                <Btn size="sm" onClick={() => setSegs((s) => [...s, { length_m: 1000, grade_pct: 0 }])}><IconPlus size={12} /> Segment</Btn>
                <Btn size="sm" kind="ghost" onClick={() => setSegs((s) => [...s, ...s])}>Repeat all</Btn>
                <Btn size="sm" kind="ghost" onClick={() => setSegs([{ length_m: 1500, grade_pct: 0 }, ...Array.from({ length: 4 }, () => [{ length_m: 800, grade_pct: 7 }, { length_m: 800, grade_pct: -7 }]).flat(), { length_m: 1500, grade_pct: 0 }])}>Preset: hill repeats</Btn>
                <Btn size="sm" kind="ghost" onClick={() => setSegs([{ length_m: 3000, grade_pct: 0.5 }, { length_m: 9000, grade_pct: 7.5 }, { length_m: 1000, grade_pct: 3 }])}>Preset: long climb</Btn>
              </div>
              <Field label="Start elevation">
                <NumberInput value={startEle} onChange={setStartEle} step={10} suffix="m" />
              </Field>
            </div>
          )}

          {tab === "gpx" && (
            <div className="form">
              <p className="muted">
                Import any GPX track or route (Strava, Komoot, RideWithGPS, Garmin…). Elevation is resampled every 10 m and smoothed to remove GPS noise.
              </p>
              <input
                ref={fileRef}
                type="file"
                accept=".gpx,application/gpx+xml,application/xml,text/xml"
                style={{ display: "none" }}
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  if (!f) return;
                  setGpxFile(f.name);
                  setGpxText(await f.text());
                }}
              />
              <Btn onClick={() => fileRef.current?.click()}>
                <IconUpload size={14} /> {gpxFile || "Choose GPX file…"}
              </Btn>
              <Btn kind="primary" onClick={importGpx} disabled={!gpxText}>Import route</Btn>
            </div>
          )}
          {err && <div className="banner bad mt12">{err}</div>}
        </Card>

        <div className="builder-preview">
          {tab !== "gpx" && preview ? (
            <>
              <Card>
                <ElevationProfile points={preview.route.points} height={260} climbs={preview.stats.climbs} units={units} />
              </Card>
              <div className="grid g5 mt16">
                <Card><Stat label="Distance" value={dist(preview.stats.distance_m, units)} unit={distUnit(units)} /></Card>
                <Card><Stat label="Elevation" value={elev(preview.stats.gain_m, units)} unit={elevUnit(units)} /></Card>
                <Card><Stat label="Max grade" value={preview.stats.max_grade_pct.toFixed(1)} unit="%" /></Card>
                <Card><Stat label="Difficulty" value={preview.stats.difficulty.toFixed(1)} color={difficultyColor(preview.stats.difficulty)} sub={preview.stats.difficulty_label} /></Card>
                <Card><Stat label="Est. time" value={dur(preview.est_time_s)} sub="at 70% FTP" /></Card>
              </div>
              <Card className="mt16" title={`Climbs (${preview.stats.climbs.length})`} action={<Btn kind="primary" onClick={save}>Save route</Btn>}>
                <div className="row gap8 wrap">
                  {preview.stats.climbs.length === 0 && <span className="muted">No categorised climbs.</span>}
                  {preview.stats.climbs.map((c, i) => (
                    <Pill key={i} color={categoryColor(c.category)}>
                      {c.category === "Uncategorised" ? "Climb" : c.category} · {short(c.length_m, units)} @ {c.avg_grade_pct.toFixed(1)}%
                    </Pill>
                  ))}
                </div>
              </Card>
            </>
          ) : tab === "gpx" ? (
            <Empty icon={<IconUpload size={30} />} title="Import a GPX file">The route is saved to your library and opens straight away.</Empty>
          ) : (
            <Empty title="Building preview…" />
          )}
        </div>
      </div>
    </div>
  );
}
