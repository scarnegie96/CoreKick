import { useState } from "react";
import { api, errMsg } from "../lib/api";
import { useApp } from "../lib/app";
import { dateShort } from "../lib/format";
import { Btn, Card, Pill } from "./ui";

const DUR: Record<number, string> = { 180: "3 min", 300: "5 min", 480: "8 min", 600: "10 min", 1200: "20 min" };

/** Banner shown when the latest eFTP assessment suggests a change. */
export function FtpSuggestionBanner({ rideId }: { rideId?: string }) {
  const { ftpAssessment: a, setFtpAssessment, setProfile, toast, profile } = useApp();
  const [busy, setBusy] = useState(false);
  if (!a || !profile || profile.ftp_auto === "off") return null;
  if (rideId && a.trigger_ride_id !== rideId) return null;
  const actionable = (a.suggestion === "increase" || a.suggestion === "decrease") && !a.dismissed && !a.auto_applied;
  const applied = a.auto_applied && Math.round(profile.ftp) === Math.round(a.suggested_ftp ?? -1);
  if (!actionable && !applied) return null;

  const apply = async () => {
    setBusy(true);
    try {
      const p = await api.ftpApplySuggestion();
      setProfile(p);
      setFtpAssessment(await api.ftpAssessment());
      toast(`FTP set to ${p.ftp} W`, "good");
    } catch (e) {
      toast(errMsg(e), "bad");
    }
    setBusy(false);
  };
  const dismiss = async () => {
    await api.ftpDismiss().catch(() => {});
    setFtpAssessment({ ...a, dismissed: true });
  };

  const up = a.change_w > 0;
  return (
    <Card className={`ftp-sugg mb16 ${up ? "" : "warn"}`}>
      <div className="row between center wrap gap12">
        <div>
          <div className="eyebrow">{applied ? "FTP updated automatically" : up ? "FTP increase detected" : "FTP may have dropped"}</div>
          <h2>
            {applied ? <>FTP is now <span className="accent">{a.suggested_ftp} W</span></> : <>Estimated FTP <span className={up ? "accent" : "warn-t"}>{a.suggested_ftp} W</span></>}
            <span className="muted small"> &nbsp;{up ? "+" : ""}{Math.round(a.change_w)} W ({up ? "+" : ""}{a.change_pct.toFixed(1)} %) vs {Math.round(a.current_ftp)} W</span>
          </h2>
          <p className="muted small">{a.reason}</p>
        </div>
        {actionable && (
          <div className="row gap8">
            <Btn kind="ghost" onClick={dismiss}>Not now</Btn>
            <Btn kind="primary" disabled={busy} onClick={apply}>Set FTP to {a.suggested_ftp} W</Btn>
          </div>
        )}
      </div>
    </Card>
  );
}

/** Detailed eFTP card: estimate, method, and the efforts it was built from. */
export function EftpCard() {
  const { ftpAssessment: a, setFtpAssessment, go, toast } = useApp();
  const [busy, setBusy] = useState(false);
  const e = a?.estimate;
  const refresh = async () => {
    setBusy(true);
    try {
      setFtpAssessment(await api.ftpReassess());
    } catch (err) {
      toast(errMsg(err), "bad");
    }
    setBusy(false);
  };
  return (
    <Card title="Estimated FTP from your riding" action={<Btn kind="ghost" size="sm" disabled={busy} onClick={refresh}>Re-assess</Btn>}>
      {!e ? (
        <p className="muted small">{a?.reason ?? "Ride at least 20 minutes with some hard sustained efforts and CoreKick will estimate your FTP automatically."}</p>
      ) : (
        <>
          <div className="row gap12 center">
            <div className="stat-v">{e.estimate}<span className="stat-u">W</span></div>
            <Pill color={e.confidence === "high" ? "var(--accent)" : e.confidence === "medium" ? "var(--warn)" : "var(--muted)"}>{e.confidence} confidence</Pill>
          </div>
          <p className="small muted mt8">
            {e.method === "critical_power"
              ? <>Critical power {e.cp?.toFixed(0)} W · W′ {e.w_prime_kj?.toFixed(1)} kJ · fit r² {e.r2?.toFixed(3)}</>
              : <>95 % of best 20-min power (not enough 3–10 min efforts for a CP fit)</>}
            {" · "}{e.rides_considered} ride{e.rides_considered === 1 ? "" : "s"} in the last 90 days
          </p>
          <table className="tbl compact mt8">
            <tbody>
              {e.points.map((p) => (
                <tr key={p.duration_s} className="click" onClick={() => go("activity", { id: p.ride_id })}>
                  <td className="nowrap">{DUR[p.duration_s] ?? `${p.duration_s}s`}</td>
                  <td className="num"><b>{Math.round(p.watts)} W</b></td>
                  <td className="muted small ell">{p.ride_name} · {dateShort(p.date)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="small muted mt8">{a?.reason} Re-assessed after every ride and once a day, so old efforts age out.</p>
        </>
      )}
    </Card>
  );
}
