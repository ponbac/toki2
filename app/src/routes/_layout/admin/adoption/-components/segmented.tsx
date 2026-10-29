import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * A small set of exclusive options, as radio buttons: arrow keys move between
 * them, and only the chosen one is in the tab order.
 */
export function Segmented<T extends string | number>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: ReadonlyArray<Readonly<{ value: T; label: string }>>;
  value: T;
  onChange: (value: T) => void;
}) {
  const buttons = React.useRef(new Map<T, HTMLButtonElement>());
  const step = (from: T, by: number) => {
    const index = options.findIndex((option) => option.value === from) + by;
    const next = options[(index + options.length) % options.length];
    if (next) {
      onChange(next.value);
      buttons.current.get(next.value)?.focus();
    }
  };

  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex shrink-0 gap-0.5 rounded-lg border border-border/70 bg-muted/40 p-0.5"
    >
      {options.map((option) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            ref={(button) => {
              if (button) {
                buttons.current.set(option.value, button);
              } else {
                buttons.current.delete(option.value);
              }
            }}
            type="button"
            role="radio"
            aria-checked={checked}
            tabIndex={checked ? 0 : -1}
            onClick={() => onChange(option.value)}
            onKeyDown={(event) => {
              const by =
                event.key === "ArrowRight" || event.key === "ArrowDown"
                  ? 1
                  : event.key === "ArrowLeft" || event.key === "ArrowUp"
                    ? -1
                    : 0;
              if (by !== 0) {
                event.preventDefault();
                step(option.value, by);
              }
            }}
            className={cn(
              "inline-flex h-7 items-center rounded-md px-2.5 text-xs font-medium whitespace-nowrap transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring motion-reduce:transition-none",
              checked
                ? "bg-background text-foreground shadow-xs ring-1 ring-border ring-inset"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

/** A section's title, the one line saying how to read it, and its controls. */
export function SectionHead({
  id,
  title,
  description,
  children,
}: {
  id?: string;
  title: string;
  description: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="max-w-[72ch] min-w-0">
        <h2 id={id} className="text-base font-bold">
          {title}
        </h2>
        <p className="mt-0.5 text-[13px] text-muted-foreground">
          {description}
        </p>
      </div>
      {children ? <div className="flex flex-wrap gap-2">{children}</div> : null}
    </div>
  );
}
