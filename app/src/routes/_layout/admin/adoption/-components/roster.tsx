import * as React from "react";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import {
  companyCostByDay,
  daysOf,
  isWorkDay,
  VOLUME_STEPS_USD,
  volumeStep,
  type DayIndex,
  type DayRange,
  type PersonStats,
} from "../../-lib/adoption";
import { formatEstimateAmount } from "../../-lib/estimates";
import { shortDate, weekdayDate } from "../../-lib/format";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { formatKnownCost } from "./format";
import { VOLUME_STEP_COLORS } from "./colors";
import { SectionHead } from "./segmented";

/**
 * Everyone, one row each and one square per day of the window, shaded by how
 * much AI was used that day. The day columns share the width the name and
 * cost columns leave. Most cost first; people without any use in
 * the window last. Selecting a name opens their page.
 */
export function Roster({
  adoption,
  people,
  index,
  range,
  onSelect,
}: {
  adoption: AiAdoption;
  people: readonly PersonStats[];
  index: DayIndex;
  range: DayRange;
  onSelect: (userId: number) => void;
}) {
  const money = useEstimateMoney();
  const days = React.useMemo(() => daysOf(range), [range]);
  const companyCosts = React.useMemo(
    () => companyCostByDay(adoption, range),
    [adoption, range],
  );
  const maxCompany = Math.max(0, ...companyCosts.values());
  const used = people.filter((stats) => stats.activeDays > 0);
  const unseen = people.filter((stats) => stats.activeDays === 0);
  const weeks = Math.round(days.length / 7);

  return (
    <section aria-labelledby="adoption-roster" className="flex flex-col gap-3">
      <SectionHead
        id="adoption-roster"
        title="Everyone, day by day"
        description="One row per person, most AI first, one square per day, shaded by how much AI was used that day. Select a name to see their page."
      >
        <RampLegend />
      </SectionHead>
      <div className="overflow-x-auto rounded-xl border border-border/60 bg-card">
        <table className="w-full border-separate border-spacing-0 text-[13px]">
          <caption className="sr-only">
            Each person's AI per day over the last {weeks} weeks, with their
            cost.
          </caption>
          <thead>
            <tr>
              <th
                scope="col"
                className="sticky left-0 z-10 w-px bg-card px-4 pt-3 pb-1.5 text-left align-bottom text-xs font-semibold whitespace-nowrap text-muted-foreground"
              >
                Company, per day
              </th>
              {days.map((day) => {
                const cost = companyCosts.get(day) ?? 0;
                return (
                  <th
                    key={day}
                    scope="col"
                    className={cn(
                      "h-14 p-0 pb-1.5 align-bottom",
                      day > range.from && isMonday(day) && "pl-1",
                    )}
                    title={`${weekdayDate(day)}: ${formatEstimateAmount(cost, money)}`}
                  >
                    <span className="sr-only">{weekdayDate(day)}</span>
                    <span
                      aria-hidden
                      className="mx-px block rounded-t-sm bg-[var(--viz-seq-2)]"
                      style={{
                        height:
                          maxCompany > 0
                            ? `${Math.max(cost > 0 ? 1 : 0, (cost / maxCompany) * 32)}px`
                            : 0,
                      }}
                    />
                  </th>
                );
              })}
              <th
                scope="col"
                className="w-px px-4 pb-1.5 text-right align-bottom text-xs font-semibold whitespace-nowrap text-muted-foreground"
              >
                {weeks} weeks
              </th>
            </tr>
            <tr aria-hidden>
              <th className="sticky left-0 z-10 bg-card" />
              {days.map((day) => (
                <th
                  key={day}
                  className={cn(
                    "h-5 p-0 text-left align-top text-[11px] font-medium whitespace-nowrap text-muted-foreground",
                    day > range.from && isMonday(day) && "pl-1",
                  )}
                >
                  {isMonday(day) || day === range.from ? (
                    <span className="block w-0 overflow-visible">
                      {shortDate(day)}
                    </span>
                  ) : null}
                </th>
              ))}
              <th />
            </tr>
          </thead>
          <tbody>
            <GroupRow
              colSpan={days.length + 2}
              title={`Used AI, ${used.length}`}
              note={`Most AI in the last ${weeks} weeks first`}
            />
            {used.map((stats) => (
              <PersonRow
                key={stats.person.userId}
                stats={stats}
                days={days}
                range={range}
                index={index}
                onSelect={onSelect}
              />
            ))}
            {unseen.length > 0 ? (
              <>
                <GroupRow
                  colSpan={days.length + 2}
                  title={`Not seen, ${unseen.length}`}
                  note="No AI use reported. They may use AI Toki cannot see."
                />
                {unseen.map((stats) => (
                  <PersonRow
                    key={stats.person.userId}
                    stats={stats}
                    days={days}
                    range={range}
                    index={index}
                    onSelect={onSelect}
                  />
                ))}
              </>
            ) : null}
          </tbody>
        </table>
        <p className="border-t border-border/60 px-4 py-3 text-xs text-muted-foreground">
          Narrow columns are weekends. Days before someone's first Token Ledger
          sync are read back from the logs on their machine.
        </p>
      </div>
    </section>
  );
}

