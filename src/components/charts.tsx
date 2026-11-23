import { useId, useMemo, useState, type ReactNode } from "react";
import { categoryColor, gradeColor, zoneColor, dur, short } from "../lib/format";
import type { Climb, Step, Units } from "../lib/types";
import { useSize } from "./ui";

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

type XY = { d: number; ele: number };

function toXY(points: XY[] | [number, number][]): XY[] {
  if (!points.length) return [];
  return Array.isArray(points[0]) ? (points as [number, number][]).map(([d, ele]) => ({ d, ele })) : (points as XY[]);
}

export function eleAt(pts: XY[], d: number): number {
  if (!pts.length) return 0;
  if (d <= pts[0].d) return pts[0].ele;
  const last = pts[pts.length - 1];
  if (d >= last.d) return last.ele;
  let lo = 0, hi = pts.length - 1;
  while (hi - lo > 1) {
    const m = (lo + hi) >> 1;
    if (pts[m].d <= d) lo = m;
    else hi = m;
  }
  const a = pts[lo], b = pts[hi];
  const f = b.d > a.d ? (d - a.d) / (b.d - a.d) : 0;
  return a.ele + f * (b.ele - a.ele);
}

function niceStep(span: number, target: number): number {
  const raw = span / Math.max(1, target);
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const n = raw / p;
  return (n < 1.5 ? 1 : n < 3 ? 2 : n < 7 ? 5 : 10) * p;
}

// ---------------------------------------------------------------------------
// Elevation profile
// ---------------------------------------------------------------------------

export interface ElevationProps {
  points: XY[] | [number, number][];
  height?: number;
  window?: [number, number];
  position?: number | null;
  ghost?: number | null;
  climbs?: Climb[];
  axis?: boolean;
  interactive?: boolean;
  units?: Units;
  minRange?: number;
  /** dims un-ridden terrain (live view) */
  progress?: boolean;
  className?: string;
  labels?: boolean;
  /** Highlighted sectors added during a ride */
  sectors?: { start: number; end: number; label: string; done?: boolean }[];
}

