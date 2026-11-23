import { useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad, useUnits } from "../lib/app";
import { dateLong, dateShort, dist, distUnit, dur, elev, elevUnit, hrZoneColor, num, speed, speedUnit, timeOfDay, ZONE_COLORS, ZONE_NAMES } from "../lib/format";
import type { RideKind, RideSummary, StravaUploadEvent } from "../lib/types";
import { useTauriEvent } from "../lib/api";
import { FtpSuggestionBanner } from "../components/FtpSuggestion";
import { LineChart, ZoneBar } from "../components/charts";
import { Btn, Card, Empty, PageHeader, Pill, Segmented, Stat, confirmAsk } from "../components/ui";
import { IconDownload, IconEdit, IconFlame, IconList, IconTrash, IconTrophy } from "../components/icons";

const KIND_LABEL: Record<RideKind, string> = { route: "Route", workout: "Workout", test: "Test", free: "Free ride" };

export default function Activities() {
  const { go } = useApp();
  const units = useUnits();
  const { data } = useLoad(() => api.listRides());
  const [kind, setKind] = useState<RideKind | "all">("all");
  const list = (data ?? []).filter((r) => kind === "all" || r.kind === kind);

  const groups = useMemo(() => {
    const m = new Map<string, RideSummary[]>();
    for (const r of list) {
      const d = new Date(r.started_at);
      const k = d.toLocaleDateString(undefined, { month: "long", year: "numeric" });
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(r);
    }
    return [...m.entries()];
  }, [list]);

  return (
    <div className="page">
      <PageHeader
        title="Activities"
        sub={`${data?.length ?? 0} rides recorded`}
        actions={
          <Segmented<RideKind | "all">
            size="sm"
            value={kind}
            onChange={setKind}
            options={[{ value: "all", label: "All" }, { value: "route", label: "Routes" }, { value: "workout", label: "Workouts" }, { value: "test", label: "Tests" }, { value: "free", label: "Free" }]}
          />
        }
      />
      {data && list.length === 0 && <Empty icon={<IconList size={30} />} title="Nothing here yet">Your rides will be listed here once you save them.</Empty>}
      {groups.map(([month, rides]) => (
        <Card key={month} title={month} className="mb16" pad={false}>
          <table className="tbl">
            <thead>
              <tr>
                <th>Date</th><th>Ride</th><th>Type</th><th className="num">Time</th><th className="num">Distance</th><th className="num">Elev</th>
                <th className="num">Avg W</th><th className="num">NP</th><th className="num">TSS</th><th className="num">kJ</th><th className="num">kcal</th>
              </tr>
            </thead>
            <tbody>
              {rides.map((r) => (
                <tr key={r.id} onClick={() => go("activity", { id: r.id })}>
                  <td className="muted">{dateShort(r.started_at)} {timeOfDay(r.started_at)}</td>
                  <td>
                    <b>{r.name}</b> {r.personal_best && <Pill color="#FFD23F"><IconTrophy size={11} /> PB</Pill>}
                    {r.test_result && <Pill color="var(--accent)">FTP {r.test_result.ftp}</Pill>}
                  </td>
                  <td className="muted">{KIND_LABEL[r.kind]}</td>
                  <td className="num">{dur(r.moving_s)}</td>
                  <td className="num">{dist(r.distance_m, units)} {distUnit(units)}</td>
                  <td className="num">{elev(r.elevation_gain_m, units)} {elevUnit(units)}</td>
                  <td className="num">{Math.round(r.avg_power)}</td>
                  <td className="num">{Math.round(r.np)}</td>
                  <td className="num">{Math.round(r.tss)}</td>
                  <td className="num">{Math.round(r.work_kj)}</td>
                  <td className="num">{Math.round(r.calories_kcal)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------

export function ActivityDetail({ id, fresh }: { id: string; fresh?: boolean }) {
  const { go, back, toast, setProfile, profile } = useApp();
  const units = useUnits();
  const { data, reload } = useLoad(() => api.getRide(id), [id]);
  const stats = useLoad(() => api.accountStats());
  const strava = useLoad(() => api.stravaStatus());
  const hrZones = useLoad(() => api.hrZones());
  const [uploading, setUploading] = useState<string | null>(null);
  useTauriEvent<StravaUploadEvent>("strava://upload", (ev) => {
    if (ev.ride_id !== id) return;
    setUploading(ev.status === "done" || ev.status === "error" ? null : ev.status);
    if (ev.status === "done") reload();
  });
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [notes, setNotes] = useState("");

  const charts = useMemo(() => {
    if (!data) return null;
    const s = data.samples;
    const t = s.t;
    const hasHr = s.hr.some((h) => h > 0);
    const hasTarget = s.target.some((h) => h > 0);
    return {
      t,
      power: [
        { name: "Power", values: s.power, color: "var(--accent)", area: true, width: 1.4, format: (v: number) => `${Math.round(v)} W` },
        ...(hasTarget ? [{ name: "Target", values: s.target.map((v) => (v > 0 ? v : null)), color: "#fff", dashed: true, width: 1.2, format: (v: number) => `${Math.round(v)} W` }] : []),
      ],
      hr: hasHr ? [{ name: "Heart rate", values: s.hr.map((v) => (v > 0 ? v : null)), color: "#FF5A7A", width: 1.4, format: (v: number) => `${Math.round(v)} bpm` }] : null,
      cad: [{ name: "Cadence", values: s.cadence, color: "#4DA3FF", width: 1.2, format: (v: number) => `${Math.round(v)} rpm` }],
      spd: [{ name: "Speed", values: s.speed.map((v) => v * 3.6 / (units === "imperial" ? 1.609344 : 1)), color: "#FFD23F", width: 1.2, format: (v: number) => `${v.toFixed(1)} ${speedUnit(units)}` }],
      ele: [{ name: "Elevation", values: s.ele.map((v) => (units === "imperial" ? v * 3.28084 : v)), color: "#8A94A6", area: true, width: 1.2, format: (v: number) => `${Math.round(v)} ${elevUnit(units)}` }],
    };
  }, [data, units]);

  if (!data || !charts) return <div className="page"><PageHeader title="Loading…" back={back} /></div>;
  const r = data.summary;
  const best = new Map((stats.data?.power_curve_all ?? []).map((c) => [c.duration_s, c]));
  const totalZ = r.zone_s.reduce((a, b) => a + b, 0) || 1;

  const uploadStrava = async () => {
    setUploading("uploading");
    try {
      await api.stravaUpload(r.id);
    } catch {
      /* the event listener shows the error */
    } finally {
      setUploading(null);
      reload();
    }
  };

  const exportAs = async (format: "tcx" | "csv" | "fit") => {
    const safe = r.name.replace(/[^\w\- ]+/g, "").trim() || "ride";
    const path = await save({ defaultPath: `${safe} ${r.started_at.slice(0, 10)}.${format}`, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
    if (!path) return;
    try {
      await api.exportRide(r.id, format, path);
      toast(`Exported ${format.toUpperCase()}`, "good");
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const applyFtp = async (ftp: number) => {
    const p = await api.setFtp(ftp, r.test ? `${r.test === "ramp" ? "Ramp" : r.test === "ftp20" ? "20-min" : "2×8-min"} test` : "Test");
    setProfile(p);
    toast(`FTP updated to ${p.ftp} W`, "good");
  };

  return (
    <div className="page">
      <PageHeader
        back={fresh ? undefined : back}
        title={editing ? <input className="title-in" value={name} onChange={(e) => setName(e.target.value)} autoFocus /> : <>{r.name} {r.personal_best && <Pill color="#FFD23F" solid><IconTrophy size={12} /> Personal best</Pill>}</>}
        sub={`${dateLong(r.started_at)} · ${timeOfDay(r.started_at)} · ${KIND_LABEL[r.kind]}${r.route_name && r.kind !== "route" ? ` on ${r.route_name}` : ""}${r.laps > 1 ? ` · ${r.laps} laps` : ""}${r.route_modified ? " · course edited during ride" : ""}`}
        actions={
          editing ? (
            <>
              <Btn onClick={() => setEditing(false)}>Cancel</Btn>
              <Btn kind="primary" onClick={async () => { await api.updateRide(r.id, name, notes); setEditing(false); reload(); }}>Save</Btn>
            </>
          ) : (
            <>
              {fresh && <Btn onClick={() => go("home")}>Done</Btn>}
              <Btn kind="ghost" onClick={() => { setName(r.name); setNotes(r.notes); setEditing(true); }}><IconEdit size={14} /> Edit</Btn>
              {r.strava_activity_id ? (
                <Btn kind="ghost" onClick={() => api.openUrl(`https://www.strava.com/activities/${r.strava_activity_id}`)}>
                  <span className="strava-mark">S</span> View on Strava
                </Btn>
              ) : strava.data?.connected ? (
                <Btn kind="primary" className="strava-btn" onClick={uploadStrava} disabled={!!uploading}>
                  {uploading ? (uploading === "processing" ? "Strava is processing…" : "Uploading…") : "Upload to Strava"}
                </Btn>
              ) : null}
              <Btn kind="ghost" onClick={() => exportAs("fit")}><IconDownload size={14} /> FIT</Btn>
              <Btn kind="ghost" onClick={() => exportAs("tcx")}><IconDownload size={14} /> TCX</Btn>
              <Btn kind="ghost" onClick={() => exportAs("csv")}><IconDownload size={14} /> CSV</Btn>
              <Btn
                kind="ghost"
                onClick={async () => {
                  if (!(await confirmAsk("Delete this ride permanently?"))) return;
                  await api.deleteRide(r.id);
                  go("activities");
                }}
              >
                <IconTrash size={14} />
              </Btn>
            </>
          )
        }
      />

      {!r.test_result && <FtpSuggestionBanner rideId={r.id} />}
      {r.simulated && <div className="banner warn mb16">Ridden on the simulator – excluded from automatic FTP estimates.</div>}

      {r.test_result && (
        <Card className="test-result mb16">
          <div className="row between center">
            <div>
              <div className="eyebrow">Test result</div>
              <h2>Estimated FTP <span className="accent">{r.test_result.ftp} W</span></h2>
              <p className="muted">
                {r.test_result.detail} × {r.test_result.factor} · {(r.test_result.ftp / (r.weight_kg || 75)).toFixed(2)} W/kg
                {!r.test_result.valid && " · effort looked incomplete – treat with caution"}
              </p>
            </div>
            {profile && Math.round(profile.ftp) !== Math.round(r.test_result.ftp) ? (
              <Btn kind="primary" onClick={() => applyFtp(r.test_result!.ftp)}>Set FTP to {r.test_result.ftp} W (was {Math.round(profile.ftp)})</Btn>
            ) : (
              <Pill color="var(--accent)">This is your current FTP</Pill>
            )}
          </div>
        </Card>
      )}

      <div className="grid g6">
        <Card><Stat label="Moving time" value={dur(r.moving_s)} sub={`elapsed ${dur(r.elapsed_s)}`} /></Card>
        <Card><Stat label="Distance" value={dist(r.distance_m, units, 2)} unit={distUnit(units)} sub={`${speed(r.avg_speed_mps * 3.6, units)} ${speedUnit(units)} avg`} /></Card>
        <Card><Stat label="Elevation" value={elev(r.elevation_gain_m, units)} unit={elevUnit(units)} /></Card>
        <Card><Stat label="Avg / NP" value={`${Math.round(r.avg_power)} / ${Math.round(r.np)}`} unit="W" sub={`max ${Math.round(r.max_power)} W · VI ${r.variability_index.toFixed(2)}`} /></Card>
        <Card><Stat label="TSS / IF" value={`${Math.round(r.tss)} / ${r.intensity_factor.toFixed(2)}`} sub={`at FTP ${Math.round(r.ftp)} W`} /></Card>
        <Card>
          <Stat
            label={<><IconFlame size={12} /> Energy</>}
            value={Math.round(r.calories_kcal)}
            unit="kcal"
            sub={`${Math.round(r.work_kj)} kJ work${r.calories_hr_kcal ? ` · HR est. ${Math.round(r.calories_hr_kcal)}` : ""}`}
          />
        </Card>
      </div>
      {(r.route_time_s || r.avg_hr) && (
        <div className="grid g6 mt16">
          {r.route_time_s && <Card><Stat label="Route time" value={dur(r.route_time_s)} color={r.personal_best ? "#FFD23F" : undefined} sub={r.personal_best ? "Personal best!" : "completed"} /></Card>}
          {r.avg_hr && <Card><Stat label="Heart rate" value={Math.round(r.avg_hr)} unit="bpm" sub={`max ${Math.round(r.max_hr ?? 0)}`} /></Card>}
          <Card><Stat label="Cadence" value={Math.round(r.avg_cadence)} unit="rpm" sub={`max ${Math.round(r.max_cadence)}`} /></Card>
          <Card><Stat label="W/kg (NP)" value={(r.np / (r.weight_kg || 75)).toFixed(2)} /></Card>
        </div>
      )}

      {editing && (
        <Card className="mt16" title="Notes">
          <textarea rows={3} className="w100" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="How did it feel?" />
        </Card>
      )}
      {!editing && r.notes && <Card className="mt16" title="Notes"><p>{r.notes}</p></Card>}

      {r.splits?.length > 0 && (
        <Card className="mt16" title={`Splits (${r.splits.length})`} pad={false}>
          <table className="tbl">
            <thead>
              <tr>
                <th>Split</th><th className="num">Start</th><th className="num">Time</th><th className="num">Distance</th><th className="num">Avg grade</th>
                <th className="num">Avg W</th><th className="num">NP</th><th className="num">Max W</th><th className="num">HR</th><th className="num">Cadence</th><th className="num">Speed</th>
              </tr>
            </thead>
            <tbody>
              {r.splits.map((sp, i) => (
                <tr key={i}>
                  <td><span className={`split-k ${sp.kind}`}>{sp.kind === "sector" ? "Sector" : sp.kind === "lap" ? "Lap" : "Route lap"}</span> {sp.name}</td>
                  <td className="num muted">{dur(sp.start_s)}</td>
                  <td className="num"><b>{dur(sp.duration_s)}</b></td>
                  <td className="num">{dist(sp.distance_m, units, 2)} {distUnit(units)}</td>
                  <td className="num">{sp.avg_grade_pct.toFixed(1)}%</td>
                  <td className="num">{Math.round(sp.avg_power)}</td>
                  <td className="num">{Math.round(sp.np)}</td>
                  <td className="num">{Math.round(sp.max_power)}</td>
                  <td className="num">{sp.avg_hr ? Math.round(sp.avg_hr) : "–"}</td>
                  <td className="num">{Math.round(sp.avg_cadence)}</td>
                  <td className="num">{speed(sp.avg_speed_kmh, units)} {speedUnit(units)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <Card className="mt16" title="Power">
        <LineChart x={charts.t} series={charts.power} height={200} xFormat={(v) => dur(v)} yMin={0} />
      </Card>
      <div className="grid g2 mt16">
        {charts.hr && <Card title="Heart rate"><LineChart x={charts.t} series={charts.hr} height={140} xFormat={(v) => dur(v)} /></Card>}
        <Card title="Cadence"><LineChart x={charts.t} series={charts.cad} height={140} xFormat={(v) => dur(v)} yMin={0} /></Card>
        <Card title="Speed"><LineChart x={charts.t} series={charts.spd} height={140} xFormat={(v) => dur(v)} yMin={0} /></Card>
        <Card title="Elevation"><LineChart x={charts.t} series={charts.ele} height={140} xFormat={(v) => dur(v)} /></Card>
      </div>

      <div className="grid g2 mt16">
        <Card title="Time in power zones">
          <ZoneBar zones={r.zone_s} height={16} />
          <table className="tbl compact mt12">
            <tbody>
              {r.zone_s.map((z, i) => (
                <tr key={i}>
                  <td><i className="dot" style={{ background: ZONE_COLORS[i] }} /> Z{i + 1} {ZONE_NAMES[i]}</td>
                  <td className="num">{dur(z)}</td>
                  <td className="num muted">{Math.round((z / totalZ) * 100)}%</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
        {r.hr_zone_s?.some((x) => x > 0) && hrZones.data && (
          <Card title="Time in heart-rate zones">
            <div className="zonebar" style={{ height: 16 }}>
              {r.hr_zone_s.map((z, i) => (z > 0 ? <div key={i} style={{ width: `${(z / r.hr_zone_s.reduce((a, b) => a + b, 0)) * 100}%`, background: hrZoneColor(i + 1, r.hr_zone_s.length) }} /> : null))}
            </div>
            <table className="tbl compact mt12">
              <tbody>
                {r.hr_zone_s.map((z, i) => {
                  const hz = hrZones.data!.find((x) => x.zone === i + 1);
                  const tot = r.hr_zone_s.reduce((a, b) => a + b, 0) || 1;
                  return (
                    <tr key={i}>
                      <td><i className="dot" style={{ background: hrZoneColor(i + 1, r.hr_zone_s.length) }} /> Z{i + 1} {hz?.name}</td>
                      <td className="num muted">{hz ? (hz.hi == null ? `≥ ${hz.lo}` : `${hz.lo}–${hz.hi}`) : ""} bpm</td>
                      <td className="num">{dur(z)}</td>
                      <td className="num muted">{Math.round((z / tot) * 100)}%</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </Card>
        )}
        <Card title="Best efforts">
          <table className="tbl compact">
            <thead><tr><th>Duration</th><th className="num">Power</th><th className="num">W/kg</th><th className="num">All-time</th></tr></thead>
            <tbody>
              {r.power_bests.map(([d, w]) => {
                const b = best.get(d);
                const isBest = b && b.ride_id === r.id;
                return (
                  <tr key={d}>
                    <td>{d < 60 ? `${d} s` : d < 3600 ? `${d / 60} min` : `${d / 3600} h`}</td>
                    <td className="num"><b>{Math.round(w)}</b> W</td>
                    <td className="num">{(w / (r.weight_kg || 75)).toFixed(2)}</td>
                    <td className="num">{isBest ? <Pill color="#FFD23F">Best ever</Pill> : b ? <span className="muted">{num(b.watts)} W</span> : "–"}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </Card>
      </div>
    </div>
  );
}
