import { useApp } from "../lib/app";

/** Compact connection indicator for the sidebar / headers. */
export function DeviceChip({ compact }: { compact?: boolean }) {
  const { devices, go } = useApp();
  let cls = "off";
  let text = "No trainer";
  if (devices?.simulator) {
    cls = "sim";
    text = "Simulator";
  } else if (devices?.trainer.state === "connected") {
    cls = "on";
    text = devices.trainer.name ?? "Trainer";
  } else if (devices?.trainer.state === "connecting" || devices?.trainer.state === "reconnecting") {
    cls = "wait";
    text = devices.trainer.state === "connecting" ? "Connecting…" : "Reconnecting…";
  }
  return (
    <button className={`dev-chip ${cls} ${compact ? "compact" : ""}`} onClick={() => go("devices")} title="Devices">
      <i />
      <span>{text}</span>
      {devices?.hrm.state === "connected" && <b className="hr-dot" title="Heart rate connected">♥</b>}
    </button>
  );
}

export function useTrainerReady() {
  const { devices } = useApp();
  return !!devices && (devices.simulator || devices.trainer.state === "connected");
}
