import { cn } from "@/lib/utils";

/**
 * A segmented control of two or three mutually exclusive views. As a radio
 * group it is one tab stop, and the arrow keys move the choice.
 */
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
      {options.map((option, index) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          tabIndex={option.value === value ? 0 : -1}
          onClick={() => onChange(option.value)}
          onKeyDown={(event) => {
            const step = ARROW_STEPS[event.key];
            if (step === undefined) {
              return;
            }
            event.preventDefault();
            const next = (index + step + options.length) % options.length;
            onChange(options[next].value);
            const radios =
              event.currentTarget.parentElement?.querySelectorAll<HTMLElement>(
                '[role="radio"]',
              );
            radios?.[next]?.focus();
          }}
          className={cn(
            "rounded-md px-2.5 py-1 text-xs transition-colors",
            option.value === value
              ? "bg-background text-foreground shadow-xs"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** How far each arrow key moves the choice of a radio group. */
const ARROW_STEPS: Readonly<Partial<Record<string, number>>> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};
