import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import {
  companyTotals,
  covers,
  daysOf,
  isWorkDay,
  median,
  previousWindow,
  topShare,
  type DayRange,
  type PersonStats,
} from "../../-lib/adoption";
import { formatCost, formatKnownCost } from "./format";
import { formatShare, formatTokens } from "../../-lib/format";
import { useEstimateMoney } from "../../-lib/use-estimate-money";

/** The window in a sentence, with the figures that frame it. */
export function Summary({
  adoption,
  people,
  range,
}: {
  adoption: AiAdoption;
  people: readonly PersonStats[];
  range: DayRange;
}) {
  const money = useEstimateMoney();
  const weeks = Math.round(daysOf(range).length / 7);
  const totals = companyTotals(adoption, range);
  const before = previousWindow(range);
  const previous = covers(adoption, before)
    ? companyTotals(adoption, before)
    : null;
  const users = people.filter((stats) => stats.activeDays > 0);
  const workDays = daysOf(range).filter(isWorkDay).length;
  const change =
    previous && previous.costUsd > 0
      ? totals.costUsd / previous.costUsd - 1
      : null;

  const facts: Array<[string, string]> = [
    [
      formatCost(workDays > 0 ? totals.costUsd / workDays : 0, money),
      "per work day across the company",
    ],
    [
      formatShare(
        topShare(
          users.map((stats) => stats.costUsd),
          3,
          totals.costUsd,
        ),
      ),
      "of the cost is the three largest users'",
    ],
    [
      formatCost(median(users.map((stats) => stats.costUsd)), money),
      `median per person over ${weeks} weeks, of the ${users.length} who used AI`,
    ],
    [
      formatCost(median(users.map((stats) => stats.perActiveDayUsd)), money),
      "median per person on a day they use AI",
    ],
  ];

  return (
    <section aria-label="Summary" className="flex flex-col gap-6">
      <p className="max-w-[34ch] text-xl leading-normal">
        The company used{" "}
        <b className="font-extrabold tabular-nums">
          {formatKnownCost(totals.costUsd, totals.unpricedRecords, money)}
        </b>{" "}
        of AI at API prices in the last {weeks} weeks
        {change === null ? (
          "."
        ) : (
          <>
            ,{" "}
            <b className="font-extrabold tabular-nums">
              {formatShare(Math.abs(change))}
            </b>{" "}
            {change >= 0 ? "more" : "less"} than the {weeks} weeks before.
          </>
        )}{" "}
        Models read and wrote{" "}
        <b className="font-extrabold tabular-nums">
          {formatTokens(totals.tokens)}
        </b>{" "}
        tokens.
      </p>
      <dl className="grid max-w-md grid-cols-2 gap-x-6 gap-y-4">
        {facts.map(([value, label]) => (
          <div key={label} className="flex flex-col-reverse">
            <dt className="text-[13px] leading-snug text-muted-foreground">
              {label}
            </dt>
            <dd className="text-xl font-bold tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
