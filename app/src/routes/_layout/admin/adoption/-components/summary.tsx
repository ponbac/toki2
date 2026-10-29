import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import {
  amountOf,
  companyTotals,
  covers,
  daysOf,
  isWorkDay,
  median,
  perActiveDayOf,
  previousWindow,
  topShare,
  type DayRange,
  type PersonStats,
  type VolumeMeasure,
} from "../../-lib/adoption";
import { formatAmount, formatKnownCost, formatTokenCount } from "./format";
import { formatCount, formatShare } from "../../-lib/format";
import { useEstimateMoney } from "../../-lib/use-estimate-money";

/** The window in a sentence, with the figures that frame it, in `measure`. */
export function Summary({
  adoption,
  people,
  range,
  measure,
}: {
  adoption: AiAdoption;
  people: readonly PersonStats[];
  range: DayRange;
  measure: VolumeMeasure;
}) {
  const money = useEstimateMoney();
  const weeks = Math.round(daysOf(range).length / 7);
  const totals = companyTotals(adoption, range);
  const before = previousWindow(range);
  const previous = covers(adoption, before)
    ? companyTotals(adoption, before)
    : null;
  // Tokens only count people Token Ledger saw: GitHub reports Copilot in
  // credits, so someone on Copilot alone has no tokens.
  const users = people.filter((stats) =>
    measure === "cost" ? stats.activeDays > 0 : stats.tokens > 0,
  );
  const workDays = daysOf(range).filter(isWorkDay).length;
  const total = amountOf(totals, measure);
  // Costs are known subtotals while usage is unpriced: a window whose
  // usage has no known amount at all has nothing to compare.
  const change =
    previous && amountOf(previous, measure) > 0 && total > 0
      ? total / amountOf(previous, measure) - 1
      : null;
  const format = (value: number) => formatAmount(value, measure, money);

  const facts: Array<[string, string]> = [
    [
      format(workDays > 0 ? total / workDays : 0),
      "per work day across the company",
    ],
    [
      formatShare(
        topShare(
          users.map((stats) => amountOf(stats, measure)),
          3,
          total,
        ),
      ),
      `of the ${measure === "cost" ? "cost" : "tokens"} is the three largest users'`,
    ],
    [
      format(median(users.map((stats) => amountOf(stats, measure)))),
      `median per person over ${weeks} weeks, of the ${users.length} ${measure === "cost" ? "who used AI" : "with token data"}`,
    ],
    [
      format(median(users.map((stats) => perActiveDayOf(stats, measure)))),
      "median per person on a day they use AI",
    ],
  ];
  const comparison =
    change === null ? null : (
      <>
        ,{" "}
        <b className="font-extrabold tabular-nums">
          {formatShare(Math.abs(change))}
        </b>{" "}
        {change >= 0 ? "more" : "less"} than the {weeks} weeks before
      </>
    );

  return (
    <section aria-label="Summary" className="flex flex-col gap-6">
      {measure === "cost" ? (
        <p className="max-w-[34ch] text-xl leading-normal">
          The company used{" "}
          <b className="font-extrabold tabular-nums">
            {formatKnownCost(totals.costUsd, totals.unpricedRecords, money)}
          </b>{" "}
          of AI at API prices in the last {weeks} weeks{comparison}. Models read
          and wrote{" "}
          <b className="font-extrabold tabular-nums">
            {formatTokenCount(totals.tokens)}
          </b>{" "}
          tokens.
        </p>
      ) : (
        <p className="max-w-[34ch] text-xl leading-normal">
          Models read and wrote{" "}
          <b className="font-extrabold tabular-nums">
            {formatTokenCount(totals.tokens)}
          </b>{" "}
          tokens in the last {weeks} weeks{comparison}. The company's AI cost{" "}
          <b className="font-extrabold tabular-nums">
            {formatKnownCost(totals.costUsd, totals.unpricedRecords, money)}
          </b>{" "}
          at API prices over the same weeks.
        </p>
      )}
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
      {measure === "cost" && totals.unpricedRecords > 0 ? (
        <p className="max-w-md text-xs text-muted-foreground">
          Costs leave out {formatCount(totals.unpricedRecords)}{" "}
          {totals.unpricedRecords === 1 ? "request" : "requests"} to models
          without an API price.
        </p>
      ) : null}
    </section>
  );
}
