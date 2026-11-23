import { useEffect, useState } from "react";
import { api, errMsg, useTauriEvent } from "../lib/api";
import { useApp } from "../lib/app";
import type { DeviceInfo, Link, LiveTelemetry, SimSettings } from "../lib/types";
import { Btn, Card, PageHeader, Pill, Slider, Stat, Toggle } from "../components/ui";
import { IconBluetooth, IconBolt, IconHeart } from "../components/icons";

function LinkCard({ title, icon, link, onDisconnect, children }: { title: string; icon: React.ReactNode; link?: Link; onDisconnect: () => void; children?: React.ReactNode }) {
  const st = link?.state ?? "disconnected";
  return (
    <Card className={`link-card ${st}`}>
      <div className="row between center">
        <div className="row gap12 center">
          <div className="link-ic">{icon}</div>
          <div>
            <div className="stat-l">{title}</div>
            <h3>{link?.name ?? "Not connected"}</h3>
            <div className={`link-st ${st}`}>{st === "connected" ? "Connected" : st === "connecting" ? "Connecting…" : st === "reconnecting" ? "Reconnecting…" : "Disconnected"}</div>
          </div>
        </div>
        {st !== "disconnected" && <Btn size="sm" kind="ghost" onClick={onDisconnect}>Disconnect</Btn>}
      </div>
      {link?.detail && <div className="banner warn mt8 small">{link.detail}</div>}
      {children}
    </Card>
  );
}

