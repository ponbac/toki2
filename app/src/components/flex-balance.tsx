import dayjs from "dayjs";
import type { FlexBreakdown } from "@/lib/flex-breakdown";
import { formatSignedHours, signedHoursColor } from "@/lib/flex-balance";
import { cn, formatHoursMinutes } from "@/lib/utils";

/**
 * The flex balance written out as two sums: what the provider has settled and
 * the estimate up to yesterday make the balance before today, and today's
 * hours so far take it to the balance if you stop now. Only the totals are
 * coloured.
 */
export function FlexBalanceBreakdown(props: {
  breakdown: FlexBreakdown;
  /** Keeps the total at the rows' size and leaves out today's bar, for
   * tooltips. */
  compact?: boolean;
  className?: string;
}) {
  const { settled, earlier, beforeToday, today, timerHours, nowHours } =
    props.breakdown;
  // Only a provider that has settled today leaves no balance before it.
  const total = beforeToday ?? nowHours;

  return (
    <div className={cn("flex flex-col leading-6", props.className)}>
      <PartRow
        label={`Settled in Kleer, ${dayjs(settled.through).format("MMM D")}`}
        hours={settled.hours}
      />
      {earlier && (
        <PartRow
          label={
            earlier.from === earlier.through
              ? "Yesterday"
              : `${dayjs(earlier.from).format("MMM D")} to yesterday`
          }
          hours={earlier.hours}
        />
      )}
      {timerHours !== null && (
        <PartRow label="Running timer" hours={timerHours} />
      )}
      {total !== null && (
        <div className="mt-2 flex items-baseline justify-between gap-4 border-t border-border pt-2.5">
          <span className="font-semibold">
            {beforeToday === null ? "Preliminary total" : "Before today"}
          </span>
          <span
            className={cn(
              "time-display",
              !props.compact && "text-2xl leading-none tracking-tight",
              signedHoursColor(total),
            )}
          >
            {formatSignedHours(total)}
          </span>
        </div>
      )}
      {today && nowHours !== null && (
        <>
          <div className={props.compact ? "mt-1" : "mt-4"}>
            <PartRow
              label={
                <>
                  Today
                  {today.scheduledHours > 0 && (
                    <span className="time-display ml-2 text-xs text-muted-foreground">
                      {formatHoursMinutes(today.coveredHours)} of{" "}
                      {formatHoursMinutes(today.scheduledHours)}
                    </span>
                  )}
                </>
              }
              hours={today.hours}
            />
            {!props.compact && today.scheduledHours > 0 && (
              <TodayBar
                coveredHours={today.coveredHours}
                scheduledHours={today.scheduledHours}
                nowHours={nowHours}
              />
            )}
          </div>
          <div className="flex items-baseline justify-between gap-4">
            <span className="text-muted-foreground">If you stop now</span>
            <span className={cn("time-display", signedHoursColor(nowHours))}>
              {formatSignedHours(nowHours)}
            </span>
          </div>
        </>
      )}
    </div>
  );
}

function PartRow(props: { label: React.ReactNode; hours: number }) {
  return (
    <div className="flex items-baseline justify-between gap-4 text-foreground/80">
      <span>{props.label}</span>
      <span className="time-display">{formatSignedHours(props.hours)}</span>
    </div>
  );
}

/**
 * Today's hours against the schedule, with a tick where the balance crosses
 * zero: fill past it and the day ends ahead even if you stop now. Hours past
 * the schedule run on in green after a tick at the full day.
 */
function TodayBar(props: {
  coveredHours: number;
  scheduledHours: number;
  nowHours: number;
}) {
  const length = Math.max(props.scheduledHours, props.coveredHours);
  const at = (hours: number) => `${(hours / length) * 100}%`;
  // Every hour worked adds one to the balance, so it reaches zero this many
  // hours into the day.
  const zeroAt = props.coveredHours - props.nowHours;
  const showZero = zeroAt > 0 && zeroAt < length;
  const over = props.coveredHours > props.scheduledHours;

  return (
    <div aria-hidden className="mt-2">
      <div className="relative h-1.5 rounded-full bg-muted">
        <div
          className={cn(
            "absolute inset-y-0 left-0 bg-primary",
            over ? "rounded-l-full" : "rounded-full",
          )}
          style={{
            width: at(Math.min(props.coveredHours, props.scheduledHours)),
          }}
        />
        {over && (
          <>
            <div
              className="absolute inset-y-0 right-0 rounded-r-full bg-emerald-500"
              style={{ left: at(props.scheduledHours) }}
            />
            <Tick at={at(props.scheduledHours)} />
          </>
        )}
        {showZero && <Tick at={at(zeroAt)} />}
      </div>
      <div className="relative mt-1 h-3.5 font-mono text-[10.5px] leading-3.5 text-muted-foreground">
        {showZero && <TickLabel at={at(zeroAt)}>0</TickLabel>}
        {over && (
          <TickLabel at={at(props.scheduledHours)}>
            {Number(props.scheduledHours.toFixed(2))} h
          </TickLabel>
        )}
      </div>
    </div>
  );
}

function Tick(props: { at: string }) {
  return (
    <div
      className="absolute -inset-y-1 w-px bg-foreground/45"
      style={{ left: props.at }}
    />
  );
}

function TickLabel(props: { at: string; children: React.ReactNode }) {
  return (
    <span
      className="absolute top-0 -translate-x-1/2 whitespace-nowrap"
      style={{ left: props.at }}
    >
      {props.children}
    </span>
  );
}
