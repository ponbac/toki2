import type * as React from "react";
import { cn } from "@/lib/utils";

/** A titled panel for one chart, table or list of the usage page. */
export function ChartCard({
  title,
  description,
  actions,
  className,
  children,
  id,
}: {
  title: string;
  description?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
  id?: string;
}) {
  return (
    <section
      id={id}
      aria-label={title}
      className={cn(
        "flex min-w-0 flex-col gap-4 rounded-2xl border border-border/70 bg-card p-5 text-card-foreground shadow-sm",
        className,
      )}
    >
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <h2 className="text-sm font-semibold">{title}</h2>
          {description && (
            <p className="text-xs text-muted-foreground">{description}</p>
          )}
        </div>
        {actions && <div className="flex shrink-0 gap-2">{actions}</div>}
      </header>
      {children}
    </section>
  );
}

/** A segmented control of two or three mutually exclusive views. */
export function Segmented<Value extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: Value;
  options: readonly Readonly<{ value: Value; label: string }>[];
  onChange: (value: Value) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex rounded-lg border border-border/70 bg-muted/40 p-0.5"
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
          className={cn(
            "rounded-md px-2.5 py-1 text-xs transition-colors",
            option.value === value
              ? "bg-background text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** One row of a chart tooltip: the value leads, the series name follows. */
export type TooltipRow = Readonly<{
  key: string;
  color: string;
  value: string;
  label: string;
}>;

/** The body of a chart tooltip, keyed by short strokes of series colour. */
export function ChartTooltipFrame({
  title,
  rows,
  footer,
}: {
  title: string;
  rows: readonly TooltipRow[];
  footer?: React.ReactNode;
}) {
  return (
    <div className="min-w-44 rounded-lg border border-border/70 bg-popover px-3 py-2 text-xs text-popover-foreground shadow-elevated">
      <p className="mb-1.5 font-medium">{title}</p>
      {rows.length > 0 && (
        <ul className="space-y-1">
          {rows.map((row) => (
            <li key={row.key} className="flex items-center gap-2">
              <span
                aria-hidden
                className="h-0.5 w-3 shrink-0 rounded-full"
                style={{ backgroundColor: row.color }}
              />
              <span className="font-semibold tabular-nums">{row.value}</span>
              <span className="truncate text-muted-foreground">
                {row.label}
              </span>
            </li>
          ))}
        </ul>
      )}
      {footer && (
        <div className="mt-1.5 border-t border-border/60 pt-1.5 text-muted-foreground">
          {footer}
        </div>
      )}
    </div>
  );
}

/** A legend entry: a swatch shaped like the mark, and a text-coloured label. */
export function LegendItem({
  color,
  label,
  shape = "rect",
}: {
  color: string;
  label: React.ReactNode;
  shape?: "rect" | "ring";
}) {
  return (
    <li className="flex items-center gap-1.5 text-xs text-muted-foreground">
      {shape === "rect" ? (
        <span
          aria-hidden
          className="size-2.5 shrink-0 rounded-sm"
          style={{ backgroundColor: color }}
        />
      ) : (
        <span
          aria-hidden
          className="size-2.5 shrink-0 rounded-full border-[1.5px] border-foreground bg-card"
        />
      )}
      <span className="truncate">{label}</span>
    </li>
  );
}
