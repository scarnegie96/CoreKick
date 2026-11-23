// Audio cues: synthesized tones (Web Audio) + optional spoken prompts.
// No audio files needed; works offline.

let ctx: AudioContext | null = null;

function ac(): AudioContext | null {
  try {
    if (!ctx) ctx = new (window.AudioContext || (window as any).webkitAudioContext)();
    if (ctx.state === "suspended") ctx.resume().catch(() => {});
    return ctx;
  } catch {
    return null;
  }
}

/** Call from a user gesture (click/keypress) so the browser allows audio. */
export function unlockAudio() {
  ac();
}

function tone(freq: number, start: number, dur: number, volume: number, type: OscillatorType = "sine") {
  const c = ac();
  if (!c) return;
  const t = c.currentTime + start;
  const o = c.createOscillator();
  const g = c.createGain();
  o.type = type;
  o.frequency.setValueAtTime(freq, t);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(Math.max(0.0002, volume), t + 0.02);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  o.connect(g).connect(c.destination);
  o.start(t);
  o.stop(t + dur + 0.05);
}

export type Cue = "high" | "low" | "back" | "test";

export function playCue(cue: Cue, volume = 0.7) {
  const v = Math.max(0, Math.min(1, volume)) * 0.35;
  switch (cue) {
    case "high": // urgent, descending: ease off
      tone(988, 0, 0.16, v, "triangle");
      tone(784, 0.18, 0.16, v, "triangle");
      tone(587, 0.36, 0.28, v, "triangle");
      break;
    case "low": // ascending: push on
      tone(523, 0, 0.16, v);
      tone(659, 0.18, 0.16, v);
      tone(784, 0.36, 0.28, v);
      break;
    case "back": // gentle confirmation
      tone(880, 0, 0.12, v * 0.6);
      tone(1175, 0.12, 0.22, v * 0.6);
      break;
    case "test":
      tone(660, 0, 0.18, v);
      break;
  }
}

export function speak(text: string, volume = 0.7) {
  try {
    const s = window.speechSynthesis;
    if (!s) return;
    s.cancel();
    const u = new SpeechSynthesisUtterance(text);
    u.volume = Math.max(0, Math.min(1, volume));
    u.rate = 1.05;
    s.speak(u);
  } catch {
    /* speech not available */
  }
}