export function ElevationProfile({
  points,
  height = 180,
  window: win,
  position,
  ghost,
  climbs,
  axis = true,
  interactive = true,
  units = "metric",
  minRange = 60,
  progress = false,
  className = "",
  labels = true,
  sectors,
}: ElevationProps) {
  const pts = useMemo(() => toXY(points), [points]);
  const [ref, { w }] = useSize<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const uid = useId().replace(/:/g, "");
  const total = pts.length ? pts[pts.length - 1].d : 0;
  const d0 = win ? win[0] : 0;
  const d1 = win ? win[1] : total;
  const pad = axis ? { l: 46, r: 12, t: 18, b: 24 } : { l: 0, r: 0, t: 6, b: 0 };
  const iw = Math.max(10, w - pad.l - pad.r);
  const ih = Math.max(10, height - pad.t - pad.b);

  const geo = useMemo(() => {
    if (!pts.length || d1 <= d0) return null;
    const n = Math.max(24, Math.min(700, Math.round(iw / 2.5)));
    const s: XY[] = [];
    for (let i = 0; i <= n; i++) {
      const d = d0 + ((d1 - d0) * i) / n;
      s.push({ d, ele: eleAt(pts, d) });
    }
    let lo = Infinity, hi = -Infinity;
    for (const p of s) {
      lo = Math.min(lo, p.ele);
      hi = Math.max(hi, p.ele);
    }
    const range = Math.max(hi - lo, minRange);
    const yMin = lo - range * 0.12;
    const yMax = lo + range * 1.12;
    return { s, yMin, yMax };
  }, [pts, d0, d1, iw, minRange]);

  if (!geo) return <div ref={ref} className={`elev ${className}`} style={{ height }} />;
  const { s, yMin, yMax } = geo;
  const x = (d: number) => pad.l + ((d - d0) / (d1 - d0)) * iw;
  const y = (e: number) => pad.t + ih - ((e - yMin) / (yMax - yMin)) * ih;
  const base = pad.t + ih;

  // One seamless fill path coloured by a horizontal gradient of the local grade.
  const stops: ReactNode[] = [];
  for (let i = 0; i < s.length - 1; i++) {
    const a = s[i], b = s[i + 1];
    const g = ((b.ele - a.ele) / Math.max(1e-6, b.d - a.d)) * 100;
    const c = gradeColor(g);
    stops.push(<stop key={`a${i}`} offset={(a.d - d0) / (d1 - d0)} stopColor={c} />);
    stops.push(<stop key={`b${i}`} offset={(b.d - d0) / (d1 - d0)} stopColor={c} />);
  }
  const fillPath =
    s.map((p, i) => `${i ? "L" : "M"}${x(p.d).toFixed(1)},${y(p.ele).toFixed(1)}`).join("") +
    `L${x(s[s.length - 1].d).toFixed(1)},${base}L${x(s[0].d).toFixed(1)},${base}Z`;
  const posX = position != null ? Math.max(pad.l, Math.min(pad.l + iw, x(position))) : null;
  const line = s.map((p, i) => `${i ? "L" : "M"}${x(p.d).toFixed(1)},${y(p.ele).toFixed(1)}`).join("");

  const ticks: ReactNode[] = [];
  if (axis && w > 0) {
    const unitM = units === "imperial" ? 1609.344 : 1000;
    const span = (d1 - d0) / unitM;
    const st = niceStep(span, Math.max(2, Math.floor(iw / 90)));
    const first = Math.ceil(d0 / unitM / st) * st;
    for (let v = first; v <= d1 / unitM + 1e-9; v += st) {
      const px = x(v * unitM);
      ticks.push(
        <g key={v}>
          <line x1={px} x2={px} y1={base} y2={base + 4} stroke="var(--muted-2)" />
          <text x={px} y={base + 16} textAnchor="middle" className="ax">
            {Number(v.toFixed(2))}
            {v === first ? (units === "imperial" ? " mi" : " km") : ""}
          </text>
        </g>
      );
    }
    const eu = units === "imperial" ? 3.28084 : 1;
    const est = niceStep((yMax - yMin) * eu, 3);
    for (let v = Math.ceil((yMin * eu) / est) * est; v <= yMax * eu; v += est) {
      const py = y(v / eu);
      ticks.push(
        <g key={`y${v}`}>
          <line x1={pad.l} x2={pad.l + iw} y1={py} y2={py} stroke="var(--line)" strokeDasharray="2 4" />
          <text x={pad.l - 8} y={py + 4} textAnchor="end" className="ax">
            {Math.round(v)}
          </text>
        </g>
      );
    }
  }

  const climbEls: ReactNode[] = [];
  if (climbs && labels) {
    climbs.forEach((c, i) => {
      if (c.end_d < d0 || c.start_d > d1) return;
      const xa = x(Math.max(d0, c.start_d)), xb = x(Math.min(d1, c.end_d));
      const col = categoryColor(c.category);
      const top = y(c.top_ele);
      climbEls.push(
        <g key={i} className="climb-mark">
          <line x1={xa} x2={xb} y1={pad.t - 6} y2={pad.t - 6} stroke={col} strokeWidth={3} strokeLinecap="round" opacity={0.9} />
          {c.category !== "Uncategorised" && xb - xa > 26 && (
            <text x={(xa + xb) / 2} y={Math.max(pad.t + 10, top - 8)} textAnchor="middle" className="climb-t" fill={col}>
              {c.category}
            </text>
          )}
        </g>
      );
    });
  }

  const sectorEls: ReactNode[] = [];
  (sectors ?? []).forEach((sc, i) => {
    if (sc.end < d0 || sc.start > d1) return;
    const xa = x(Math.max(d0, sc.start)), xb = x(Math.min(d1, sc.end));
    sectorEls.push(
      <g key={`s${i}`} className={`sector-band ${sc.done ? "done" : ""}`}>
        <rect x={xa} y={pad.t} width={Math.max(1, xb - xa)} height={base - pad.t} />
        <line x1={xa} x2={xa} y1={pad.t} y2={base} />
        <line x1={xb} x2={xb} y1={pad.t} y2={base} />
        {xb - xa > 40 && labels !== false && (
          <text x={(xa + xb) / 2} y={pad.t + 11} textAnchor="middle">{sc.label.length > 22 ? sc.label.slice(0, 21) + "…" : sc.label}</text>
        )}
      </g>
    );
  });

  const marker = (d: number, cls: string, r: number) => {
    if (d < d0 || d > d1) return null;
    const px = x(d), py = y(eleAt(pts, d));
    return (
      <g className={cls}>
        <line x1={px} x2={px} y1={py} y2={base} />
        <circle cx={px} cy={py} r={r} />
      </g>
    );
  };

  const onMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!interactive) return;
    const r = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
    const px = e.clientX - r.left;
    if (px < pad.l || px > pad.l + iw) return setHover(null);
    setHover(d0 + ((px - pad.l) / iw) * (d1 - d0));
  };

  let tip: ReactNode = null;
  if (hover != null) {
    const e0 = eleAt(pts, hover - 25), e1 = eleAt(pts, hover + 25);
    const g = ((e1 - e0) / 50) * 100;
    const px = x(hover), py = y(eleAt(pts, hover));
    const left = Math.min(Math.max(px + 10, 0), w - 150);
    tip = (
      <>
        <line x1={px} x2={px} y1={pad.t} y2={base} stroke="var(--text)" strokeOpacity={0.5} />
        <circle cx={px} cy={py} r={4} fill="var(--text)" />
        <foreignObject x={left} y={pad.t} width={140} height={58}>
          <div className="chart-tip">
            <b>{short(hover, units)}</b>
            <span>
              {Math.round(units === "imperial" ? eleAt(pts, hover) * 3.28084 : eleAt(pts, hover))} {units === "imperial" ? "ft" : "m"}
            </span>
            <span style={{ color: gradeColor(g) }}>{g.toFixed(1)}%</span>
          </div>
        </foreignObject>
      </>
    );
  }

  return (
    <div ref={ref} className={`elev ${className}`} style={{ height }}>
      {w > 0 && (
        <svg width={w} height={height} onMouseMove={onMove} onMouseLeave={() => setHover(null)}>
          <defs>
            <linearGradient id={`fade${uid}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="#fff" stopOpacity="1" />
              <stop offset="1" stopColor="#fff" stopOpacity="0.25" />
            </linearGradient>
            <mask id={`m${uid}`}>
              <rect x="0" y="0" width={w} height={height} fill={`url(#fade${uid})`} />
            </mask>
            <linearGradient id={`gr${uid}`} gradientUnits="userSpaceOnUse" x1={pad.l} x2={pad.l + iw} y1="0" y2="0">
              {stops}
            </linearGradient>
            {progress && posX != null && (
              <clipPath id={`cl${uid}`}>
                <rect x={0} y={0} width={posX} height={height} />
              </clipPath>
            )}
          </defs>
          {ticks}
          <g mask={`url(#m${uid})`}>
            <path d={fillPath} fill={`url(#gr${uid})`} opacity={progress ? 0.4 : 0.88} />
            {progress && posX != null && <path d={fillPath} fill={`url(#gr${uid})`} clipPath={`url(#cl${uid})`} opacity={0.95} />}
          </g>
          <path d={line} fill="none" stroke="var(--ink)" strokeOpacity={0.75} strokeWidth={axis ? 1.6 : 1.2} strokeLinejoin="round" />
          {sectorEls}
          {climbEls}
          {ghost != null && marker(ghost, "ghost-mark", 6)}
          {position != null && marker(position, "rider-mark", 7)}
          {tip}
        </svg>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Workout chart
// ---------------------------------------------------------------------------

export function WorkoutChart({ steps, ftp, t, height = 120, axis = false, bias = 1 }: { steps: Step[]; ftp: number; t?: number | null; height?: number; axis?: boolean; bias?: number }) {
  const [ref, { w }] = useSize<HTMLDivElement>();
  const uid = useId().replace(/:/g, "");
  const total = steps.length ? steps[steps.length - 1].start_s + steps[steps.length - 1].duration_s : 0;
  const maxF = Math.max(1.05, ...steps.map((s) => (s.target.type === "erg" ? Math.max(s.target.from, s.target.to) * bias : 0.7))) * 1.08;
  const pad = axis ? { l: 34, b: 18, t: 6 } : { l: 0, b: 0, t: 2 };
  const iw = Math.max(10, w - pad.l);
  const ih = height - pad.b - pad.t;
  const x = (s: number) => pad.l + (s / Math.max(1, total)) * iw;
  const y = (f: number) => pad.t + ih - (f / maxF) * ih;
  return (
    <div ref={ref} className="wchart" style={{ height }}>
      {w > 0 && total > 0 && (
        <svg width={w} height={height}>
          <defs>
            <pattern id={`h${uid}`} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <rect width="6" height="6" fill="rgba(15,23,42,.04)" />
              <line x1="0" y1="0" x2="0" y2="6" stroke="rgba(15,23,42,.16)" strokeWidth="2" />
            </pattern>
          </defs>
          {axis && (
            <>
              {[0.5, 1].map((f) => (
                <g key={f}>
                  <line x1={pad.l} x2={pad.l + iw} y1={y(f)} y2={y(f)} stroke="var(--line)" strokeDasharray={f === 1 ? "4 3" : "2 5"} />
                  <text x={pad.l - 6} y={y(f) + 4} textAnchor="end" className="ax">{Math.round(f * ftp)}</text>
                </g>
              ))}
              <text x={pad.l} y={height - 4} className="ax">0:00</text>
              <text x={pad.l + iw} y={height - 4} textAnchor="end" className="ax">{dur(total)}</text>
            </>
          )}
          {steps.map((s, i) => {
            const xa = x(s.start_s), xb = x(s.start_s + s.duration_s);
            const done = t != null && s.start_s + s.duration_s <= t;
            const op = t == null ? 0.92 : done ? 0.35 : 0.95;
            if (s.target.type === "free") {
              return (
                <g key={i} opacity={op}>
                  <rect x={xa} y={y(0.7)} width={Math.max(0.5, xb - xa - 0.5)} height={y(0) - y(0.7)} fill={`url(#h${uid})`} rx={1.5} />
                  {s.effort && <rect x={xa} y={y(0.7)} width={Math.max(0.5, xb - xa - 0.5)} height={2} fill="var(--accent)" />}
                </g>
              );
            }
            const a = s.target.from * bias, b = s.target.to * bias;
            return (
              <polygon
                key={i}
                points={`${xa},${y(0)} ${xa},${y(a)} ${xb - 0.5},${y(b)} ${xb - 0.5},${y(0)}`}
                fill={zoneColor(((a + b) / 2) * ftp, ftp)}
                opacity={op}
              />
            );
          })}
          {t != null && t <= total && (
            <g>
              <line x1={x(t)} x2={x(t)} y1={pad.t} y2={pad.t + ih} stroke="var(--ink)" strokeWidth={2} />
              <circle cx={x(t)} cy={pad.t + 3} r={4} fill="var(--ink)" />
            </g>
          )}
        </svg>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Line chart
// ---------------------------------------------------------------------------

export interface Series {
  name: string;
  values: (number | null)[];
  color: string;
  area?: boolean;
  dashed?: boolean;
  width?: number;
  format?: (v: number) => string;
  axis?: "left" | "right";
}

export function LineChart({
  x,
  series,
  height = 200,
  xFormat = (v) => String(v),
  yFormat: yFormatProp,
  logX,
  yMin,
  yMax,
  xTicks,
  zeroLine,
}: {
  x: number[];
  series: Series[];
  height?: number;
  xFormat?: (v: number) => string;
  yFormat?: (v: number) => string;
  logX?: boolean;
  yMin?: number;
  yMax?: number;
  xTicks?: number[];
  zeroLine?: boolean;
}) {
  const [ref, { w }] = useSize<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const uid = useId().replace(/:/g, "");
  const pad = { l: 44, r: 12, t: 10, b: 24 };
  const iw = Math.max(10, w - pad.l - pad.r);
  const ih = height - pad.t - pad.b;

  const prep = useMemo(() => {
    if (x.length < 2) return null;
    // bucket-average long series to ~2 px resolution
    const buckets = Math.max(2, Math.floor(iw / 2));
    const n = x.length;
    let idx: number[][];
    if (n > buckets * 1.5 && !logX) {
      idx = [];
      for (let b = 0; b < buckets; b++) {
        const a = Math.floor((b * n) / buckets), e = Math.max(a + 1, Math.floor(((b + 1) * n) / buckets));
        idx.push([a, e]);
      }
    } else {
      idx = x.map((_, i) => [i, i + 1]);
    }
    const xs = idx.map(([a, e]) => x[Math.floor((a + e - 1) / 2)]);
    const ss = series.map((s) =>
      idx.map(([a, e]) => {
        let sum = 0, c = 0;
        for (let i = a; i < e; i++) {
          const v = s.values[i];
          if (v != null && isFinite(v)) {
            sum += v;
            c++;
          }
        }
        return c ? sum / c : null;
      })
    );
    let lo = Infinity, hi = -Infinity;
    ss.forEach((vals, k) => {
      if (series[k].axis === "right") return;
      vals.forEach((v) => {
        if (v != null) {
          lo = Math.min(lo, v);
          hi = Math.max(hi, v);
        }
      });
    });
    if (!isFinite(lo)) {
      lo = 0;
      hi = 1;
    }
    if (yMin != null) lo = yMin;
    if (yMax != null) hi = yMax;
    if (hi - lo < 1e-6) hi = lo + 1;
    const span = hi - lo;
    return { xs, ss, lo: yMin ?? lo - span * 0.05, hi: yMax ?? hi + span * 0.08 };
  }, [x, series, iw, logX, yMin, yMax]);

  if (!prep) return <div ref={ref} style={{ height }} className="lchart" />;
  const { xs, ss, lo, hi } = prep;
  const x0 = x[0], x1 = x[x.length - 1];
  const fx = (v: number) =>
    logX ? pad.l + ((Math.log(v) - Math.log(x0)) / (Math.log(x1) - Math.log(x0))) * iw : pad.l + ((v - x0) / (x1 - x0 || 1)) * iw;
  const fy = (v: number) => pad.t + ih - ((v - lo) / (hi - lo)) * ih;

  const path = (vals: (number | null)[]) => {
    let d = "";
    let pen = false;
    vals.forEach((v, i) => {
      if (v == null) {
        pen = false;
        return;
      }
      d += `${pen ? "L" : "M"}${fx(xs[i]).toFixed(1)},${fy(v).toFixed(1)}`;
      pen = true;
    });
    return d;
  };

  const yt = niceStep(hi - lo, 4);
  const yFormat = yFormatProp ?? ((v: number) => v.toFixed(Math.max(0, Math.min(2, -Math.floor(Math.log10(yt))))));
  const yTicks: number[] = [];
  for (let v = Math.ceil(lo / yt) * yt; v <= hi; v += yt) yTicks.push(v);
  const xt = xTicks ?? (() => {
    const st = niceStep(x1 - x0, Math.max(2, Math.floor(iw / 110)));
    const out: number[] = [];
    for (let v = Math.ceil(x0 / st) * st; v <= x1; v += st) out.push(v);
    return out;
  })();

  const onMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - r.left;
    if (px < pad.l || px > pad.l + iw) return setHover(null);
    let best = 0, bd = Infinity;
    xs.forEach((v, i) => {
      const dd = Math.abs(fx(v) - px);
      if (dd < bd) {
        bd = dd;
        best = i;
      }
    });
    setHover(best);
  };

  return (
    <div ref={ref} className="lchart" style={{ height }}>
      {w > 0 && (
        <svg width={w} height={height} onMouseMove={onMove} onMouseLeave={() => setHover(null)}>
          <defs>
            {series.map((s, k) => (
              <linearGradient key={k} id={`g${uid}${k}`} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stopColor={s.color} stopOpacity="0.35" />
                <stop offset="1" stopColor={s.color} stopOpacity="0" />
              </linearGradient>
            ))}
          </defs>
          {yTicks.map((v) => (
            <g key={v}>
              <line x1={pad.l} x2={pad.l + iw} y1={fy(v)} y2={fy(v)} stroke="var(--line)" strokeDasharray={zeroLine && Math.abs(v) < 1e-9 ? undefined : "2 5"} />
              <text x={pad.l - 8} y={fy(v) + 4} textAnchor="end" className="ax">{yFormat(v)}</text>
            </g>
          ))}
          {xt.map((v) => (
            <text key={v} x={fx(v)} y={height - 6} textAnchor="middle" className="ax">{xFormat(v)}</text>
          ))}
          {ss.map((vals, k) => {
            const s = series[k];
            if (s.axis === "right") return null;
            const p = path(vals);
            const firstI = vals.findIndex((v) => v != null);
            let lastI = vals.length - 1;
            while (lastI > 0 && vals[lastI] == null) lastI--;
            return (
              <g key={k}>
                {s.area && firstI >= 0 && (
                  <path d={`${p}L${fx(xs[lastI])},${fy(lo)}L${fx(xs[firstI])},${fy(lo)}Z`} fill={`url(#g${uid}${k})`} />
                )}
                <path d={p} fill="none" stroke={s.color} strokeWidth={s.width ?? 1.6} strokeDasharray={s.dashed ? "5 4" : undefined} strokeLinejoin="round" />
              </g>
            );
          })}
          {hover != null && (
            <g>
              <line x1={fx(xs[hover])} x2={fx(xs[hover])} y1={pad.t} y2={pad.t + ih} stroke="var(--text)" strokeOpacity={0.4} />
              {ss.map((vals, k) =>
                vals[hover] != null && series[k].axis !== "right" ? <circle key={k} cx={fx(xs[hover])} cy={fy(vals[hover]!)} r={3.5} fill={series[k].color} /> : null
              )}
              <foreignObject x={Math.min(fx(xs[hover]) + 10, w - 170)} y={pad.t} width={160} height={24 + series.length * 18}>
                <div className="chart-tip">
                  <b>{xFormat(xs[hover])}</b>
                  {series.map((s, k) =>
                    ss[k][hover] != null ? (
                      <span key={k}>
                        <i style={{ background: s.color }} />
                        {s.name}: {(s.format ?? yFormat)(ss[k][hover]!)}
                      </span>
                    ) : null
                  )}
                </div>
              </foreignObject>
            </g>
          )}
        </svg>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Bar chart
// ---------------------------------------------------------------------------

export function BarChart({ labels, values, height = 160, color = "var(--accent)", format = (v: number) => v.toFixed(1), goal, highlightLast = true }: {
  labels: string[];
  values: number[];
  height?: number;
  color?: string;
  format?: (v: number) => string;
  goal?: number;
  highlightLast?: boolean;
}) {
  const [ref, { w }] = useSize<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const pad = { l: 8, r: 8, t: 18, b: 22 };
  const iw = Math.max(10, w - pad.l - pad.r);
  const ih = height - pad.t - pad.b;
  const max = Math.max(1e-6, goal ?? 0, ...values) * 1.1;
  const bw = iw / Math.max(1, values.length);
  return (
    <div ref={ref} className="bchart" style={{ height }}>
      {w > 0 && (
        <svg width={w} height={height}>
          {goal != null && goal > 0 && (
            <g>
              <line x1={pad.l} x2={pad.l + iw} y1={pad.t + ih - (goal / max) * ih} y2={pad.t + ih - (goal / max) * ih} stroke="var(--muted)" strokeDasharray="4 4" />
              <text x={pad.l + iw} y={pad.t + ih - (goal / max) * ih - 4} textAnchor="end" className="ax">goal</text>
            </g>
          )}
          {values.map((v, i) => {
            const h = (v / max) * ih;
            const last = highlightLast && i === values.length - 1;
            return (
              <g key={i} onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover(null)}>
                <rect x={pad.l + i * bw} y={pad.t} width={bw} height={ih} fill="transparent" />
                <rect
                  x={pad.l + i * bw + bw * 0.18}
                  y={pad.t + ih - h}
                  width={bw * 0.64}
                  height={Math.max(h, v > 0 ? 2 : 0)}
                  rx={Math.min(4, bw * 0.2)}
                  fill={color}
                  opacity={last ? 1 : hover === i ? 0.9 : 0.55}
                />
                {(hover === i || last) && v > 0 && (
                  <text x={pad.l + i * bw + bw / 2} y={pad.t + ih - h - 5} textAnchor="middle" className="bar-v">{format(v)}</text>
                )}
                {((i % Math.ceil(values.length / 8) === 0 && values.length - 1 - i >= Math.ceil(values.length / 8)) || i === values.length - 1) && (
                  <text x={pad.l + i * bw + bw / 2} y={height - 6} textAnchor="middle" className="ax">{labels[i]}</text>
                )}
              </g>
            );
          })}
        </svg>
      )}
    </div>
  );
}

/** Horizontal stacked bar of time-in-zone. */
export function ZoneBar({ zones, height = 14 }: { zones: number[]; height?: number }) {
  const total = zones.reduce((a, b) => a + b, 0) || 1;
  const cols = ["#9AA3B2", "#2F7FE0", "#16B57A", "#E3A600", "#F07A1A", "#E5484D", "#8E4EC6"];
  return (
    <div className="zonebar" style={{ height }}>
      {zones.map((z, i) =>
        z > 0 ? <div key={i} style={{ width: `${(z / total) * 100}%`, background: cols[i] }} title={`Z${i + 1}: ${dur(z)}`} /> : null
      )}
    </div>
  );
}
