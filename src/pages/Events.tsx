import { useState } from "react";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad, useUnits } from "../lib/app";
import { dateShort, dist, distUnit, dur, relDay, timeOfDay, todayISO, WEEKDAYS } from "../lib/format";
import type { EventTarget, EventView, Freq, RideEvent, RideSummary, TestKind } from "../lib/types";
import { Btn, Card, Empty, Field, Modal, NumberInput, PageHeader, Pill, Segmented, Toggle, confirmAsk } from "../components/ui";
import { IconCalendar, IconEdit, IconFlame, IconPlay, IconPlus, IconRepeat, IconTrash, IconTrophy } from "../components/icons";

const COLORS = ["#7CFFB2", "#4DA3FF", "#FFD23F", "#FF9F43", "#FF5A7A", "#B36BFF", "#64D8CB"];

function scheduleText(ev: RideEvent): string {
  const r = ev.recurrence;
  const at = `at ${r.time}`;
  switch (r.freq) {
    case "once": return `Once on ${dateShort(r.start_date)} ${at}`;
    case "daily": return `Every day ${at}`;
    case "every_n_days": return `Every ${r.interval} days ${at}`;
    case "weekly": {
      const days = (r.weekdays.length ? r.weekdays : [(new Date(r.start_date).getDay() + 6) % 7]).map((d) => WEEKDAYS[d]).join(", ");
      return `${r.interval > 1 ? `Every ${r.interval} weeks` : "Weekly"} on ${days} ${at}`;
    }
  }
}

