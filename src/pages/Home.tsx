import { api } from "../lib/api";
import { useApp, useLoad, useUnits } from "../lib/app";
import { dist, distUnit, dur, durWords, elev, elevUnit, hours, num, relDay, timeOfDay, dateShort, difficultyColor } from "../lib/format";
import { BarChart, ElevationProfile, WorkoutChart } from "../components/charts";
import { Btn, Card, Pill, Ring, Stat } from "../components/ui";
import { IconFlame, IconGhost, IconMountain, IconPlay, IconRoute, IconTrophy } from "../components/icons";
import { DeviceChip } from "../components/DeviceChip";
import { FtpSuggestionBanner } from "../components/FtpSuggestion";
import type { AccountStats, RideSummary, WorkoutView } from "../lib/types";

function greeting() {
  const h = new Date().getHours();
  return h < 5 ? "Late night" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
}

/** Suggest a session from current form (TSB). */
function suggest(stats: AccountStats | null, workouts: WorkoutView[]): { w: WorkoutView; why: string } | null {
  if (!workouts.length) return null;
  const f = stats?.fitness[stats.fitness.length - 1];
  const tsb = f ? f.ctl - f.atl : 0;
  const pick = (id: string) => workouts.find((w) => w.workout.id === id) ?? workouts[0];
  if (!stats || stats.all_time.rides === 0) return { w: pick("builtin-endurance-60"), why: "A steady endurance hour is a great first session." };
  if (tsb < -25) return { w: pick("builtin-recovery-spin"), why: `Form is ${tsb.toFixed(0)} – you're carrying fatigue. Spin it out.` };
  if (tsb < -10) return { w: pick("builtin-endurance-60"), why: `Form is ${tsb.toFixed(0)}. Keep it aerobic today.` };
  if (tsb < 5) return { w: pick("builtin-sweet-spot-2x20"), why: `Form is ${tsb.toFixed(0)} – good day for productive sweet spot.` };
  return { w: pick("builtin-vo2-5x3"), why: `You're fresh (form +${tsb.toFixed(0)}). Time to go hard.` };
}

