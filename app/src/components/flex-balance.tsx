import dayjs from "dayjs";
import type { FlexBalance } from "@/lib/api/queries/time-tracking";
import { formatSignedHours, signedHoursColor } from "@/lib/flex-balance";
import { cn } from "@/lib/utils";

const SETTLED_SWATCH_STYLE = {
  background: "hsl(var(--primary))",
} as const;

const ESTIMATED_SWATCH_STYLE = {
  background:
    "repeating-linear-gradient(135deg, hsl(var(--primary) / 0.55) 0 2px, transparent 2px 5px)",
  boxShadow: "inset 0 0 0 1px hsl(var(--primary) / 0.5)",
} as const;

/**
 * The flex balance written out as a sum: what the provider has settled, the
 * estimate for the days after it, and optionally their total.
 */
export function FlexBalanceBreakdown(props: {
  balance: FlexBalance;
  /** Hours not booked yet, such as a running timer, added to the estimate. */
  unbookedHours?: number;
  showTotal?: boolean;
  className?: string;
}) {
  const unbookedHours = props.unbookedHours ?? 0;
  const estimatedHours = props.balance.estimatedHours + unbookedHours;
  const totalHours = props.balance.balanceHours + unbookedHours;
  const settledThrough = dayjs(props.balance.settledThrough);
  const estimateFrom = settledThrough.add(1, "day");
  const estimateLabel =
    estimateFrom.date() === 1 && estimateFrom.isSame(dayjs(), "month")
      ? `${estimateFrom.format("MMMM")} so far`
      : `Since ${estimateFrom.format("MMM D")}`;

  return (
    <div className={cn("flex flex-col gap-1", props.className)}>
      <BreakdownRow
        swatchStyle={SETTLED_SWATCH_STYLE}
        label={`Settled in Kleer, ${settledThrough.format("MMM D")}`}
        hours={props.balance.settledHours}
      />
      <BreakdownRow
        swatchStyle={ESTIMATED_SWATCH_STYLE}
        label={unbookedHours ? `${estimateLabel}, with timer` : estimateLabel}
        hours={estimatedHours}
      />
      {props.showTotal && (
        <div className="mt-1 flex items-baseline justify-between gap-4 border-t border-border/50 pt-2 font-semibold">
          <span>Preliminary total</span>
          <span
            className={cn("time-display text-xl", signedHoursColor(totalHours))}
          >
            {formatSignedHours(totalHours)}
          </span>
        </div>
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
