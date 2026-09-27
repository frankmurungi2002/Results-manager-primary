/**
 * Shared primitives.
 *
 * Every screen builds from these so that spacing, focus behaviour, disabled
 * states and loading states stay identical across the application — SRS 14.7's
 * "no main feature polished while everything else is an afterthought" applies
 * to the interface as much as to the features.
 */

import {
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
  useEffect,
  useId,
  useRef,
} from "react";
import { AlertTriangle, Check, Info, X, XCircle } from "lucide-react";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md" | "lg";
  icon?: ReactNode;
  loading?: boolean;
  block?: boolean;
}

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  loading = false,
  block = false,
  className,
  children,
  disabled,
  type = "button",
  ...rest
}: ButtonProps) {
  const iconOnly = !children && (icon || loading);
  return (
    <button
      type={type}
      className={cx(
        "btn",
        `btn-${variant}`,
        size === "sm" && "btn-sm",
        size === "lg" && "btn-lg",
        iconOnly && "btn-icon",
        block && "btn-block",
        className,
      )}
      disabled={disabled || loading}
      {...rest}
    >
      {loading ? <span className="spinner" /> : icon}
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

interface FieldProps {
  label?: string;
  hint?: string;
  error?: string | null;
  htmlFor?: string;
  className?: string;
  children: ReactNode;
}

export function Field({ label, hint, error, htmlFor, className, children }: FieldProps) {
  return (
    <div className={cx("field", className)}>
      {label && (
        <label className="field-label" htmlFor={htmlFor}>
          {label}
        </label>
      )}
      {children}
      {error ? (
        <span className="field-error">{error}</span>
      ) : hint ? (
        <span className="field-hint">{hint}</span>
      ) : null}
    </div>
  );
}

interface TextInputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  hint?: string;
  error?: string | null;
}

export function TextInput({ label, hint, error, className, ...rest }: TextInputProps) {
  const id = useId();
  return (
    <Field label={label} hint={hint} error={error} htmlFor={id}>
      <input
        id={id}
        className={cx("input", error && "is-invalid", className)}
        {...rest}
      />
    </Field>
  );
}

interface SelectInputProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  hint?: string;
  error?: string | null;
}

export function SelectInput({
  label,
  hint,
  error,
  className,
  children,
  ...rest
}: SelectInputProps) {
  const id = useId();
  return (
    <Field label={label} hint={hint} error={error} htmlFor={id}>
      <select
        id={id}
        className={cx("select", error && "is-invalid", className)}
        {...rest}
      >
        {children}
      </select>
    </Field>
  );
}

interface TextAreaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  hint?: string;
  error?: string | null;
}

export function TextArea({ label, hint, error, className, ...rest }: TextAreaProps) {
  const id = useId();
  return (
    <Field label={label} hint={hint} error={error} htmlFor={id}>
      <textarea id={id} className={cx("textarea", className)} {...rest} />
    </Field>
  );
}

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  label?: string;
}

export function Switch({ checked, onChange, disabled, label }: SwitchProps) {
  return (
    <span className="switch">
      <input
        type="checkbox"
        role="switch"
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="switch-track" />
      <span className="switch-thumb" />
    </span>
  );
}

// ---------------------------------------------------------------------------
// Surfaces
// ---------------------------------------------------------------------------

interface CardProps {
  title?: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  footer?: ReactNode;
  flush?: boolean;
  className?: string;
  children: ReactNode;
}

export function Card({
  title,
  subtitle,
  actions,
  footer,
  flush = false,
  className,
  children,
}: CardProps) {
  return (
    <section className={cx("card", className)}>
      {(title || actions) && (
        <header className="card-header">
          <div className="grow">
            {title && <div className="card-title">{title}</div>}
            {subtitle && <div className="card-subtitle">{subtitle}</div>}
          </div>
          {actions && <div className="row">{actions}</div>}
        </header>
      )}
      <div className={cx("card-body", flush && "card-body-flush")}>{children}</div>
      {footer && <footer className="card-footer">{footer}</footer>}
    </section>
  );
}

type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger" | "info";

export function Badge({
  tone = "neutral",
  children,
}: {
  tone?: BadgeTone;
  children: ReactNode;
}) {
  return <span className={cx("badge", `badge-${tone}`)}>{children}</span>;
}