export default function Home() {
  const { profile, go, prepareRide } = useApp();
  const units = useUnits();
  const stats = useLoad(() => api.accountStats());
  const rides = useLoad(() => api.listRides());
  const routes = useLoad(() => api.listRoutes());
  const workouts = useLoad(() => api.listWorkouts());
  const upcoming = useLoad(() => api.upcomingEvents(7));

  const s = stats.data;
  const week = s?.this_week;
  const goalS = (profile?.weekly_goal_hours ?? 5) * 3600;
  const fit = s?.fitness[s.fitness.length - 1];
  const tsb = fit ? fit.ctl - fit.atl : 0;
  const featured = routes.data?.find((r) => r.favourite) ?? routes.data?.[0];
  const sug = suggest(s, workouts.data ?? []);
  const recent: RideSummary[] = (rides.data ?? []).slice(0, 5);

  return (
    <div className="page home">
      <header className="page-h">
        <div>
          <p className="eyebrow">{new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}</p>
          <h1>
            {greeting()}, {profile?.name ?? "rider"}
          </h1>
        </div>
        <div className="page-a">
          <DeviceChip />
          <Btn kind="primary" size="lg" onClick={() => go("ride")}>
            <IconPlay size={16} /> Start riding
          </Btn>
        </div>
      </header>

      <FtpSuggestionBanner />

      <div className="grid g4">
        <Card className="tile-week">
          <div className="row gap16 center">
            <Ring value={(week?.moving_s ?? 0) / goalS} size={78} stroke={8}>
              <b>{Math.round(((week?.moving_s ?? 0) / goalS) * 100)}%</b>
            </Ring>
            <div>
              <div className="stat-l">This week</div>
              <div className="stat-v">
                {hours(week?.moving_s ?? 0)}
                <span className="stat-u">/ {profile?.weekly_goal_hours ?? 5} h</span>
              </div>
              <div className="stat-s">{week?.rides ?? 0} rides · {num(week?.tss ?? 0)} TSS</div>
            </div>
          </div>
        </Card>
        <Card>
          <Stat label="Distance this week" value={dist(week?.distance_m ?? 0, units)} unit={distUnit(units)} sub={`${elev(week?.elevation_m ?? 0, units)} ${elevUnit(units)} climbed`} />
        </Card>
        <Card>
          <Stat
            label="Form (TSB)"
            value={`${tsb >= 0 ? "+" : ""}${tsb.toFixed(0)}`}
            color={tsb < -20 ? "var(--bad)" : tsb < -5 ? "var(--warn)" : "var(--accent)"}
            sub={`Fitness ${num(fit?.ctl ?? 0)} · Fatigue ${num(fit?.atl ?? 0)}`}
          />
        </Card>
        <Card>
          <Stat label="FTP" value={num(profile?.ftp)} unit="W" sub={`${(s?.wkg ?? 0).toFixed(2)} W/kg · ${s?.week_streak ?? 0}-week streak`} />
        </Card>
      </div>

      <div className="grid g3 mt16">
        <Card title="Suggested session" className="span2" action={<Btn kind="ghost" size="sm" onClick={() => go("workouts")}>All workouts</Btn>}>
          {sug ? (
            <div className="sugg">
              <div className="sugg-t">
                <h2>{sug.w.workout.name}</h2>
                <p className="muted">{sug.why}</p>
                <div className="row gap8 wrap mt8">
                  <Pill>{durWords(sug.w.estimate.duration_s)}</Pill>
                  <Pill>TSS {sug.w.estimate.tss.toFixed(0)}</Pill>
                  <Pill>IF {sug.w.estimate.intensity_factor.toFixed(2)}</Pill>
                  <Pill>{sug.w.estimate.work_kj.toFixed(0)} kJ</Pill>
                </div>
              </div>
              <WorkoutChart steps={sug.w.steps} ftp={profile?.ftp ?? 200} height={90} />
              <div className="row gap8 mt12">
                <Btn kind="primary" onClick={() => prepareRide({ kind: "workout", workout_id: sug.w.workout.id })}>
                  <IconPlay size={14} /> Ride workout
                </Btn>
              </div>
            </div>
          ) : (
            <div className="muted">Loading…</div>
          )}
        </Card>
        <Card title="Upcoming" action={<Btn kind="ghost" size="sm" onClick={() => go("events")}>Events</Btn>}>
          {upcoming.data && upcoming.data.length ? (
            <ul className="agenda">
              {upcoming.data.slice(0, 5).map((o) => (
                <li key={o.event_id + o.at}>
                  <i style={{ background: o.color }} />
                  <div>
                    <b>{o.event_name}</b>
                    <span className="muted">
                      {relDay(o.at)} · {timeOfDay(o.at)}
                    </span>
                  </div>
                  <Btn size="sm" kind="subtle" onClick={() => prepareRide({ kind: "route", event_id: o.event_id })}>Ride</Btn>
                </li>
              ))}
            </ul>
          ) : (
            <div className="empty-inline">
              No events this week.{" "}
              <a onClick={() => go("events")}>Schedule a repeat ride →</a>
            </div>
          )}
        </Card>
      </div>

      <div className="grid g3 mt16">
        <Card
          title={
            <span className="row gap8 center">
              <IconRoute size={16} /> {featured?.favourite ? "Favourite route" : "Featured route"}
            </span>
          }
          className="span2"
          action={<Btn kind="ghost" size="sm" onClick={() => go("routes")}>All routes</Btn>}
        >
          {featured && (
            <div className="feat-route" onClick={() => go("route", { id: featured.id })}>
              <div className="row between center">
                <div>
                  <h2>{featured.name}</h2>
                  <p className="muted">{featured.description}</p>
                </div>
                <div className="row gap8">
                  <Pill color={difficultyColor(featured.stats.difficulty)}>{featured.stats.difficulty_label}</Pill>
                  {featured.best_time_s && (
                    <Pill color="#B8A6FF">
                      <IconGhost size={12} /> PB {dur(featured.best_time_s)}
                    </Pill>
                  )}
                </div>
              </div>
              <ElevationProfile points={featured.thumbnail} height={130} axis={false} interactive={false} climbs={featured.stats.climbs} units={units} />
              <div className="row gap24 mt8">
                <Stat label="Distance" value={dist(featured.stats.distance_m, units)} unit={distUnit(units)} />
                <Stat label="Elevation" value={elev(featured.stats.gain_m, units)} unit={elevUnit(units)} />
                <Stat label="Max grade" value={featured.stats.max_grade_pct.toFixed(1)} unit="%" />
                <Stat label="Est. time" value={dur(featured.est_time_s)} />
                <div className="grow" />
                <span onClick={(e) => e.stopPropagation()}>
                  <Btn kind="primary" onClick={() => prepareRide({ kind: "route", route_id: featured.id })}>
                    <IconPlay size={14} /> Ride
                  </Btn>
                </span>
              </div>
            </div>
          )}
        </Card>
        <Card title="Last 12 weeks">
          {s && (
            <BarChart
              labels={s.weekly.slice(-12).map((w) => dateShort(w.week_start))}
              values={s.weekly.slice(-12).map((w) => w.totals.moving_s / 3600)}
              goal={profile?.weekly_goal_hours}
              format={(v) => `${v.toFixed(1)}h`}
              height={190}
            />
          )}
        </Card>
      </div>

      <Card title="Recent rides" className="mt16" action={<Btn kind="ghost" size="sm" onClick={() => go("activities")}>All activities</Btn>}>
        {recent.length === 0 ? (
          <div className="empty-inline">No rides yet – connect your KICKR (or the simulator) and get going.</div>
        ) : (
          <table className="tbl">
            <tbody>
              {recent.map((r) => (
                <tr key={r.id} onClick={() => go("activity", { id: r.id })}>
                  <td className="muted w120">{relDay(r.started_at)}</td>
                  <td>
                    <b>{r.name}</b> {r.personal_best && <Pill color="#FFD23F"><IconTrophy size={11} /> PB</Pill>}
                    {r.test_result && <Pill color="var(--accent)">FTP {r.test_result.ftp} W</Pill>}
                  </td>
                  <td className="num">{dur(r.moving_s)}</td>
                  <td className="num">{dist(r.distance_m, units)} {distUnit(units)}</td>
                  <td className="num"><IconMountain size={12} /> {elev(r.elevation_gain_m, units)} {elevUnit(units)}</td>
                  <td className="num">{Math.round(r.avg_power)} W</td>
                  <td className="num"><IconFlame size={12} /> {Math.round(r.calories_kcal)} kcal</td>
                  <td className="num muted">TSS {Math.round(r.tss)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
