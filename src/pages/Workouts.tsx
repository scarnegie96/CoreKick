import { useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad } from "../lib/app";
import { dur, durWords, ZONE_COLORS, ZONE_NAMES } from "../lib/format";
import type { Block, Workout, WorkoutView } from "../lib/types";
import { WorkoutChart, ZoneBar } from "../components/charts";
import { Btn, Card, Empty, Field, NumberInput, PageHeader, Pill, Segmented, Stat, Toggle, confirmAsk } from "../components/ui";
import { IconCopy, IconDownload, IconEdit, IconPlay, IconPlus, IconTrash, IconUpload, IconWorkout, IconX } from "../components/icons";

type DurFilter = "all" | "short" | "mid" | "long";
type Sort = "default" | "short" | "long" | "tss";

export default function Workouts() {
  const { go, prepareRide, profile, toast } = useApp();
  const { data, reload } = useLoad(() => api.listWorkouts());
  const [cat, setCat] = useState("All");
  const [q, setQ] = useState("");
  const [len, setLen] = useState<DurFilter>("all");
  const [sort, setSort] = useState<Sort>("default");
  const fileRef = useRef<HTMLInputElement>(null);
  const cats = useMemo(() => ["All", "Mine", ...Array.from(new Set((data ?? []).map((w) => w.workout.category)))], [data]);
  const ftp = profile?.ftp ?? 200;

  const list = useMemo(() => {
    let v = (data ?? []).filter((w) => {
      if (cat === "Mine" ? w.workout.builtin : cat !== "All" && w.workout.category !== cat) return false;
      const d = w.estimate.duration_s / 60;
      if (len === "short" && d > 45) return false;
      if (len === "mid" && (d <= 45 || d > 75)) return false;
      if (len === "long" && d <= 75) return false;
      const hay = `${w.workout.name} ${w.workout.description} ${w.workout.category}`.toLowerCase();
      return hay.includes(q.toLowerCase());
    });
    if (sort === "short") v = [...v].sort((a, b) => a.estimate.duration_s - b.estimate.duration_s);
    if (sort === "long") v = [...v].sort((a, b) => b.estimate.duration_s - a.estimate.duration_s);
    if (sort === "tss") v = [...v].sort((a, b) => b.estimate.tss - a.estimate.tss);
    return v;
  }, [data, cat, q, len, sort]);

  const newWorkout = (): Workout => ({
    id: "",
    name: "",
    description: "",
    category: "Custom",
    builtin: false,
    created_at: "",
    blocks: [
      { type: "ramp", duration_s: 600, from: 0.45, to: 0.75, label: "Warm-up" },
      { type: "intervals", repeats: 4, on_s: 240, on_power: 1.05, off_s: 120, off_power: 0.5 },
      { type: "ramp", duration_s: 300, from: 0.6, to: 0.4, label: "Cool-down" },
    ],
  });

  const onImport = async (f: File) => {
    try {
      const v = await api.importWorkout(f.name, await f.text());
      toast(`Imported “${v.workout.name}”`, "good");
      reload();
      go("workout-edit", { workout: v.workout });
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const exportZwo = async (w: Workout) => {
    const safe = w.name.replace(/[^\w\- ]+/g, "").trim() || "workout";
    const path = await save({ defaultPath: `${safe}.zwo`, filters: [{ name: "Zwift workout", extensions: ["zwo"] }] });
    if (!path) return;
    try {
      await api.exportWorkout(w.id, path);
      toast("Exported .zwo – works in Zwift, TrainerRoad, MyWhoosh and more", "good");
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  return (
    <div className="page">
      <PageHeader
        title="Workouts"
        sub={`${data?.length ?? 0} structured sessions scaled to your FTP (${Math.round(ftp)} W). Import Zwift .zwo, .erg or .mrc files, or write your own.`}
        actions={
          <>
            <input
              ref={fileRef}
              type="file"
              accept=".zwo,.erg,.mrc,.txt,.xml"
              style={{ display: "none" }}
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) onImport(f);
                e.target.value = "";
              }}
            />
            <Btn onClick={() => fileRef.current?.click()}><IconUpload size={14} /> Import</Btn>
            <Btn kind="primary" onClick={() => go("workout-edit", { workout: newWorkout() })}><IconPlus size={14} /> New workout</Btn>
          </>
        }
      />
      <div className="row gap12 center wrap mb12">
        <input className="search" placeholder="Search workouts…" value={q} onChange={(e) => setQ(e.target.value)} />
        <Segmented<DurFilter> size="sm" value={len} onChange={setLen} options={[{ value: "all", label: "Any length" }, { value: "short", label: "≤ 45 min" }, { value: "mid", label: "45–75" }, { value: "long", label: "75 min +" }]} />
        <Segmented<Sort> size="sm" value={sort} onChange={setSort} options={[{ value: "default", label: "Default" }, { value: "short", label: "Shortest" }, { value: "long", label: "Longest" }, { value: "tss", label: "Hardest" }]} />
      </div>
      <div className="row gap6 wrap mb16">
        {cats.map((c) => (
          <button key={c} className={`chip ${c === cat ? "on" : ""}`} onClick={() => setCat(c)}>{c}</button>
        ))}
      </div>
      {data && list.length === 0 && <Empty icon={<IconWorkout size={30} />} title="No workouts match">Try another filter, import a file or build one.</Empty>}
      <div className="cards wide">
        {list.map((w) => (
          <WorkoutCard
            key={w.workout.id}
            w={w}
            ftp={ftp}
            onRide={() => prepareRide({ kind: "workout", workout_id: w.workout.id })}
            onEdit={() => go("workout-edit", { workout: w.workout.builtin ? { ...w.workout, id: "", builtin: false, name: `${w.workout.name} (copy)` } : w.workout })}
            onExport={() => exportZwo(w.workout)}
            onDelete={
              w.workout.builtin
                ? undefined
                : async () => {
                    if (!(await confirmAsk(`Delete “${w.workout.name}”?`))) return;
                    await api.deleteWorkout(w.workout.id).catch((e) => toast(errMsg(e), "bad"));
                    reload();
                  }
            }
          />
        ))}
      </div>
    </div>
  );
}

function WorkoutCard({ w, ftp, onRide, onEdit, onExport, onDelete }: { w: WorkoutView; ftp: number; onRide: () => void; onEdit: () => void; onExport: () => void; onDelete?: () => void }) {
  const e = w.estimate;
  return (
    <article className="wcard">
      <header>
        <div>
          <h3>{w.workout.name}</h3>
          <div className="row gap6 wrap">
            <Pill>{w.workout.category}</Pill>
            {!w.workout.builtin && <Pill color="var(--accent)">Mine</Pill>}
            {w.times_ridden > 0 && <Pill color="#B8A6FF">Ridden {w.times_ridden}×</Pill>}
          </div>
        </div>
        <div className="row gap4">
          <button className="icon-btn" title="Export .zwo" onClick={onExport}><IconDownload size={15} /></button>
          <button className="icon-btn" title={w.workout.builtin ? "Duplicate & edit" : "Edit"} onClick={onEdit}>
            {w.workout.builtin ? <IconCopy size={15} /> : <IconEdit size={15} />}
          </button>
          {onDelete && <button className="icon-btn" title="Delete" onClick={onDelete}><IconTrash size={15} /></button>}
        </div>
      </header>
      <p className="muted clamp2">{w.workout.description}</p>
      <WorkoutChart steps={w.steps} ftp={ftp} height={70} />
      <footer>
        <span><b>{durWords(e.duration_s)}</b></span>
        <span>TSS <b>{e.tss.toFixed(0)}</b></span>
        <span>IF <b>{e.intensity_factor.toFixed(2)}</b></span>
        <span><b>{e.work_kj.toFixed(0)}</b> kJ</span>
        <div className="grow" />
        <Btn size="sm" kind="primary" onClick={onRide}><IconPlay size={12} /> Ride</Btn>
      </footer>
    </article>
  );
}

// ---------------------------------------------------------------------------
// Block editing (shared with the in-ride editor)
// ---------------------------------------------------------------------------

export const BLOCK_LABEL: Record<Block["type"], string> = { steady: "Steady", ramp: "Ramp", intervals: "Intervals", free: "Free / effort", set: "Repeat set" };

export function defaultBlock(t: Block["type"]): Block {
  switch (t) {
    case "steady": return { type: "steady", duration_s: 600, power: 0.7 };
    case "ramp": return { type: "ramp", duration_s: 300, from: 0.5, to: 0.8 };
    case "intervals": return { type: "intervals", repeats: 5, on_s: 60, on_power: 1.2, off_s: 60, off_power: 0.5 };
    case "free": return { type: "free", duration_s: 300, grade_pct: 1, effort: false };
    case "set": return { type: "set", repeats: 3, blocks: [{ type: "steady", duration_s: 30, power: 1.5 }, { type: "steady", duration_s: 30, power: 0.5 }] };
  }
}

const pctIn = (v: number, on: (n: number) => void) => <NumberInput value={Math.round(v * 100)} step={5} min={0} max={300} suffix="%" onChange={(n) => on(n / 100)} width={92} />;
const minIn = (s: number, on: (n: number) => void) => <NumberInput value={+(s / 60).toFixed(2)} step={0.5} min={0.1} suffix="min" onChange={(n) => on(Math.max(5, Math.round(n * 60)))} width={100} />;
const secIn = (s: number, on: (n: number) => void) => <NumberInput value={s} step={5} min={0} suffix="s" onChange={(n) => on(Math.max(0, Math.round(n)))} width={90} />;

function BlockEditor({ b, onChange, onRemove, onMove, onDuplicate, depth }: {
  b: Block;
  onChange: (b: Block) => void;
  onRemove: () => void;
  onMove: (d: number) => void;
  onDuplicate: () => void;
  depth: number;
}) {
  return (
    <div className={`block b-${b.type}`}>
      <div className="block-h">
        <select value={b.type} onChange={(e) => onChange(defaultBlock(e.target.value as Block["type"]))}>
          {(Object.keys(BLOCK_LABEL) as Block["type"][]).filter((t) => t !== "set" || depth < 2).map((t) => <option key={t} value={t}>{BLOCK_LABEL[t]}</option>)}
        </select>
        <div className="grow" />
        <button className="icon-btn" onClick={() => onMove(-1)} title="Move up">↑</button>
        <button className="icon-btn" onClick={() => onMove(1)} title="Move down">↓</button>
        <button className="icon-btn" onClick={onDuplicate} title="Duplicate"><IconCopy size={13} /></button>
        <button className="icon-btn" onClick={onRemove} title="Remove"><IconX size={14} /></button>
      </div>
      <div className="block-b">
        {b.type === "steady" && (
          <>
            <Field label="Duration">{minIn(b.duration_s, (n) => onChange({ ...b, duration_s: n }))}</Field>
            <Field label="Power">{pctIn(b.power, (n) => onChange({ ...b, power: n }))}</Field>
            <Field label="Cadence"><NumberInput value={b.cadence ?? 0} step={5} min={0} max={140} suffix="rpm" width={100} onChange={(n) => onChange({ ...b, cadence: n > 0 ? n : null })} /></Field>
          </>
        )}
        {b.type === "ramp" && (
          <>
            <Field label="Duration">{minIn(b.duration_s, (n) => onChange({ ...b, duration_s: n }))}</Field>
            <Field label="From">{pctIn(b.from, (n) => onChange({ ...b, from: n }))}</Field>
            <Field label="To">{pctIn(b.to, (n) => onChange({ ...b, to: n }))}</Field>
          </>
        )}
        {b.type === "intervals" && (
          <>
            <Field label="Repeats"><NumberInput value={b.repeats} step={1} min={1} max={50} width={80} onChange={(n) => onChange({ ...b, repeats: Math.max(1, Math.round(n)) })} /></Field>
            <Field label="On">{secIn(b.on_s, (n) => onChange({ ...b, on_s: Math.max(5, n) }))}</Field>
            <Field label="@">{pctIn(b.on_power, (n) => onChange({ ...b, on_power: n }))}</Field>
            <Field label="Off">{secIn(b.off_s, (n) => onChange({ ...b, off_s: n }))}</Field>
            <Field label="@">{pctIn(b.off_power, (n) => onChange({ ...b, off_power: n }))}</Field>
            <Field label="Cadence"><NumberInput value={b.on_cadence ?? 0} step={5} min={0} max={140} suffix="rpm" width={100} onChange={(n) => onChange({ ...b, on_cadence: n > 0 ? n : null })} /></Field>
          </>
        )}
        {b.type === "free" && (
          <>
            <Field label="Duration">{minIn(b.duration_s, (n) => onChange({ ...b, duration_s: n }))}</Field>
            <Field label="Virtual grade"><NumberInput value={b.grade_pct} step={0.5} min={-5} max={15} suffix="%" width={90} onChange={(n) => onChange({ ...b, grade_pct: n })} /></Field>
            <Field label="All-out effort"><Toggle value={b.effort} onChange={(v) => onChange({ ...b, effort: v })} /></Field>
          </>
        )}
        {b.type === "set" && (
          <div className="set-body">
            <Field label="Repeat" inline>
              <NumberInput value={b.repeats} step={1} min={1} max={50} suffix="×" width={90} onChange={(n) => onChange({ ...b, repeats: Math.max(1, Math.round(n)) })} />
            </Field>
            <BlockList blocks={b.blocks} onChange={(blocks) => onChange({ ...b, blocks })} depth={depth + 1} />
          </div>
        )}
      </div>
    </div>
  );
}

export function BlockList({ blocks, onChange, depth = 0 }: { blocks: Block[]; onChange: (b: Block[]) => void; depth?: number }) {
  const set = (i: number, b: Block) => onChange(blocks.map((y, j) => (j === i ? b : y)));
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= blocks.length) return;
    const c = [...blocks];
    [c[i], c[j]] = [c[j], c[i]];
    onChange(c);
  };
  return (
    <>
      <div className="blocks">
        {blocks.map((b, i) => (
          <BlockEditor
            key={i}
            b={b}
            depth={depth}
            onChange={(nb) => set(i, nb)}
            onMove={(d) => move(i, d)}
            onRemove={() => onChange(blocks.filter((_, j) => j !== i))}
            onDuplicate={() => onChange([...blocks.slice(0, i + 1), JSON.parse(JSON.stringify(b)), ...blocks.slice(i + 1)])}
          />
        ))}
      </div>
      <div className="row gap6 wrap mt8">
        {(Object.keys(BLOCK_LABEL) as Block["type"][]).filter((t) => t !== "set" || depth < 2).map((t) => (
          <Btn key={t} size="sm" onClick={() => onChange([...blocks, defaultBlock(t)])}><IconPlus size={12} /> {BLOCK_LABEL[t]}</Btn>
        ))}
      </div>
    </>
  );
}

