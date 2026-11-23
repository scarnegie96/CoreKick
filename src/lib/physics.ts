// Lightweight JS mirror of the Rust steady-state speed solver (for settings previews only).
const G = 9.80665;

export function physicsSpeed(power: number, grade: number, mass: number, cda: number, crr: number, eff = 0.976): number {
  const theta = Math.atan(grade);
  const f = (v: number) => power * eff - (mass * G * Math.sin(theta) + crr * mass * G * Math.cos(theta) + 0.5 * 1.225 * cda * v * v) * v;
  let lo = 0, hi = 60;
  for (let i = 0; i < 60; i++) {
    const m = (lo + hi) / 2;
    if (f(m) > 0) lo = m;
    else hi = m;
  }
  return (lo + hi) / 2;
}

import type { HrZone, Profile } from "./types";

/** Mirror of Profile::hr_zones (for live previews of unsaved settings). */
export function hrZonesFor(p: Profile): HrZone[] {
  const max = Math.max(100, p.max_hr);
  const rest = Math.min(p.rest_hr, max - 20);
  const r = (x: number) => Math.round(x);
  if (p.hr_zone_model === "lthr") {
    const l = p.lthr > 0 ? p.lthr : max * 0.9;
    const b = [0, 0.81, 0.9, 0.94, 1.0, 1.03, 1.07];
    const names = ["Recovery", "Aerobic", "Tempo", "Sub-threshold", "Supra-threshold", "Aerobic capacity", "Anaerobic"];
    return b.map((x, i) => ({ zone: i + 1, name: names[i], lo: r(l * x), hi: i < 6 ? r(l * b[i + 1]) : null }));
  }
  const at = (f: number) => (p.hr_zone_model === "max_hr" ? max * f : rest + (max - rest) * f);
  const names = ["Recovery", "Endurance", "Tempo", "Threshold", "VO2 Max"];
  return [0.5, 0.6, 0.7, 0.8, 0.9].map((f, i) => ({ zone: i + 1, name: names[i], lo: r(at(f)), hi: i < 4 ? r(at(f + 0.1)) : null }));
}
