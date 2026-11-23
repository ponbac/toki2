import dayjs from "dayjs";
import type { FlexBreakdown } from "@/lib/flex-breakdown";
import { formatSignedHours, signedHoursColor } from "@/lib/flex-balance";
import { cn, formatHoursMinutes } from "@/lib/utils";

const SETTLED_SWATCH_STYLE = {
  background: "hsl(var(--primary))",
} as const;

const ESTIMATED_SWATCH_STYLE = {
  background:
    "repeating-linear-gradient(135deg, hsl(var(--primary) / 0.55) 0 2px, transparent 2px 5px)",
  boxShadow: "inset 0 0 0 1px hsl(var(--primary) / 0.5)",
} as const;

const TODAY_SWATCH_STYLE = {
  borderRadius: "9999px",
  boxShadow: "inset 0 0 0 1.5px hsl(var(--primary) / 0.7)",
} as const;

/**
 * The flex balance written out as a sum: what the provider has settled and
 * the estimate up to yesterday make the balance before today, and today's
 * hours so far follow it.
 */
export function FlexBalanceBreakdown(props: {
  breakdown: FlexBreakdown;
  /** Keeps the total at the rows' size, for tooltips. */
  compact?: boolean;
  className?: string;
}) {
  const { settled, earlier, beforeToday, today, timerHours, nowHours } =
    props.breakdown;
  // Only a provider that has settled today leaves no balance before it.
  const total = beforeToday ?? nowHours;

  return (
    <div className={cn("flex flex-col gap-1", props.className)}>
      <BreakdownRow
        swatchStyle={SETTLED_SWATCH_STYLE}
        label={`Settled in Kleer, ${dayjs(settled.through).format("MMM D")}`}
        hours={settled.hours}
      />
      {earlier && (
        <BreakdownRow
          swatchStyle={ESTIMATED_SWATCH_STYLE}
          label={
            earlier.from === earlier.through
              ? "Yesterday"
              : `${dayjs(earlier.from).format("MMM D")} to yesterday`
          }
          hours={earlier.hours}
        />
      )}
      {timerHours !== null && (
        <BreakdownRow
          swatchStyle={TODAY_SWATCH_STYLE}
          label="Running timer"
          hours={timerHours}
        />
      )}
      {total !== null && (
        <div className="mt-1 flex items-baseline justify-between gap-4 border-t border-border/50 pt-2 font-semibold">
          <span>
            {beforeToday === null ? "Preliminary total" : "Before today"}
          </span>
          <span
            className={cn(
              "time-display",
              !props.compact && "text-xl",
              signedHoursColor(total),
            )}
          >
            {formatSignedHours(total)}
          </span>
        </div>
      )}
      {today && nowHours !== null && (
        <>
          <div className="mt-2">
            <BreakdownRow
              swatchStyle={TODAY_SWATCH_STYLE}
              label={
                today.scheduledHours > 0
                  ? `Today, ${formatHoursMinutes(today.coveredHours)} of ${formatHoursMinutes(today.scheduledHours)}`
                  : "Today"
              }
              hours={today.hours}
            />
            {today.scheduledHours > 0 && (
              <div
                aria-hidden
                className="mt-1 ml-4.5 h-1 overflow-hidden rounded-full bg-muted"
              >
                <div
                  className="h-full rounded-full bg-primary"
                  style={{
                    width: `${Math.min(today.coveredHours / today.scheduledHours, 1) * 100}%`,
                  }}
                />
              </div>
            )}
          </div>
          <div className="flex items-baseline justify-between gap-4 text-muted-foreground">
            <span>If you stop now</span>
            <span className="time-display">{formatSignedHours(nowHours)}</span>
          </div>
        </>
      )}
    </div>
  );
}

function BreakdownRow(props: {
  swatchStyle: React.CSSProperties;
  label: string;
  hours: number;
}) {
  return (
    <div className="flex items-baseline justify-between gap-4">
      <span className="flex items-center gap-2">
        <span
          aria-hidden
          className="size-2.5 shrink-0 rounded-[3px]"
          style={props.swatchStyle}
        />
        {props.label}
      </span>
      <span className={cn("time-display", signedHoursColor(props.hours))}>
        {formatSignedHours(props.hours)}
      </span>
    </div>
  );
}