export default function Devices() {
  const { devices, refreshDevices, toast, profile } = useApp();
  const [list, setList] = useState<DeviceInfo[]>([]);
  const [tele, setTele] = useState<LiveTelemetry | null>(null);
  const [sim, setSim] = useState<SimSettings | null>(null);

  useEffect(() => {
    api.bleDevices().then(setList);
    api.simGet().then(setSim);
    refreshDevices();
  }, [refreshDevices]);
  useTauriEvent<DeviceInfo[]>("ble://devices", setList);
  useTauriEvent<LiveTelemetry>("ble://telemetry", setTele);

  const scan = async () => {
    try {
      await api.bleScan(25);
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const connect = async (d: DeviceInfo, role: "trainer" | "hrm") => {
    try {
      await api.bleConnect(d.id, role);
      if (role === "trainer" && sim?.enabled) setSim({ ...sim, enabled: false });
      refreshDevices();
    } catch (e) {
      toast(errMsg(e), "bad");
    }
  };

  const setSimS = async (s: SimSettings) => {
    setSim(s);
    await api.simSet(s);
    refreshDevices();
  };

  return (
    <div className="page">
      <PageHeader
        title="Devices"
        sub="KICKR CORE connects over Bluetooth FTMS. Heart-rate straps are optional."
        actions={
          <Btn kind="primary" onClick={scan} disabled={devices?.scanning}>
            <IconBluetooth size={14} /> {devices?.scanning ? "Scanning…" : "Scan for devices"}
          </Btn>
        }
      />
      {devices && !devices.available && (
        <div className="banner bad mb16">
          Bluetooth unavailable: {devices.error}. On macOS, allow CoreKick under System Settings → Privacy & Security → Bluetooth.
        </div>
      )}
      <div className="grid g3">
        <LinkCard title="Smart trainer" icon={<IconBolt size={22} />} link={devices?.trainer} onDisconnect={() => api.bleDisconnect("trainer").then(refreshDevices)}>
          {devices?.trainer.power_range && <div className="muted small mt8">ERG range {devices.trainer.power_range[0]}–{devices.trainer.power_range[1]} W</div>}
        </LinkCard>
        <LinkCard title="Heart rate" icon={<IconHeart size={22} />} link={devices?.hrm} onDisconnect={() => api.bleDisconnect("hrm").then(refreshDevices)} />
        <Card className="live-tele">
          <div className="stat-l">Live readings · {tele?.source === "simulator" ? "simulator" : "Bluetooth"}</div>
          <div className="row gap24 mt8">
            <Stat label="Power" value={tele?.connected ? Math.round(tele.power_w) : "–"} unit="W" />
            <Stat label="Cadence" value={tele?.connected ? Math.round(tele.cadence_rpm) : "–"} unit="rpm" />
            <Stat label="HR" value={tele?.heart_rate ? Math.round(tele.heart_rate) : "–"} unit="bpm" />
          </div>
        </Card>
      </div>

      <div className="grid g2 mt16">
        <Card title="Nearby devices" action={devices?.scanning && <span className="scan-dot">Scanning</span>}>
          {list.length === 0 ? (
            <div className="empty-inline">
              {devices?.scanning ? "Looking for trainers and heart-rate monitors…" : "Press “Scan for devices”. Pedal the KICKR briefly to wake it up."}
            </div>
          ) : (
            <ul className="dev-list">
              {list.map((d) => {
                const isTrainer = devices?.trainer.device_id === d.id && devices.trainer.state !== "disconnected";
                const isHrm = devices?.hrm.device_id === d.id && devices.hrm.state !== "disconnected";
                return (
                  <li key={d.id}>
                    <div className="dev-ic">{d.kind === "heart_rate" ? <IconHeart size={18} /> : <IconBolt size={18} />}</div>
                    <div className="grow">
                      <b>{d.name}</b>
                      <div className="row gap6 mt4">
                        <Pill>{d.kind === "trainer" ? "Trainer" : d.kind === "heart_rate" ? "Heart rate" : "Power meter"}</Pill>
                        {d.ftms && <Pill color="var(--accent)">FTMS</Pill>}
                        {d.rssi != null && <span className="muted small">{d.rssi} dBm</span>}
                      </div>
                    </div>
                    {d.kind === "heart_rate" ? (
                      <Btn size="sm" kind={isHrm ? "subtle" : "primary"} disabled={isHrm} onClick={() => connect(d, "hrm")}>{isHrm ? "Connected" : "Connect"}</Btn>
                    ) : (
                      <Btn size="sm" kind={isTrainer ? "subtle" : "primary"} disabled={isTrainer} onClick={() => connect(d, "trainer")}>{isTrainer ? "Connected" : "Use as trainer"}</Btn>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </Card>
        <div className="stack">
          <Card title="Simulator">
            <p className="muted">No bike handy? The simulator stands in for the KICKR and a rider, so you can try routes, workouts and tests from your desk.</p>
            {sim && (
              <div className="form mt8">
                <Toggle value={sim.enabled} onChange={(v) => setSimS({ ...sim, enabled: v })} label={sim.enabled ? "Simulator active" : "Use simulator"} />
                <div>
                  <div className="field-l">Simulated rider effort</div>
                  <Slider value={sim.effort_w} min={0} max={Math.round((profile?.ftp ?? 200) * 1.8)} step={5} onChange={(v) => setSimS({ ...sim, effort_w: v })} format={(v) => `${v} W`} />
                </div>
              </div>
            )}
          </Card>
          <Card title="Calibration (spin-down)">
            <p className="muted small">
              A spin-down measures how quickly the flywheel coasts down with the brake released, so the trainer can account for belt tension, bearing friction
              and temperature – that's what keeps power accurate to ±2 %.
            </p>
            <ul className="tips">
              <li>KICKR CORE calibrates itself automatically: after a 10-minute warm-up, stop pedalling for a few seconds and let the flywheel coast.</li>
              <li>CORE (v1) also supports a manual spin-down in the Wahoo app – do one after moving the trainer or if power seems off.</li>
              <li>A <i>factory</i> (advanced) spin-down is rarely needed: in the Wahoo app, open the trainer and tap its icon 10 times.</li>
            </ul>
          </Card>
          <Card title="Connection tips">
            <ul className="tips">
              <li>
                <b>COROS PACE Pro as a heart-rate monitor:</b> long-press BACK/LAP → Toolbox → <i>Broadcast HR</i>, then Scan here and connect it under Heart rate. During
                broadcast the watch allows only one Bluetooth link, so disconnect it from your phone first.
              </li>
              <li>Using the Zwift Cog? Turn on <b>virtual shifting</b> in Profile → Drivetrain. ERG workouts don't need it.</li>
              <li>Update the KICKR CORE firmware in the Wahoo app – FTMS control arrived in the 2021 firmware.</li>
              <li>Only one app can control the trainer. Close Zwift, the Wahoo app and others before connecting.</li>
              <li>KICKR CORE accepts up to three Bluetooth connections, so a watch can still read power alongside CoreKick.</li>
              <li>If the trainer drops out mid-ride CoreKick reconnects automatically and re-sends the current gradient or ERG target.</li>
              <li>Spin down/calibrate occasionally in the Wahoo app for the most accurate power.</li>
            </ul>
          </Card>
        </div>
      </div>
    </div>
  );
}
