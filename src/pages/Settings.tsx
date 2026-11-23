import { useEffect, useState } from "react";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad } from "../lib/app";
import type { HrZoneModel, Profile, StravaStatus } from "../lib/types";
import { Btn, Card, Field, NumberInput, PageHeader, Segmented, Slider, Toggle } from "../components/ui";
import { hrZonesFor, physicsSpeed } from "../lib/physics";
import { hrZoneColor } from "../lib/format";
import { playCue, speak, unlockAudio } from "../lib/audio";

const POSITIONS = [
  { label: "Upright", cda: 0.4 },
  { label: "Hoods", cda: 0.32 },
  { label: "Drops", cda: 0.28 },
  { label: "Aero / TT", cda: 0.24 },
];
const WHEELS = [
  { label: "700×23", c: 2.096 },
  { label: "700×25", c: 2.105 },
  { label: "700×28", c: 2.136 },
  { label: "700×32", c: 2.155 },
  { label: "29er", c: 2.326 },
];
const SURFACES = [
  { label: "Smooth tarmac", crr: 0.004 },
  { label: "Average road", crr: 0.005 },
  { label: "Rough road", crr: 0.007 },
  { label: "Gravel", crr: 0.009 },
];

export default function Settings() {
  const { profile, setProfile, toast } = useApp();
  const [p, setP] = useState<Profile | null>(profile);
  useEffect(() => setP(profile), [profile]);
  if (!p) return null;
  const set = <K extends keyof Profile>(k: K, v: Profile[K]) => setP({ ...p, [k]: v });
  const setAlert = (patch: Partial<Profile["hr_alerts"]>) => setP({ ...p, hr_alerts: { ...p.hr_alerts, ...patch } });
  const zones = hrZonesFor(p);
  const dirty = JSON.stringify(p) !== JSON.stringify(profile);
  const imp = p.units === "imperial";

  const save = async () => {
    try {
      const saved = await api.saveProfile(p);
      setProfile(saved);
      toast("Profile saved", "good");
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const v250 = physicsSpeed(250, 0, p.weight_kg + p.bike_weight_kg, p.cda, p.crr) * 3.6;
  const v250c = physicsSpeed(250, 0.06, p.weight_kg + p.bike_weight_kg, p.cda, p.crr) * 3.6;

  return (
    <div className="page">
      <PageHeader
        title="Profile & settings"
        sub="Your body and bike drive the physics, calories and every power-based number."
        actions={<Btn kind="primary" onClick={save} disabled={!dirty}>Save changes</Btn>}
      />
      <div className="grid g2">
        <Card title="Rider">
          <div className="form two">
            <Field label="Name"><input value={p.name} onChange={(e) => set("name", e.target.value)} /></Field>
            <Field label="Units">
              <Segmented size="sm" value={p.units} onChange={(v) => set("units", v)} options={[{ value: "metric", label: "Metric" }, { value: "imperial", label: "Imperial" }]} />
            </Field>
            <Field label="Weight">
              {imp ? (
                <NumberInput value={+(p.weight_kg * 2.20462).toFixed(1)} step={0.5} suffix="lb" onChange={(v) => set("weight_kg", v / 2.20462)} />
              ) : (
                <NumberInput value={+p.weight_kg.toFixed(1)} step={0.5} suffix="kg" onChange={(v) => set("weight_kg", v)} />
              )}
            </Field>
            <Field label="Bike weight">
              {imp ? (
                <NumberInput value={+(p.bike_weight_kg * 2.20462).toFixed(1)} step={0.5} suffix="lb" onChange={(v) => set("bike_weight_kg", v / 2.20462)} />
              ) : (
                <NumberInput value={+p.bike_weight_kg.toFixed(1)} step={0.5} suffix="kg" onChange={(v) => set("bike_weight_kg", v)} />
              )}
            </Field>
            <Field label="Height"><NumberInput value={p.height_cm} suffix="cm" onChange={(v) => set("height_cm", v)} /></Field>
            <Field label="Age"><NumberInput value={p.age} onChange={(v) => set("age", Math.round(v))} /></Field>
            <Field label="Sex" hint="Used only for heart-rate calorie estimates.">
              <Segmented size="sm" value={p.sex} onChange={(v) => set("sex", v)} options={[{ value: "male", label: "Male" }, { value: "female", label: "Female" }]} />
            </Field>
            <Field label="FTP"><NumberInput value={p.ftp} suffix="W" onChange={(v) => set("ftp", v)} /></Field>
            <Field label="Auto FTP" hint="Re-assessed from your best 3–20 min efforts (last 90 days) after every ride and daily. Drops are only ever suggested.">
              <Segmented size="sm" value={p.ftp_auto} onChange={(v) => set("ftp_auto", v)} options={[{ value: "off", label: "Off" }, { value: "suggest", label: "Suggest" }, { value: "auto_increase", label: "Auto-raise" }]} />
            </Field>
            <Field label="Weekly goal"><NumberInput value={p.weekly_goal_hours} step={0.5} suffix="h" onChange={(v) => set("weekly_goal_hours", v)} /></Field>
          </div>
        </Card>

        <Card title="Ride physics">
          <div className="form">
            <Field label="Riding position" hint={`CdA ${p.cda.toFixed(3)} m² – lower is faster on the flat.`}>
              <Segmented<number> size="sm" value={POSITIONS.find((x) => Math.abs(x.cda - p.cda) < 0.005)?.cda ?? -1} onChange={(v) => set("cda", v)} options={POSITIONS.map((x) => ({ value: x.cda, label: x.label }))} />
            </Field>
            <Field label="Road surface" hint={`Crr ${p.crr.toFixed(4)}`}>
              <Segmented<number> size="sm" value={SURFACES.find((x) => Math.abs(x.crr - p.crr) < 0.0005)?.crr ?? -1} onChange={(v) => set("crr", v)} options={SURFACES.map((x) => ({ value: x.crr, label: x.label }))} />
            </Field>
            <div className="banner">
              At 250 W you'd do <b>{(imp ? v250 / 1.609 : v250).toFixed(1)} {imp ? "mph" : "km/h"}</b> on the flat and{" "}
              <b>{(imp ? v250c / 1.609 : v250c).toFixed(1)} {imp ? "mph" : "km/h"}</b> up a 6% climb.
            </div>
          </div>
        </Card>

        <Card title="Drivetrain">
          <div className="form">
            <Field label="Virtual shifting" inline hint="Turn on if your KICKR has a Zwift Cog or any single cog. CoreKick works out your speed from cadence × virtual gear and sets the trainer's resistance to match the gradient. Shift with the on-screen buttons, [ and ] keys, or a Bluetooth page-turner remote (Page Up / Page Down).">
              <Toggle value={p.virtual_shifting} onChange={(v) => set("virtual_shifting", v)} label={p.virtual_shifting ? "On – Zwift Cog / single speed" : "Off – real cassette"} />
            </Field>
            {p.virtual_shifting && (
              <>
                <Field label="Starting gear" hint="1 = easiest (like 34×34), 24 = hardest (like 52×11).">
                  <Slider value={p.start_gear} min={1} max={24} step={1} onChange={(v) => set("start_gear", v)} format={(v) => `Gear ${v}`} />
                </Field>
                <Field label="Wheel size" hint={`Circumference ${p.wheel_circumference_m.toFixed(3)} m – sets how fast each gear feels.`}>
                  <Segmented<number>
                    size="sm"
                    value={WHEELS.find((w) => Math.abs(w.c - p.wheel_circumference_m) < 0.002)?.c ?? -1}
                    onChange={(v) => set("wheel_circumference_m", v)}
                    options={WHEELS.map((w) => ({ value: w.c, label: w.label }))}
                  />
                </Field>
                <div className="banner">
                  ERG workouts don't use gears – the trainer sets the power. Gears take over on routes, free rides, test efforts and when you switch a workout to resistance mode.
                </div>
              </>
            )}
          </div>
        </Card>

        <Card title="Heart-rate zones">
          <div className="form">
            <Field label="Zone model">
              <Segmented<HrZoneModel>
                size="sm"
                value={p.hr_zone_model}
                onChange={(v) => set("hr_zone_model", v)}
                options={[{ value: "max_hr", label: "% of max HR" }, { value: "reserve", label: "HR reserve" }, { value: "lthr", label: "Threshold HR (7 zones)" }]}
              />
            </Field>
            <div className="row gap12 wrap">
              <Field label="Max HR"><NumberInput value={p.max_hr} suffix="bpm" onChange={(v) => set("max_hr", Math.round(v))} /></Field>
              <Field label="Resting HR"><NumberInput value={p.rest_hr} suffix="bpm" onChange={(v) => set("rest_hr", Math.round(v))} /></Field>
              {p.hr_zone_model === "lthr" && (
                <Field label="Threshold HR" hint="Average HR of the last 20 min of a 30-min all-out effort.">
                  <NumberInput value={p.lthr || Math.round(p.max_hr * 0.9)} suffix="bpm" onChange={(v) => set("lthr", Math.round(v))} />
                </Field>
              )}
            </div>
            <table className="tbl compact">
              <tbody>
                {zones.map((z) => (
                  <tr key={z.zone}>
                    <td><i className="dot" style={{ background: hrZoneColor(z.zone, zones.length) }} /> Z{z.zone}</td>
                    <td>{z.name}</td>
                    <td className="num">{z.hi == null ? `≥ ${z.lo}` : z.zone === 1 && z.lo === 0 ? `< ${z.hi}` : `${z.lo}–${z.hi}`} bpm</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>

        <Card title="Heart-rate alerts (defaults)">
          <div className="form">
            <Toggle value={p.hr_alerts.enabled} onChange={(v) => setAlert({ enabled: v })} label={p.hr_alerts.enabled ? "On at the start of every ride" : "Off by default – switch on during a ride with H"} />
            <Field label="Keep heart rate between">
              <div className="row gap6 center">
                <select value={p.hr_alerts.low_zone} onChange={(e) => setAlert({ low_zone: Math.min(+e.target.value, p.hr_alerts.high_zone) })}>
                  {zones.map((z) => <option key={z.zone} value={z.zone}>Z{z.zone} {z.name}</option>)}
                </select>
                <span className="muted">and</span>
                <select value={p.hr_alerts.high_zone} onChange={(e) => setAlert({ high_zone: Math.max(+e.target.value, p.hr_alerts.low_zone) })}>
                  {zones.map((z) => <option key={z.zone} value={z.zone}>Z{z.zone} {z.name}</option>)}
                </select>
              </div>
            </Field>
            <div className="row gap16 wrap">
              <Toggle value={p.hr_alerts.alert_high} onChange={(v) => setAlert({ alert_high: v })} label="Alert when too high" />
              <Toggle value={p.hr_alerts.alert_low} onChange={(v) => setAlert({ alert_low: v })} label="Alert when too low" />
              <Toggle value={p.hr_alerts.voice} onChange={(v) => setAlert({ voice: v })} label="Spoken prompts" />
            </div>
            <div className="row gap12 wrap">
              <Field label="Warm-up grace"><NumberInput value={Math.round(p.hr_alerts.grace_s / 60)} suffix="min" onChange={(v) => setAlert({ grace_s: Math.max(0, v) * 60 })} width={110} /></Field>
              <Field label="Repeat every"><NumberInput value={p.hr_alerts.repeat_s} step={5} suffix="s" onChange={(v) => setAlert({ repeat_s: Math.max(10, v) })} width={100} /></Field>
            </div>
            <Field label="Volume">
              <Slider value={p.hr_alerts.volume} min={0} max={1} step={0.05} onChange={(v) => setAlert({ volume: v })} format={(v) => `${Math.round(v * 100)}%`} />
            </Field>
            <div className="row gap8">
              <Btn size="sm" onClick={() => { unlockAudio(); playCue("high", p.hr_alerts.volume); if (p.hr_alerts.voice) speak("Heart rate high. Ease off.", p.hr_alerts.volume); }}>Test “too high”</Btn>
              <Btn size="sm" onClick={() => { unlockAudio(); playCue("low", p.hr_alerts.volume); if (p.hr_alerts.voice) speak("Heart rate low. Push on.", p.hr_alerts.volume); }}>Test “too low”</Btn>
            </div>
          </div>
        </Card>

        <StravaCard />

        <Card title="Trainer feel">
          <div className="form">
            <Field label="Trainer difficulty" hint="How much of each real gradient the KICKR simulates. Virtual speed always uses the true gradient.">
              <Slider value={p.trainer_difficulty} min={0} max={1.5} step={0.05} onChange={(v) => set("trainer_difficulty", v)} format={(v) => `${Math.round(v * 100)}%`} />
            </Field>
            <Field label="Max uphill gradient" hint="KICKR CORE simulates up to 16%.">
              <Slider value={p.max_trainer_grade} min={4} max={20} step={0.5} onChange={(v) => set("max_trainer_grade", v)} format={(v) => `${v}%`} />
            </Field>
            <Field label="Max downhill gradient">
              <Slider value={p.min_trainer_grade} min={-20} max={0} step={0.5} onChange={(v) => set("min_trainer_grade", v)} format={(v) => `${v}%`} />
            </Field>
          </div>
        </Card>

        <Card title="Energy & calories">
          <div className="form">
            <Field label="Gross efficiency" hint="Share of metabolic energy that reaches the pedals. Trained cyclists are typically 20–25%.">
              <Slider value={p.gross_efficiency} min={0.18} max={0.28} step={0.005} onChange={(v) => set("gross_efficiency", v)} format={(v) => `${(v * 100).toFixed(1)}%`} />
            </Field>
            <div className="banner">
              Calories are computed from measured work: <code>kcal = kJ ÷ (4.184 × {p.gross_efficiency.toFixed(3)})</code> ={" "}
              <b>{(1 / (4.184 * p.gross_efficiency)).toFixed(2)} kcal per kJ</b>. Power-based energy is far more accurate than heart-rate formulas,
              which are shown alongside when a strap is connected.
            </div>
          </div>
        </Card>
      </div>
    </div>
  );
}

function StravaCard() {
  const { toast } = useApp();
  const { data: st, setData } = useLoad(() => api.stravaStatus());
  const [id, setId] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState(false);
  if (!st) return <Card title="Strava"><div className="muted">Loading…</div></Card>;

  const run = async (f: () => Promise<StravaStatus>, ok?: string) => {
    setBusy(true);
    try {
      setData(await f());
      if (ok) toast(ok, "good");
    } catch (e) {
      toast(errMsg(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title={<span className="row gap8 center"><span className="strava-mark">S</span> Strava</span>}>
      {st.connected && !editing ? (
        <div className="form">
          <div className="banner">Connected as <b>{st.athlete_name}</b>. Rides upload as <b>Virtual Rides</b> (FIT, with power, heart rate and cadence).</div>
          <Toggle value={st.auto_upload} onChange={(v) => run(() => api.stravaSetAuto(v), v ? "Rides will upload automatically" : undefined)} label="Upload every saved ride automatically" />
          <div className="row gap8">
            <Btn size="sm" kind="ghost" onClick={() => run(() => api.stravaDisconnect(), "Disconnected from Strava")} disabled={busy}>Disconnect</Btn>
          </div>
        </div>
      ) : st.configured && !editing ? (
        <div className="form">
          <p className="muted small">Your Strava API app (Client ID {st.client_id}) is set up. Connect to authorise uploads – your browser will open Strava's permission page.</p>
          <div className="row gap8">
            <Btn kind="primary" className="strava-btn" onClick={() => run(() => api.stravaConnect(), "Connected to Strava")} disabled={busy}>{busy ? "Waiting for Strava…" : "Connect with Strava"}</Btn>
            <Btn size="sm" kind="ghost" onClick={() => setEditing(true)}>Change app</Btn>
          </div>
          {busy && <p className="muted small">Approve CoreKick in the browser (keep “Upload your activities” ticked), then come back here.</p>}
        </div>
      ) : (
        <div className="form">
          <ol className="steps-ol">
            <li>
              Open <a onClick={() => api.openUrl("https://www.strava.com/settings/api")}>strava.com/settings/api</a> and create an application (any name, e.g. “CoreKick”; website <code>http://localhost</code>).
            </li>
            <li>Set <b>Authorization Callback Domain</b> to <code>localhost</code>.</li>
            <li>Copy the <b>Client ID</b> and <b>Client Secret</b> here.</li>
          </ol>
          <div className="row gap12 wrap">
            <Field label="Client ID"><input value={id} onChange={(e) => setId(e.target.value)} placeholder="123456" /></Field>
            <Field label="Client Secret"><input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} placeholder="40 characters" /></Field>
          </div>
          <div className="row gap8">
            <Btn kind="primary" disabled={!id || !secret || busy} onClick={() => run(async () => { const r = await api.stravaSaveClient(id, secret); setEditing(false); return r; }, "Strava app saved")}>Save</Btn>
            {editing && <Btn kind="ghost" onClick={() => setEditing(false)}>Cancel</Btn>}
          </div>
          <p className="muted small">
            Strava issues API access per app, so you use your own: a new app covers one athlete – you. Since June 2026 Strava requires the developer of a standard-tier
            app to have a Strava subscription. Credentials are stored only on this Mac.
          </p>
        </div>
      )}
    </Card>
  );
}
