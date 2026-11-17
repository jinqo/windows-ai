import type { ReactNode } from "react";

export function Toggle({ on, onChange, disabled, label }: { on: boolean; onChange: (v: boolean) => void; disabled?: boolean; label?: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      className={`toggle ${on ? "on" : ""}`}
      onClick={() => onChange(!on)}
    />
  );
}

export function Field({ label, hint, children, stack }: { label: ReactNode; hint?: ReactNode; children: ReactNode; stack?: boolean }) {
  return (
    <div className={`field ${stack ? "stack" : ""}`}>
      <div className="label">
        <b>{label}</b>
        {hint && <span>{hint}</span>}
      </div>
      <div className="control">{children}</div>
    </div>
  );
}

export function Segmented<T extends string>({
  value, options, onChange, disabled,
}: { value: T; options: { value: T; label: string; disabled?: boolean; className?: string }[]; onChange: (v: T) => void; disabled?: boolean }) {
  return (
    <div className="seg" role="radiogroup">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          disabled={disabled || o.disabled}
          className={`${value === o.value ? "on" : ""} ${o.className ?? ""}`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Card({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <>
      {title && <div className="card-title">{title}</div>}
      <div className="card">{children}</div>
    </>
  );
}

export function Notice({ kind = "info", children, icon }: { kind?: "info" | "warn" | "err" | "ok"; children: ReactNode; icon?: ReactNode }) {
  return (
    <div className={`notice ${kind}`}>
      {icon}
      <div>{children}</div>
    </div>
  );
}
