import { useState } from "react";
import { api } from "../lib/api";
import { useApp, useLoad, useUnits } from "../lib/app";
import { dateShort, dist, distUnit, dur, elev, elevUnit, hours, num, ZONE_COLORS, ZONE_NAMES } from "../lib/format";
import type { RecordRef, Totals } from "../lib/types";
import { BarChart, LineChart, ZoneBar } from "../components/charts";
import { Card, Empty, PageHeader, Segmented, Stat } from "../components/ui";
import { IconChart, IconFlame, IconMountain, IconTrophy } from "../components/icons";

type Period = "week" | "month" | "year" | "all";
type Metric = "hours" | "distance" | "tss" | "elevation";

export default function Stats() {
  const { go, profile } = useApp();
  const units = useUnits();
  const { data: s } = useLoad(() => api.accountStats());
  const [period, setPeriod] = useState<Period>("month");
  const [metric, setMetric] = useState<Metric>("hours");
  const [wkg, setWkg] = useState(false);

  if (!s) return <div className="page"><PageHeader title="Stats" /></div>;
  const t: Totals = period === "week" ? s.this_week : period === "month" ? s.this_month : period === "year" ? s.this_year : s.all_time;
  const fit = s.fitness[s.fitness.length - 1];

  const weekly = s.weekly.map((w) => {
    const v = w.totals;
    return metric === "hours" ? v.moving_s / 3600 : metric === "distance" ? (units === "imperial" ? v.distance_m / 1609.344 : v.distance_m / 1000) : metric === "tss" ? v.tss : units === "imperial" ? v.elevation_m * 3.28084 : v.elevation_m;
  });

  const curveX = s.power_curve_all.map((c) => c.duration_s);
  const c90 = new Map(s.power_curve_90d.map((c) => [c.duration_s, c]));
  const durLabel = (d: number) => (d < 60 ? `${d}s` : d < 3600 ? `${Math.round(d / 60)}m` : `${(d / 3600).toFixed(d % 3600 ? 1 : 0)}h`);
  const zTotal = s.zone_s_28d.reduce((a, b) => a + b, 0);

  const rec = (label: string, r: RecordRef | null, fmt: (v: number) => string) =>
    r && (
      <li onClick={() => go("activity", { id: r.ride_id })}>
        <span className="muted">{label}</span>
        <b>{fmt(r.value)}</b>
        <span className="muted small">{r.ride_name} · {dateShort(r.date)}</span>
      </li>
    );

  return (
    <div className="page">
      <PageHeader
        title="Stats"
        sub={`${profile?.name ?? "Rider"} · FTP ${Math.round(s.ftp)} W · ${s.wkg.toFixed(2)} W/kg · ${s.week_streak}-week streak`}
        actions={<Segmented<Period> value={period} onChange={setPeriod} options={[{ value: "week", label: "Week" }, { value: "month", label: "Month" }, { value: "year", label: "Year" }, { value: "all", label: "All time" }]} />}
      />
      {s.all_time.rides === 0 && <Empty icon={<IconChart size={30} />} title="No rides yet">Stats fill in as you ride. Fitness, fatigue and your power curve all build from your saved rides.</Empty>}
      <div className="grid g6">
        <Card><Stat label="Rides" value={t.rides} /></Card>
        <Card><Stat label="Time" value={hours(t.moving_s)} unit="h" /></Card>
        <Card><Stat label="Distance" value={dist(t.distance_m, units, 0)} unit={distUnit(units)} /></Card>
        <Card><Stat label={<><IconMountain size={12} /> Climbed</>} value={elev(t.elevation_m, units)} unit={elevUnit(units)} /></Card>
        <Card><Stat label="Work" value={num(t.work_kj)} unit="kJ" sub={<><IconFlame size={11} /> {num(t.calories_kcal)} kcal</>} /></Card>
        <Card><Stat label="Training load" value={num(t.tss)} unit="TSS" /></Card>
      </div>

      <div className="grid g2 mt16">
        <Card
          title="Weekly volume"
          action={<Segmented<Metric> size="sm" value={metric} onChange={setMetric} options={[{ value: "hours", label: "Hours" }, { value: "distance", label: distUnit(units) }, { value: "elevation", label: elevUnit(units) }, { value: "tss", label: "TSS" }]} />}
        >
          <BarChart
            labels={s.weekly.map((w) => dateShort(w.week_start))}
            values={weekly}
            goal={metric === "hours" ? s.weekly_goal_hours : undefined}
            format={(v) => (metric === "hours" ? `${v.toFixed(1)}h` : num(v))}
            height={220}
          />
        </Card>
        <Card
          title="Fitness & form"
          action={fit && <span className="muted small">Fitness {fit.ctl.toFixed(0)} · Fatigue {fit.atl.toFixed(0)} · Form {(fit.ctl - fit.atl).toFixed(0)}</span>}
        >
          <LineChart
            x={s.fitness.map((f) => new Date(f.date).getTime())}
            series={[
              { name: "Fitness (CTL)", values: s.fitness.map((f) => f.ctl), color: "#2F7FE0", area: true, width: 2 },
              { name: "Fatigue (ATL)", values: s.fitness.map((f) => f.atl), color: "#E5466B", width: 1.4 },
              { name: "Form (TSB)", values: s.fitness.map((f) => f.tsb), color: "#E3A600", width: 1.4, dashed: true },
            ]}
            height={220}
            xFormat={(v) => dateShort(new Date(v).toISOString())}
            zeroLine
          />
        </Card>
      </div>

      <div className="grid g2 mt16">
        <Card title="Power curve" action={<Segmented<string> size="sm" value={wkg ? "wkg" : "w"} onChange={(v) => setWkg(v === "wkg")} options={[{ value: "w", label: "Watts" }, { value: "wkg", label: "W/kg" }]} />}>
          {curveX.length > 1 ? (
            <LineChart
              x={curveX}
              logX
              xTicks={curveX.filter((d) => [5, 60, 300, 1200, 3600].includes(d))}
              series={[
                { name: "All time", values: s.power_curve_all.map((c) => (wkg ? c.wkg : c.watts)), color: "var(--accent)", width: 2.2, area: true },
                { name: "Last 90 days", values: curveX.map((d) => { const c = c90.get(d); return c ? (wkg ? c.wkg : c.watts) : null; }), color: "#7259E8", width: 1.6, dashed: true },
              ]}
              height={240}
              xFormat={durLabel}
              yFormat={(v) => (wkg ? v.toFixed(1) : `${Math.round(v)}`)}
            />
          ) : (
            <div className="empty-inline">Ride for a few minutes to start your power curve.</div>
          )}
        </Card>
        <Card title="Last 28 days – time in zone">
          {zTotal > 0 ? (
            <>
              <ZoneBar zones={s.zone_s_28d} height={18} />
              <div className="zone-rows mt12">
                {s.zone_s_28d.map((z, i) => (
                  <div key={i} className="zone-row">
                    <span><i className="dot" style={{ background: ZONE_COLORS[i] }} />Z{i + 1} {ZONE_NAMES[i]}</span>
                    <div className="zr-bar"><i style={{ width: `${(z / Math.max(...s.zone_s_28d)) * 100}%`, background: ZONE_COLORS[i] }} /></div>
                    <span className="num">{dur(z)}</span>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <div className="empty-inline">No rides in the last 28 days.</div>
          )}
        </Card>
      </div>

      <div className="grid g2 mt16">
        <Card title={<span className="row gap6 center"><IconTrophy size={15} /> Records</span>}>
          <ul className="records">
            {rec("Longest ride", s.records.longest_distance, (v) => `${dist(v, units)} ${distUnit(units)}`)}
            {rec("Longest time", s.records.longest_time, (v) => dur(v))}
            {rec("Most climbing", s.records.most_elevation, (v) => `${elev(v, units)} ${elevUnit(units)}`)}
            {rec("Most work", s.records.most_work, (v) => `${num(v)} kJ`)}
            {rec("Highest NP (20 min+)", s.records.highest_np, (v) => `${Math.round(v)} W`)}
            {rec("Biggest TSS", s.records.highest_tss, (v) => num(v))}
          </ul>
          {!s.records.longest_distance && <div className="empty-inline">Records appear after your first ride.</div>}
        </Card>
        <Card title="Best powers">
          <div className="bests">
            {s.power_curve_all.filter((c) => [5, 60, 300, 1200, 3600].includes(c.duration_s)).map((c) => (
              <div key={c.duration_s} onClick={() => go("activity", { id: c.ride_id })}>
                <span className="muted">{durLabel(c.duration_s)}</span>
                <b>{Math.round(c.watts)} W</b>
                <span className="muted small">{c.wkg.toFixed(2)} W/kg</span>
              </div>
            ))}
          </div>
          <div className="row gap24 mt16">
            <Stat label="All-time rides" value={s.all_time.rides} />
            <Stat label="All-time hours" value={hours(s.all_time.moving_s)} />
            <Stat label="All-time distance" value={dist(s.all_time.distance_m, units, 0)} unit={distUnit(units)} />
          </div>
        </Card>
      </div>
    </div>
  );
}