function GroupRow({
  colSpan,
  title,
  note,
}: {
  colSpan: number;
  title: string;
  note: string;
}) {
  return (
    <tr>
      <th
        scope="colgroup"
        colSpan={colSpan}
        className="px-4 pt-4 pb-1.5 text-left text-[13px] font-bold"
      >
        <span className="sticky left-4">
          {title}
          <span className="ml-2 font-medium text-muted-foreground">{note}</span>
        </span>
      </th>
    </tr>
  );
}

function PersonRow({
  stats,
  days,
  range,
  index,
  onSelect,
}: {
  stats: PersonStats;
  days: readonly string[];
  range: DayRange;
  index: DayIndex;
  onSelect: (userId: number) => void;
}) {
  const money = useEstimateMoney();
  const byDay = index.get(stats.person.userId);
  return (
    <tr className="group">
      <th
        scope="row"
        className="sticky left-0 z-10 bg-card p-0 text-left font-semibold group-hover:bg-muted/50"
      >
        <button
          type="button"
          onClick={() => onSelect(stats.person.userId)}
          className="w-full px-4 py-[3px] text-left whitespace-nowrap hover:text-primary focus-visible:text-primary focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
        >
          {stats.person.fullName}
        </button>
      </th>
      {days.map((day) => {
        const use = byDay?.get(day);
        const weekend = !isWorkDay(day);
        return (
          <td
            key={day}
            className={cn(
              "p-0 group-hover:bg-muted/50",
              day > range.from && isMonday(day) && "pl-1",
            )}
            title={
              use
                ? `${weekdayDate(day)}: ${formatEstimateAmount(use.costUsd, money)}, ${use.tools.map((tool) => AI_PROVIDER_LABELS[tool]).join(", ")}`
                : `${weekdayDate(day)}: no AI`
            }
          >
            <span
              aria-hidden
              className={cn(
                "mx-px my-1 block h-4 rounded-[3px]",
                weekend ? "min-w-1.5" : "min-w-3",
                !use &&
                  (weekend
                    ? "bg-muted/60"
                    : "shadow-[inset_0_0_0_1px_var(--viz-grid)]"),
              )}
              style={
                use
                  ? { background: VOLUME_STEP_COLORS[volumeStep(use.costUsd)] }
                  : undefined
              }
            />
          </td>
        );
      })}
      <td className="px-3 text-right font-bold whitespace-nowrap tabular-nums group-hover:bg-muted/50">
        {stats.activeDays > 0
          ? formatKnownCost(stats.costUsd, stats.unpricedRecords, money)
          : "–"}
      </td>
    </tr>
  );
}

/** The squares' ramp, from least to most. */
function RampLegend() {
  const money = useEstimateMoney();
  const labels = [
    `Under ${formatEstimateAmount(VOLUME_STEPS_USD[0], money)}`,
    ...VOLUME_STEPS_USD.slice(1).map(
      (step, i) =>
        `${formatEstimateAmount(VOLUME_STEPS_USD[i], money)} to ${formatEstimateAmount(step, money)}`,
    ),
    `Over ${formatEstimateAmount(VOLUME_STEPS_USD.at(-1) ?? 0, money)}`,
  ];
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      <span>Less</span>
      <span className="flex gap-0.5">
        {VOLUME_STEP_COLORS.map((color, step) => (
          <span
            key={color}
            title={labels[step]}
            className="block h-3 w-4 rounded-sm"
            style={{ background: color }}
          />
        ))}
      </span>
      <span>More AI that day</span>
    </div>
  );
}

function isMonday(day: string): boolean {
  return new Date(`${day}T00:00:00Z`).getUTCDay() === 1;
}
