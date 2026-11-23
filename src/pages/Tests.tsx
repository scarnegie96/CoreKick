import { useState } from "react";
import { api, errMsg } from "../lib/api";
import { useApp, useLoad } from "../lib/app";
import { dateShort } from "../lib/format";
import { LineChart, WorkoutChart } from "../components/charts";
import { Btn, Card, NumberInput, PageHeader, Pill, Stat } from "../components/ui";
import { IconPlay } from "../components/icons";
import { EftpCard, FtpSuggestionBanner } from "../components/FtpSuggestion";

const ZONE_ROWS = [
  ["Z1", "Active Recovery", 0, 0.55],
  ["Z2", "Endurance", 0.56, 0.75],
  ["Z3", "Tempo", 0.76, 0.9],
  ["Z4", "Threshold", 0.91, 1.05],
  ["Z5", "VO2 Max", 1.06, 1.2],
  ["Z6", "Anaerobic", 1.21, 1.5],
  ["Z7", "Neuromuscular", 1.51, null],
] as const;
const ZC = ["#9AA3B2", "#2F7FE0", "#16B57A", "#E3A600", "#F07A1A", "#E5484D", "#8E4EC6"];

export default function Tests() {
  const { profile, setProfile, prepareRide, toast } = useApp();
  const tests = useLoad(() => api.listTests(), [profile?.ftp]);
  const stats = useLoad(() => api.accountStats(), [profile?.ftp]);
  const [manual, setManual] = useState<number | null>(null);
  const ftp = profile?.ftp ?? 200;
  const hist = stats.data?.ftp_history ?? [];

  const saveManual = async () => {
    if (manual == null) return;
    try {
      const p = await api.setFtp(manual, "Manual");
      setProfile(p);
      setManual(null);
      toast(`FTP set to ${p.ftp} W`, "good");
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  return (
    <div className="page">
      <PageHeader title="Fitness tests" sub="Find your Functional Threshold Power. Every workout, zone and TSS figure in CoreKick scales from it." />
      <FtpSuggestionBanner />
      <div className="grid g3-1">
        <div className="stack">
          {tests.data?.map((t) => (
            <Card key={t.info.kind}>
              <div className="row between center">
                <div>
                  <h2>{t.info.name}</h2>
                  <p className="muted">{t.info.summary}</p>
                </div>
                <div className="row gap8 center">
                  <Pill>~{t.info.approx_minutes} min</Pill>
                  <Btn kind="primary" onClick={() => prepareRide({ kind: "test", test: t.info.kind })}><IconPlay size={12} /> Start</Btn>
                </div>
              </div>
              <WorkoutChart steps={t.steps} ftp={ftp} height={80} />
              <p className="small mt8">{t.info.how}</p>
              {t.last_result?.test_result && (
                <div className="banner mt8">
                  Last: <b>{t.last_result.test_result.ftp} W</b> on {dateShort(t.last_result.started_at)} — {t.last_result.test_result.detail}
                </div>
              )}
            </Card>
          ))}
        </div>
        <div className="stack">
          <Card title="Current FTP">
            <Stat big label="Functional Threshold Power" value={Math.round(ftp)} unit="W" sub={`${(ftp / (profile?.weight_kg ?? 75)).toFixed(2)} W/kg`} />
            <div className="row gap8 mt12 center">
              <NumberInput value={manual ?? ftp} onChange={setManual} step={1} suffix="W" width={110} />
              <Btn onClick={saveManual} disabled={manual == null || manual === ftp}>Set manually</Btn>
            </div>
          </Card>
          <EftpCard />
          <Card title="Your power zones">
            <table className="tbl compact">
              <tbody>
                {ZONE_ROWS.map(([z, n, lo, hi], i) => (
                  <tr key={z}>
                    <td><i className="dot" style={{ background: ZC[i] }} /> {z}</td>
                    <td>{n}</td>
                    <td className="num">{hi == null ? `> ${Math.round(lo * ftp)}` : `${Math.round(lo * ftp)}–${Math.round(hi * ftp)}`} W</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
          <Card title="FTP history">
            {hist.length > 1 ? (
              <LineChart
                x={hist.map((h) => new Date(h.date).getTime())}
                series={[{ name: "FTP", values: hist.map((h) => h.ftp), color: "var(--accent)", area: true, width: 2 }]}
                height={170}
                xFormat={(v) => dateShort(new Date(v).toISOString())}
                yFormat={(v) => `${Math.round(v)}`}
              />
            ) : (
              <div className="empty-inline">Your FTP history will chart here after a test.</div>
            )}
            <ul className="hist-list">
              {[...hist].reverse().slice(0, 6).map((h, i) => (
                <li key={i}><span className="muted">{dateShort(h.date)}</span><b>{Math.round(h.ftp)} W</b><span className="muted">{h.source}</span></li>
              ))}
            </ul>
          </Card>
        </div>
      </div>
    </div>
  );
}