export const TEXT_HELP = "10m 50-75%   ·   20m @90% 85rpm   ·   5x 1m @120% / 1m @50%   ·   3x (30s @150%, 30s @50%)   ·   2m free   ·   30s effort @2%   ·   1:30 @250w";

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

export function WorkoutEditor({ initial }: { initial: Workout }) {
  const { back, go, toast, profile } = useApp();
  const [w, setW] = useState<Workout>(initial);
  const [preview, setPreview] = useState<WorkoutView | null>(null);
  const [mode, setMode] = useState<"blocks" | "text">("blocks");
  const [text, setText] = useState("");
  const [textErr, setTextErr] = useState<string | null>(null);
  const ftp = profile?.ftp ?? 200;

  useEffect(() => {
    const t = setTimeout(() => {
      if (w.blocks.length) api.previewWorkout(w).then(setPreview).catch(() => {});
      else setPreview(null);
    }, 150);
    return () => clearTimeout(t);
  }, [w]);

  // Switching to text mode shows the current blocks as text.
  useEffect(() => {
    if (mode === "text") api.workoutToText(w.blocks).then(setText);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  // Live-parse text as you type.
  useEffect(() => {
    if (mode !== "text") return;
    const t = setTimeout(() => {
      if (!text.trim()) return setTextErr(null);
      api
        .parseWorkoutText(text)
        .then((blocks) => {
          setTextErr(null);
          setW((x) => ({ ...x, blocks }));
        })
        .catch((e) => setTextErr(errMsg(e)));
    }, 300);
    return () => clearTimeout(t);
  }, [text, mode]);

  const saveIt = async () => {
    if (!w.name.trim()) return toast("Give your workout a name", "warn");
    try {
      await api.saveWorkout({ ...w, name: w.name.trim() });
      toast("Workout saved", "good");
      go("workouts");
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  return (
    <div className="page">
      <PageHeader
        back={back}
        title={initial.id ? "Edit workout" : "New workout"}
        actions={<><Btn onClick={back}>Cancel</Btn><Btn kind="primary" onClick={saveIt}>Save workout</Btn></>}
      />
      <div className="builder">
        <Card className="builder-form">
          <div className="form">
            <Field label="Name"><input value={w.name} onChange={(e) => setW({ ...w, name: e.target.value })} placeholder="e.g. Threshold 4×8" /></Field>
            <Field label="Category">
              <input list="wcats" value={w.category} onChange={(e) => setW({ ...w, category: e.target.value })} />
              <datalist id="wcats">
                {["Recovery", "Endurance", "Tempo", "Sweet Spot", "Threshold", "VO2 Max", "Anaerobic", "Mixed", "Custom"].map((c) => <option key={c} value={c} />)}
              </datalist>
            </Field>
            <Field label="Description"><textarea rows={2} value={w.description} onChange={(e) => setW({ ...w, description: e.target.value })} /></Field>
          </div>
          <div className="row between center mt16 mb8">
            <h4 style={{ margin: 0 }}>Structure</h4>
            <Segmented size="sm" value={mode} onChange={setMode} options={[{ value: "blocks", label: "Blocks" }, { value: "text", label: "Text" }]} />
          </div>
          {mode === "blocks" ? (
            <BlockList blocks={w.blocks} onChange={(blocks) => setW({ ...w, blocks })} />
          ) : (
            <div className="form">
              <textarea className="code-in" rows={12} value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} />
              {textErr ? <div className="banner bad small">{textErr}</div> : <div className="field-h">One item per line (or comma-separated). Examples: {TEXT_HELP}</div>}
            </div>
          )}
        </Card>
        <div className="builder-preview">
          <Card title="Preview">
            {preview ? (
              <>
                <WorkoutChart steps={preview.steps} ftp={ftp} height={200} axis />
                <div className="grid g5 mt16">
                  <Stat label="Duration" value={dur(preview.estimate.duration_s)} />
                  <Stat label="TSS" value={preview.estimate.tss.toFixed(0)} />
                  <Stat label="IF" value={preview.estimate.intensity_factor.toFixed(2)} />
                  <Stat label="Avg / NP" value={`${Math.round(preview.estimate.avg_power)} / ${Math.round(preview.estimate.np)}`} unit="W" />
                  <Stat label="Work" value={preview.estimate.work_kj.toFixed(0)} unit="kJ" />
                </div>
                <h4 className="mt16">Time in zone</h4>
                <ZoneBar zones={preview.estimate.zone_s} height={16} />
                <div className="legend mt8">
                  {preview.estimate.zone_s.map((z, i) => z > 0 && (
                    <span key={i}><i style={{ background: ZONE_COLORS[i] }} />Z{i + 1} {ZONE_NAMES[i]} · {dur(z)}</span>
                  ))}
                </div>
                <h4 className="mt16">Steps ({preview.steps.length})</h4>
                <div className="steps-list">
                  {preview.steps.map((s, i) => (
                    <div key={i} className="step-row">
                      <span className="muted">{dur(s.start_s)}</span>
                      <span>{s.label}</span>
                      <span className="num">{dur(s.duration_s)}</span>
                      <span className="num">
                        {s.target.type === "erg"
                          ? s.target.from === s.target.to
                            ? `${Math.round(s.target.from * ftp)} W`
                            : `${Math.round(s.target.from * ftp)}→${Math.round(s.target.to * ftp)} W`
                          : s.effort ? "All out" : "Free"}
                      </span>
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <div className="muted">Add a block to see the preview.</div>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}