export default function Events() {
  const { prepareRide, toast } = useApp();
  const units = useUnits();
  const events = useLoad(() => api.listEvents());
  const upcoming = useLoad(() => api.upcomingEvents(21));
  const [editing, setEditing] = useState<RideEvent | null>(null);
  const [history, setHistory] = useState<EventView | null>(null);

  const blank = (): RideEvent => ({
    id: "",
    name: "",
    description: "",
    target: { type: "route", route_id: "", laps: 1 },
    recurrence: { freq: "weekly", start_date: todayISO(), time: "18:30", end_date: null, weekdays: [], interval: 1 },
    created_at: "",
    color: COLORS[(events.data?.length ?? 0) % COLORS.length],
    active: true,
  });

  const reload = () => {
    events.reload();
    upcoming.reload();
  };

  return (
    <div className="page">
      <PageHeader
        title="Events"
        sub="Repeat rides on a schedule – the Tuesday hill reps, the Sunday long one. Track every attempt and build a streak."
        actions={<Btn kind="primary" onClick={() => setEditing(blank())}><IconPlus size={14} /> New event</Btn>}
      />
      <div className="grid g3-1">
        <div className="stack">
          {events.data?.length === 0 && (
            <Empty icon={<IconRepeat size={30} />} title="No events yet">
              Create a recurring ride: pick a route, workout or test, choose the days and time, and it'll show up on your home screen.
            </Empty>
          )}
          {events.data?.map((v) => (
            <Card key={v.event.id} className={`event ${v.event.active ? "" : "inactive"}`}>
              <div className="ev-row">
                <i className="ev-color" style={{ background: v.event.color }} />
                <div className="grow">
                  <div className="row gap8 center wrap">
                    <h3>{v.event.name}</h3>
                    {!v.event.active && <Pill>Paused</Pill>}
                    {v.streak > 1 && <Pill color="#FF9F43"><IconFlame size={11} /> {v.streak} in a row</Pill>}
                  </div>
                  <div className="muted">{v.target_name} · {scheduleText(v.event)}</div>
                  {v.event.description && <p className="small mt4">{v.event.description}</p>}
                </div>
                <div className="ev-stats">
                  <div><span className="muted small">Next</span><b>{v.next ? `${relDay(v.next)} ${timeOfDay(v.next)}` : "–"}</b></div>
                  <div><span className="muted small">Attempts</span><b>{v.attempts}</b></div>
                  <div><span className="muted small">Best</span><b>{v.best_time_s ? dur(v.best_time_s) : v.best_np ? `${Math.round(v.best_np)} W NP` : "–"}</b></div>
                </div>
                <div className="row gap6">
                  <Btn size="sm" kind="ghost" onClick={() => setHistory(v)} title="History"><IconTrophy size={14} /></Btn>
                  <Btn size="sm" kind="ghost" onClick={() => setEditing(v.event)} title="Edit"><IconEdit size={14} /></Btn>
                  <Btn
                    size="sm"
                    kind="ghost"
                    title="Delete"
                    onClick={async () => {
                      if (!(await confirmAsk(`Delete event “${v.event.name}”? Rides are kept.`))) return;
                      await api.deleteEvent(v.event.id);
                      reload();
                    }}
                  >
                    <IconTrash size={14} />
                  </Btn>
                  <Btn size="sm" kind="primary" onClick={() => prepareRide({ kind: "route", event_id: v.event.id })}><IconPlay size={12} /> Ride</Btn>
                </div>
              </div>
            </Card>
          ))}
        </div>
        <Card title={<span className="row gap6 center"><IconCalendar size={15} /> Next 3 weeks</span>}>
          {upcoming.data?.length ? (
            <ul className="agenda">
              {upcoming.data.map((o) => (
                <li key={o.event_id + o.at}>
                  <i style={{ background: o.color }} />
                  <div>
                    <b>{o.event_name}</b>
                    <span className="muted">{relDay(o.at)} · {timeOfDay(o.at)}</span>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <div className="empty-inline">Nothing scheduled.</div>
          )}
        </Card>
      </div>

      {editing && (
        <EventEditor
          initial={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            reload();
            toast("Event saved", "good");
          }}
        />
      )}
      {history && <EventHistory view={history} units={units} onClose={() => setHistory(null)} />}
    </div>
  );
}

function EventEditor({ initial, onClose, onSaved }: { initial: RideEvent; onClose: () => void; onSaved: () => void }) {
  const { toast } = useApp();
  const [ev, setEv] = useState<RideEvent>(initial);
  const routes = useLoad(() => api.listRoutes());
  const workouts = useLoad(() => api.listWorkouts());
  const r = ev.recurrence;
  const setR = (patch: Partial<typeof r>) => setEv({ ...ev, recurrence: { ...r, ...patch } });
  const t = ev.target;

  const setKind = (k: EventTarget["type"]) => {
    if (k === t.type) return;
    if (k === "route") setEv({ ...ev, target: { type: "route", route_id: routes.data?.[0]?.id ?? "", laps: 1 } });
    if (k === "workout") setEv({ ...ev, target: { type: "workout", workout_id: workouts.data?.[0]?.workout.id ?? "", route_id: null } });
    if (k === "test") setEv({ ...ev, target: { type: "test", test: "ramp" } });
  };

  const routeId = t.type === "route" ? t.route_id || routes.data?.[0]?.id || "" : "";
  const workoutId = t.type === "workout" ? t.workout_id || workouts.data?.[0]?.workout.id || "" : "";

  const save = async () => {
    const target: EventTarget =
      t.type === "route" ? { ...t, route_id: routeId } : t.type === "workout" ? { ...t, workout_id: workoutId } : t;
    if (!ev.name.trim()) return toast("Name your event", "warn");
    if ((target.type === "route" && !target.route_id) || (target.type === "workout" && !target.workout_id)) return toast("Choose what to ride", "warn");
    try {
      await api.saveEvent({ ...ev, name: ev.name.trim(), target });
      onSaved();
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  return (
    <Modal
      title={initial.id ? "Edit event" : "New event"}
      onClose={onClose}
      wide
      footer={<><Btn onClick={onClose}>Cancel</Btn><Btn kind="primary" onClick={save}>Save event</Btn></>}
    >
      <div className="form two">
        <Field label="Name"><input value={ev.name} onChange={(e) => setEv({ ...ev, name: e.target.value })} placeholder="Tuesday hill reps" autoFocus /></Field>
        <Field label="Colour">
          <div className="row gap6">
            {COLORS.map((c) => (
              <button key={c} className={`swatch ${c === ev.color ? "on" : ""}`} style={{ background: c }} onClick={() => setEv({ ...ev, color: c })} aria-label={c} />
            ))}
          </div>
        </Field>
        <Field label="Ride">
          <Segmented<EventTarget["type"]> size="sm" value={t.type} onChange={setKind} options={[{ value: "route", label: "Route" }, { value: "workout", label: "Workout" }, { value: "test", label: "Test" }]} />
        </Field>
        {t.type === "route" && (
          <div className="row gap12">
            <Field label="Route">
              <select value={routeId} onChange={(e) => setEv({ ...ev, target: { ...t, route_id: e.target.value } })}>
                {routes.data?.map((x) => <option key={x.id} value={x.id}>{x.name}</option>)}
              </select>
            </Field>
            <Field label="Laps"><NumberInput value={t.laps} min={1} max={20} width={80} onChange={(n) => setEv({ ...ev, target: { ...t, laps: Math.max(1, Math.round(n)) } })} /></Field>
          </div>
        )}
        {t.type === "workout" && (
          <div className="row gap12">
            <Field label="Workout">
              <select value={workoutId} onChange={(e) => setEv({ ...ev, target: { ...t, workout_id: e.target.value } })}>
                {workouts.data?.map((x) => <option key={x.workout.id} value={x.workout.id}>{x.workout.name}</option>)}
              </select>
            </Field>
            <Field label="Scenery">
              <select value={t.route_id ?? ""} onChange={(e) => setEv({ ...ev, target: { ...t, route_id: e.target.value || null } })}>
                <option value="">Flat road</option>
                {routes.data?.map((x) => <option key={x.id} value={x.id}>{x.name}</option>)}
              </select>
            </Field>
          </div>
        )}
        {t.type === "test" && (
          <Field label="Test">
            <select value={t.test} onChange={(e) => setEv({ ...ev, target: { type: "test", test: e.target.value as TestKind } })}>
              <option value="ramp">Ramp test</option>
              <option value="ftp20">20-minute FTP test</option>
              <option value="ftp8">2 × 8-minute test</option>
            </select>
          </Field>
        )}
        <Field label="Repeats">
          <Segmented<Freq> size="sm" value={r.freq} onChange={(f) => setR({ freq: f })} options={[{ value: "once", label: "Once" }, { value: "daily", label: "Daily" }, { value: "weekly", label: "Weekly" }, { value: "every_n_days", label: "Every N days" }]} />
        </Field>
        {r.freq === "weekly" && (
          <Field label="On">
            <div className="row gap4">
              {WEEKDAYS.map((d, i) => (
                <button key={d} className={`chip ${r.weekdays.includes(i) ? "on" : ""}`} onClick={() => setR({ weekdays: r.weekdays.includes(i) ? r.weekdays.filter((x) => x !== i) : [...r.weekdays, i].sort() })}>
                  {d}
                </button>
              ))}
            </div>
          </Field>
        )}
        {(r.freq === "weekly" || r.freq === "every_n_days") && (
          <Field label={r.freq === "weekly" ? "Every N weeks" : "Every N days"}>
            <NumberInput value={r.interval} min={1} max={30} width={90} onChange={(n) => setR({ interval: Math.max(1, Math.round(n)) })} />
          </Field>
        )}
        <div className="row gap12">
          <Field label={r.freq === "once" ? "Date" : "Starts"}><input type="date" value={r.start_date} onChange={(e) => setR({ start_date: e.target.value })} /></Field>
          <Field label="Time"><input type="time" value={r.time} onChange={(e) => setR({ time: e.target.value })} /></Field>
          {r.freq !== "once" && <Field label="Ends (optional)"><input type="date" value={r.end_date ?? ""} onChange={(e) => setR({ end_date: e.target.value || null })} /></Field>}
        </div>
        <Field label="Notes"><textarea rows={2} value={ev.description} onChange={(e) => setEv({ ...ev, description: e.target.value })} placeholder="Goal, pacing plan…" /></Field>
        <Field label="Active" inline><Toggle value={ev.active} onChange={(v) => setEv({ ...ev, active: v })} /></Field>
      </div>
    </Modal>
  );
}

function EventHistory({ view, units, onClose }: { view: EventView; units: "metric" | "imperial"; onClose: () => void }) {
  const { go } = useApp();
  const rides = useLoad(() => api.eventRides(view.event.id), [view.event.id]);
  const sorted: RideSummary[] = [...(rides.data ?? [])];
  const timed = view.event.target.type === "route";
  if (timed) sorted.sort((a, b) => (a.route_time_s ?? 1e12) - (b.route_time_s ?? 1e12));
  return (
    <Modal title={`${view.event.name} – history`} onClose={onClose} wide>
      {sorted.length === 0 ? (
        <div className="empty-inline">No attempts yet.</div>
      ) : (
        <table className="tbl">
          <thead>
            <tr><th>#</th><th>Date</th><th className="num">{timed ? "Time" : "Duration"}</th><th className="num">Distance</th><th className="num">Avg W</th><th className="num">NP</th><th className="num">TSS</th></tr>
          </thead>
          <tbody>
            {sorted.map((r, i) => (
              <tr key={r.id} onClick={() => { onClose(); go("activity", { id: r.id }); }}>
                <td>{i + 1}</td>
                <td>{dateShort(r.started_at)} {r.personal_best && <Pill color="#FFD23F">PB</Pill>}</td>
                <td className="num">{timed ? (r.route_time_s ? dur(r.route_time_s) : "DNF") : dur(r.moving_s)}</td>
                <td className="num">{dist(r.distance_m, units)} {distUnit(units)}</td>
                <td className="num">{Math.round(r.avg_power)}</td>
                <td className="num">{Math.round(r.np)}</td>
                <td className="num">{Math.round(r.tss)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Modal>
  );
}
