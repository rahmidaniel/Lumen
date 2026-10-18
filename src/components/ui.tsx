import { clsx } from "clsx";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { hexToRgb, rgbToHex } from "../lib/color";
import type { Rgb } from "../lib/types";

export const cx = clsx;

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "ghost" | "danger";
  size?: "sm" | "md";
};

export function Button({ variant = "default", size = "md", className, ...props }: ButtonProps) {
  return (
    <button
      className={cx(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:opacity-40 disabled:pointer-events-none",
        size === "sm" ? "h-7 px-2.5 text-xs" : "h-9 px-3.5 text-sm",
        variant === "default" && "bg-surface-3 text-text hover:bg-border-strong border border-border",
        variant === "primary" && "bg-accent text-white hover:brightness-110 shadow-[0_6px_20px_-8px_var(--color-accent)]",
        variant === "ghost" && "text-muted hover:text-text hover:bg-surface-3",
        variant === "danger" && "text-danger hover:bg-danger/10",
        className,
      )}
      {...props}
    />
  );
}

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { label: string; active?: boolean; size?: "sm" | "md" };

export function IconButton({ label, active, size = "md", className, ...props }: IconButtonProps) {
  return (
    <button
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex items-center justify-center rounded-md transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:opacity-40 disabled:pointer-events-none",
        size === "sm" ? "h-7 w-7" : "h-9 w-9",
        active ? "bg-accent-soft text-text" : "text-muted hover:text-text hover:bg-surface-3",
        className,
      )}
      {...props}
    />
  );
}

export function Badge({ tone = "neutral", children, className }: { tone?: "neutral" | "ok" | "warn" | "danger" | "accent"; children: ReactNode; className?: string }) {
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium leading-4",
        tone === "neutral" && "bg-surface-3 text-muted",
        tone === "ok" && "bg-ok/15 text-ok",
        tone === "warn" && "bg-warn/15 text-warn",
        tone === "danger" && "bg-danger/15 text-danger",
        tone === "accent" && "bg-accent-soft text-[#c4b5fd]",
        className,
      )}
    >
      {children}
    </span>
  );
}

export function Panel({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col min-h-0">
      <header className="flex items-center justify-between px-4 h-12 shrink-0">
        <h2 className="text-sm font-semibold tracking-tight">{title}</h2>
        {actions && <div className="flex items-center gap-1">{actions}</div>}
      </header>
      <div className="px-4 pb-4 flex flex-col gap-3">{children}</div>
    </section>
  );
}

export function Field({ label, hint, children, inline }: { label: string; hint?: ReactNode; children: ReactNode; inline?: boolean }) {
  return (
    <label className={cx("flex gap-1.5", inline ? "flex-row items-center justify-between" : "flex-col")}>
      <span className="flex items-center justify-between text-xs text-muted">
        <span>{label}</span>
        {hint && <span className="font-mono text-faint tabular-nums">{hint}</span>}
      </span>
      {children}
    </label>
  );
}

type SliderProps = {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
};

export function Slider({ label, value, min, max, step = 0.01, onChange, format }: SliderProps) {
  const fill = `${((value - min) / (max - min)) * 100}%`;
  return (
    <Field label={label} hint={(format ?? ((v) => v.toFixed(2)))(value)}>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        style={{ ["--fill" as string]: fill }}
        onChange={(e) => onChange(parseFloat(e.target.value))}
      />
    </Field>
  );
}

/** An on/off switch. Fixed size: it never gives up width to long text beside it. */
export function Toggle({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label?: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-150",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-surface-2",
        "disabled:cursor-not-allowed",
        checked ? "bg-accent hover:brightness-110" : "bg-surface-3 inset-ring inset-ring-border-strong hover:bg-border",
      )}
    >
      <span
        className={cx(
          "pointer-events-none block h-4 w-4 rounded-full shadow-[0_1px_2px_rgb(0_0_0/0.4)] transition-[translate,background-color] duration-150 ease-out",
          checked ? "translate-x-4 bg-white" : "translate-x-0 bg-muted",
        )}
      />
    </button>
  );
}

export function Select<T extends string>({ value, options, onChange, className }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; className?: string }) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className={cx(
        "h-9 w-full rounded-md border border-border bg-surface-2 px-2.5 text-sm text-text outline-none",
        "focus-visible:ring-2 focus-visible:ring-accent/60",
        className,
      )}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function TextInput({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cx(
        "h-9 w-full rounded-md border border-border bg-surface-2 px-2.5 text-sm text-text outline-none placeholder:text-faint",
        "focus-visible:ring-2 focus-visible:ring-accent/60",
        className,
      )}
      {...props}
    />
  );
}

export function NumberInput({ value, onChange, step = 1, className }: { value: number; onChange: (v: number) => void; step?: number; className?: string }) {
  return (
    <input
      type="number"
      step={step}
      value={Number.isFinite(value) ? Math.round(value * 100) / 100 : 0}
      onChange={(e) => {
        const v = parseFloat(e.target.value);
        if (Number.isFinite(v)) onChange(v);
      }}
      className={cx(
        "h-8 w-full rounded-md border border-border bg-surface-2 px-2 text-sm font-mono tabular-nums text-text outline-none",
        "focus-visible:ring-2 focus-visible:ring-accent/60",
        className,
      )}
    />
  );
}

export function ColorInput({ value, onChange, label }: { value: Rgb; onChange: (c: Rgb) => void; label?: string }) {
  const hex = rgbToHex(value);
  return (
    <div className="flex items-center gap-2">
      <input type="color" value={hex} aria-label={label ?? "Colour"} onChange={(e) => onChange(hexToRgb(e.target.value))} />
      <span className="font-mono text-xs text-muted uppercase">{hex}</span>
    </div>
  );
}
