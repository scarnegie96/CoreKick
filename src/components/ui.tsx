import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { useApp } from "../lib/app";
import { IconX } from "./icons";

export function useSize<T extends HTMLElement>(): [React.RefObject<T>, { w: number; h: number }] {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const r = e.contentRect;
      setSize((s) => (Math.abs(s.w - r.width) > 0.5 || Math.abs(s.h - r.height) > 0.5 ? { w: r.width, h: r.height } : s));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size];
}

export function Card({ title, action, children, className = "", pad = true }: { title?: ReactNode; action?: ReactNode; children: ReactNode; className?: string; pad?: boolean }) {
  return (
    <section className={`card ${pad ? "pad" : ""} ${className}`}>
      {(title || action) && (
        <header className="card-h">
          {title && <h3>{title}</h3>}
          {action && <div className="card-a">{action}</div>}
        </header>
      )}
      {children}
    </section>
  );
}

export function Stat({ label, value, unit, sub, color, big }: { label: ReactNode; value: ReactNode; unit?: ReactNode; sub?: ReactNode; color?: string; big?: boolean }) {
  return (
    <div className={`stat ${big ? "big" : ""}`}>
      <div className="stat-l">{label}</div>
      <div className="stat-v" style={color ? { color } : undefined}>
        {value}
        {unit && <span className="stat-u">{unit}</span>}
      </div>
      {sub && <div className="stat-s">{sub}</div>}
    </div>
  );
}

export function Btn({ children, kind = "default", size, onClick, disabled, title, type = "button", className = "" }: {
  children: ReactNode;
  kind?: "default" | "primary" | "ghost" | "danger" | "subtle";
  size?: "sm" | "lg";
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
  type?: "button" | "submit";
  className?: string;
}) {
  return (
    <button type={type} title={title} className={`btn ${kind} ${size ?? ""} ${className}`} onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}

export function Segmented<T extends string | number>({ value, options, onChange, size }: { value: T; options: { value: T; label: ReactNode }[]; onChange: (v: T) => void; size?: "sm" }) {
  return (
    <div className={`seg ${size ?? ""}`}>
      {options.map((o) => (
        <button key={String(o.value)} className={o.value === value ? "on" : ""} onClick={() => onChange(o.value)} type="button">
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Field({ label, children, hint, inline }: { label: ReactNode; children: ReactNode; hint?: ReactNode; inline?: boolean }) {
  return (
    <label className={`field ${inline ? "inline" : ""}`}>
      <span className="field-l">{label}</span>
      {children}
      {hint && <span className="field-h">{hint}</span>}
    </label>
  );
}

export function NumberInput({ value, onChange, step = 1, min, max, suffix, width }: { value: number; onChange: (v: number) => void; step?: number; min?: number; max?: number; suffix?: string; width?: number }) {
  const [text, setText] = useState(String(value));
  useEffect(() => {
    if (Number(text) !== value) setText(String(value));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);
  return (
    <span className="num-in" style={width ? { width } : undefined}>
      <input
        type="number"
        value={text}
        step={step}
        min={min}
        max={max}
        onChange={(e) => {
          setText(e.target.value);
          const v = parseFloat(e.target.value);
          if (!isNaN(v)) onChange(v);
        }}
      />
      {suffix && <span className="num-suf">{suffix}</span>}
    </span>
  );
}

export function Slider({ value, min, max, step = 1, onChange, format }: { value: number; min: number; max: number; step?: number; onChange: (v: number) => void; format?: (v: number) => ReactNode }) {
  const pctv = ((value - min) / (max - min)) * 100;
  return (
    <div className="slider">
      <input type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(parseFloat(e.target.value))} style={{ ["--p" as any]: `${pctv}%` }} />
      {format && <span className="slider-v">{format(value)}</span>}
    </div>
  );
}

export function Toggle({ value, onChange, label }: { value: boolean; onChange: (v: boolean) => void; label?: ReactNode }) {
  return (
    <button type="button" className={`toggle ${value ? "on" : ""}`} onClick={() => onChange(!value)}>
      <span className="knob" />
      {label && <span className="toggle-l">{label}</span>}
    </button>
  );
}

export function Pill({ children, color, solid }: { children: ReactNode; color?: string; solid?: boolean }) {
  return (
    <span className={`pill ${solid ? "solid" : ""}`} style={color ? ({ ["--c" as any]: color } as React.CSSProperties) : undefined}>
      {children}
    </span>
  );
}

export function Modal({ title, onClose, children, footer, wide }: { title: ReactNode; onClose: () => void; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);
  return (
    <div className="modal-bg" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`modal ${wide ? "wide" : ""}`}>
        <header className="modal-h">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label="Close">
            <IconX />
          </button>
        </header>
        <div className="modal-b">{children}</div>
        {footer && <footer className="modal-f">{footer}</footer>}
      </div>
    </div>
  );
}

export function Empty({ icon, title, children }: { icon?: ReactNode; title: ReactNode; children?: ReactNode }) {
  return (
    <div className="empty">
      {icon && <div className="empty-i">{icon}</div>}
      <h4>{title}</h4>
      {children && <div className="empty-b">{children}</div>}
    </div>
  );
}

export function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          {t.text}
        </div>
      ))}
    </div>
  );
}

export function PageHeader({ title, sub, actions, back }: { title: ReactNode; sub?: ReactNode; actions?: ReactNode; back?: () => void }) {
  return (
    <header className="page-h">
      <div>
        {back && (
          <button className="back-link" onClick={back}>
            ← Back
          </button>
        )}
        <h1>{title}</h1>
        {sub && <p className="page-sub">{sub}</p>}
      </div>
      {actions && <div className="page-a">{actions}</div>}
    </header>
  );
}

export function Ring({ value, size = 64, stroke = 7, color = "var(--accent)", children }: { value: number; size?: number; stroke?: number; color?: string; children?: ReactNode }) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const v = Math.max(0, Math.min(1, value));
  return (
    <div className="ring" style={{ width: size, height: size }}>
      <svg width={size} height={size}>
        <circle cx={size / 2} cy={size / 2} r={r} stroke="var(--line)" strokeWidth={stroke} fill="none" />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          stroke={color}
          strokeWidth={stroke}
          fill="none"
          strokeLinecap="round"
          strokeDasharray={`${c * v} ${c}`}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
      </svg>
      <div className="ring-c">{children}</div>
    </div>
  );
}

/** Native confirmation dialog (window.confirm is not reliable in WKWebView). */
export async function confirmAsk(msg: string, okLabel = "Delete"): Promise<boolean> {
  try {
    return await ask(msg, { title: "CoreKick", kind: "warning", okLabel, cancelLabel: "Cancel" });
  } catch {
    return window.confirm(msg);
  }
}