export function StatTile({
  label,
  value,
  meta,
}: {
  label: string;
  value: ReactNode;
  meta?: ReactNode;
}) {
  return (
    <div className="card stat">
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {meta && <div className="stat-meta">{meta}</div>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

export function Alert({
  tone = "info",
  title,
  children,
}: {
  tone?: "info" | "success" | "warning" | "danger";
  title?: string;
  children: ReactNode;
}) {
  const Icon =
    tone === "success"
      ? Check
      : tone === "warning"
        ? AlertTriangle
        : tone === "danger"
          ? XCircle
          : Info;

  return (
    <div className={cx("alert", `alert-${tone}`)} role="status">
      <Icon size={16} />
      <div>
        {title && <div className="alert-title">{title}</div>}
        <div>{children}</div>
      </div>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  children,
  action,
}: {
  icon?: ReactNode;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      {icon && <div className="empty-icon">{icon}</div>}
      <div className="empty-title">{title}</div>
      {children && <p className="empty-text">{children}</p>}
      {action}
    </div>
  );
}

export function Loading({ label = "Loading" }: { label?: string }) {
  return (
    <div className="empty">
      <span className="spinner" />
      <span className="empty-text">{label}…</span>
    </div>
  );
}

export function TableSkeleton({ rows = 6, columns = 4 }: { rows?: number; columns?: number }) {
  return (
    <div style={{ padding: "var(--space-4)" }}>
      <div className="stack" style={{ gap: "var(--space-3)" }}>
        {Array.from({ length: rows }).map((_, rowIndex) => (
          <div key={rowIndex} className="row" style={{ gap: "var(--space-4)" }}>
            {Array.from({ length: columns }).map((_, columnIndex) => (
              <div
                key={columnIndex}
                className="skeleton"
                style={{
                  height: 13,
                  flex: columnIndex === 0 ? 2 : 1,
                  opacity: 1 - rowIndex * 0.1,
                }}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Modal
// ---------------------------------------------------------------------------

interface ModalProps {
  open: boolean;
  title: string;
  description?: string;
  onClose: () => void;
  footer?: ReactNode;
  wide?: boolean;
  children: ReactNode;
}

export function Modal({
  open,
  title,
  description,
  onClose,
  footer,
  wide = false,
  children,
}: ModalProps) {
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKeyDown);

    // Focus the first control so keyboard users are not stranded on the body.
    const focusable = panel.current?.querySelector<HTMLElement>(
      "input, select, textarea, button:not([disabled])",
    );
    focusable?.focus();

    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panel}
        className={cx("modal", wide && "modal-wide")}
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <header className="modal-header">
          <div>
            <h2 style={{ fontSize: "var(--text-lg)" }}>{title}</h2>
            {description && (
              <p className="muted" style={{ fontSize: "var(--text-sm)", marginTop: 4 }}>
                {description}
              </p>
            )}
          </div>
          <Button variant="ghost" size="sm" icon={<X size={16} />} onClick={onClose} />
        </header>
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-footer">{footer}</footer>}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

export function Progress({ value, max = 100 }: { value: number; max?: number }) {
  const percent = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;
  return (
    <div
      className="progress"
      role="progressbar"
      aria-valuenow={Math.round(percent)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="progress-bar" style={{ width: `${percent}%` }} />
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented" role="tablist">
      {options.map((option) => (
        <button
          key={option.value}
          role="tab"
          aria-selected={value === option.value}
          className={cx(value === option.value && "is-active")}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Two letters for an avatar.
 *
 * Tolerates a missing name on purpose. A presentational helper must never be
 * able to unmount the application: when this threw on an undefined name, React
 * tore down the whole tree and a headteacher got a blank window seconds after
 * signing in. A missing name is worth a placeholder, never a dead screen.
 *
 * The underlying cause of a missing field is caught at build time instead, by
 * the IPC shape check in `scripts/verify.py`.
 */
export function initials(name: string | null | undefined): string {
  if (typeof name !== "string") return "?";
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

/** Dates on screen read the way a Ugandan school office writes them. */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return `${date.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
  })}, ${date.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}`;
}

export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "never";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "never";

  const seconds = Math.round((Date.now() - then) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hr ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} day${days === 1 ? "" : "s"} ago`;
  return formatDate(iso);
}
