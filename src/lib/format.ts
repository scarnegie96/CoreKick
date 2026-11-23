import type { Units } from "./types";

export const KM_PER_MI = 1.609344;
export const FT_PER_M = 3.28084;

export function dur(s: number | null | undefined, opts: { hours?: boolean } = {}): string {
  if (s == null || !isFinite(s)) return "–";
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = t % 60;
  if (h > 0 || opts.hours) return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  return `${m}:${String(sec).padStart(2, "0")}`;
}

export function durWords(s: number): string {
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600);
  const m = Math.round((t % 3600) / 60);
  if (h === 0) return `${m} min`;
  return m ? `${h} h ${m} min` : `${h} h`;
}

export function hours(s: number): string {
  return (s / 3600).toFixed(s >= 36000 ? 0 : 1);
}

export function dist(m: number, u: Units, digits = 1): string {
  return u === "imperial" ? (m / 1000 / KM_PER_MI).toFixed(digits) : (m / 1000).toFixed(digits);
}
export const distUnit = (u: Units) => (u === "imperial" ? "mi" : "km");

export function elev(m: number, u: Units): string {
  return Math.round(u === "imperial" ? m * FT_PER_M : m).toLocaleString();
}
export const elevUnit = (u: Units) => (u === "imperial" ? "ft" : "m");

export function speed(kmh: number, u: Units): string {
  return (u === "imperial" ? kmh / KM_PER_MI : kmh).toFixed(1);
}
export const speedUnit = (u: Units) => (u === "imperial" ? "mph" : "km/h");

export function short(m: number, u: Units): string {
  if (u === "imperial") {
    const ft = m * FT_PER_M;
    return ft < 1000 ? `${Math.round(ft)} ft` : `${(m / 1000 / KM_PER_MI).toFixed(1)} mi`;
  }
  return m < 1000 ? `${Math.round(m)} m` : `${(m / 1000).toFixed(1)} km`;
}

export function num(x: number | null | undefined, digits = 0): string {
  if (x == null || !isFinite(x)) return "–";
  return x.toLocaleString(undefined, { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

export function pct(x: number, digits = 1): string {
  return `${x >= 0 ? "" : "−"}${Math.abs(x).toFixed(digits)}%`;
}

export function dateShort(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

export function dateLong(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: "numeric" });
}

export function timeOfDay(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

export function relDay(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const a = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const b = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  const diff = Math.round((a - b) / 86400000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Tomorrow";
  if (diff === -1) return "Yesterday";
  if (diff > 1 && diff < 7) return d.toLocaleDateString(undefined, { weekday: "long" });
  return dateShort(iso);
}

export const ZONE_COLORS = ["#9AA3B2", "#2F7FE0", "#16B57A", "#E3A600", "#F07A1A", "#E5484D", "#8E4EC6"];
export const ZONE_NAMES = ["Recovery", "Endurance", "Tempo", "Threshold", "VO2 Max", "Anaerobic", "Neuromuscular"];
export const ZONE_BOUNDS = [0.55, 0.75, 0.9, 1.05, 1.2, 1.5];

export function zoneIndex(watts: number, ftp: number): number {
  if (!ftp) return 0;
  const f = watts / ftp;
  const i = ZONE_BOUNDS.findIndex((b) => f <= b);
  return i === -1 ? 6 : i;
}
export const zoneColor = (watts: number, ftp: number) => ZONE_COLORS[zoneIndex(watts, ftp)];

/** Colour for a gradient in percent. */
export function gradeColor(g: number): string {
  if (g < -4) return "#1A9ED8";
  if (g < -1) return "#1FA595";
  if (g < 2) return "#1DB870";
  if (g < 4.5) return "#7CB82F";
  if (g < 7) return "#E3A600";
  if (g < 9.5) return "#F07A1A";
  if (g < 12) return "#E5484D";
  return "#9B4DDB";
}

export function categoryColor(cat: string): string {
  switch (cat) {
    case "HC": return "#9B4DDB";
    case "Cat 1": return "#E5484D";
    case "Cat 2": return "#F07A1A";
    case "Cat 3": return "#E3A600";
    case "Cat 4": return "#7CB82F";
    default: return "#1DB870";
  }
}

export function difficultyColor(score: number): string {
  if (score < 2.5) return "#1DB870";
  if (score < 4.5) return "#7CB82F";
  if (score < 6.5) return "#E3A600";
  if (score < 8.5) return "#F07A1A";
  return "#E5484D";
}

const HR5 = ["#9AA3B2", "#2F7FE0", "#16B57A", "#F07A1A", "#E5484D"];
const HR7 = ["#9AA3B2", "#2F7FE0", "#16B57A", "#E3A600", "#F07A1A", "#E5484D", "#8E4EC6"];
/** Colour for a 1-based heart-rate zone in a model with `count` zones. */
export function hrZoneColor(zone: number, count: number): string {
  const t = count > 5 ? HR7 : HR5;
  return t[Math.max(0, Math.min(t.length - 1, zone - 1))];
}

export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function todayISO(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function clamp(x: number, a: number, b: number) {
  return Math.max(a, Math.min(b, x));
}
