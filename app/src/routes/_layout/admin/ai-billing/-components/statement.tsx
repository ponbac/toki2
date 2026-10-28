import * as React from "react";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import { formatFee } from "@/lib/ai-subscriptions";
import type { AiBillingOverview } from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import {
  PENDING_RATE_LABEL,
  adjustedMonthsOf,
  allPending,
  formatHundredths,
  formatSignedHundredths,
  hundredths,
  meteredName,
  usdAsCredits,
} from "../../-lib/billing";
import { addUsage, emptyUsage } from "../../-lib/daily";
import { formatShare, listOf, monthLabel, plural } from "../../-lib/format";
import {
  spinitShare,
  statementUnassigned,
  type AllocationSegment,
  type Ledger,
  type LedgerEntry,
} from "../../-lib/ledger";
import { AttentionPill } from "../../-components/attention-pill";
import { estimateRateNote, formatEstimateIn } from "../../-lib/estimates";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { AllocationStrip } from "./allocation-strip";

/** An exact amount with its cents and currency set apart, for the headline. */
function Headline({ amount }: { amount: string }) {
  const parts = /^(.*?)([.,]\d{2} [A-Z]{3})$/.exec(amount);
  return (
    <p className="text-[2.75rem] leading-none font-semibold tracking-[-0.02em] whitespace-nowrap tabular-nums">
      {parts ? (
        <>
          {parts[1]}
          <span className="text-[1.625rem] font-medium tracking-normal text-muted-foreground">
            {parts[2]}
          </span>
        </>
      ) : (
        amount
      )}
    </p>
  );
}

/**
 * The month's bill as a statement: what it invoices (its own lines plus the
 * adjustments to closed months posted on it), how much of its own lines goes
 * to projects that can be invoiced, how much has no project yet and how much
 * stays with Spinit, drawn as one strip, with the bill split into API usage,
 * subscription fees, a metered payer's credits and adjustments beside it.
 */
export function Statement({
  overview,
  ledger,
  segments,
  highlighted,
  onHighlight,
  onSelect,
}: {
  overview: AiBillingOverview;
  ledger: Ledger;
  /** The ledger as the strip draws it. */
  segments: readonly AllocationSegment[];
  highlighted: string | null;
  onHighlight: (key: string | null) => void;
  onSelect: (segment: AllocationSegment) => void;
}) {
  const { totals, billingCurrency: currency, pendingRates } = overview;
  const converted = totals.converted;
  const pending = (currencies: readonly string[]) =>
    currencies.length > 0 && allPending(currencies, pendingRates);
  const unconverted = (currencies: readonly string[]) => (
    <span className="text-[#c98500] dark:text-[#fab219]">
      {pending(currencies) ? PENDING_RATE_LABEL : "No rate"}
    </span>
  );
  const missingFeeRates = totals.fees
    .filter(
      (fee) =>
        fee.billed !== "0.00" && overview.missingRates.includes(fee.currency),
    )
    .map((fee) => fee.currency);
  const money = useEstimateMoney();
  // Units of the billing currency per dollar, where the page knows them.
  const usdRate =
    money.kind === "converted" ? money.perUsd : currency === "USD" ? 1 : null;
  const rateNote = estimateRateNote(money);
  const billed = converted.billed;
  // What the month invoices: its own lines and its adjustments.
  const invoiced = converted.invoiced;
  const adjustmentLines = overview.adjustmentLines;
  const adjustedMonths = adjustedMonthsOf(adjustmentLines).map(monthLabel);
  const adjustmentMissing = [
    ...new Set(
      adjustmentLines
        .filter((line) => line.missingRate)
        .map((line) => line.billableCurrency),
    ),
  ];
  const billShare =
    billed !== null &&
    usdRate !== null &&
    totals.usage.apiEquivalentUsd > 0 &&
    totals.usage.unpricedRecords === 0
      ? Number(billed) / (totals.usage.apiEquivalentUsd * usdRate)
      : null;
  const personal = overview.personalUsage.reduce(
    (sum, entry) => addUsage(sum, entry.usage),
    emptyUsage(),
  );
  const hasApi =
    totals.apiBilledUsd !== "0.00" || totals.apiUnpricedRecords > 0;
  const metered = overview.metered;
  // A credit is a cent: the metered lines' cents are their credits.
  const credits = usdAsCredits(totals.meteredBilledUsd);
  const unbilled = metered
    ? [metered.unbilled.notImported, metered.unbilled.trackedUnlinked].reduce(
        (sum, usage) => addUsage(sum, usage),
        emptyUsage(),
      )
    : null;

  return (
    <section
      aria-labelledby="statement-heading"
      className="flex flex-col gap-5"
    >
      <div className="flex flex-wrap items-end justify-between gap-x-10 gap-y-6">
        <div className="flex min-w-0 flex-col gap-2.5">
          <h2
            id="statement-heading"
            className="text-sm font-medium text-muted-foreground"
          >
            AI bill for {monthLabel(overview.month)}
          </h2>
          {invoiced === null ? (
            <p className="text-3xl font-semibold">
              {unconverted([...overview.missingRates, ...adjustmentMissing])}
            </p>
          ) : (
            <Headline amount={formatFee(invoiced, currency)} />
          )}
          {adjustmentLines.length > 0 && (
            <p className="max-w-[48ch] text-sm text-muted-foreground">
              {converted.adjustments === null ? (
                <>
                  Includes adjustments to{" "}
                  {listOf(adjustedMonths, "conjunction")}, one of them without
                  an exchange rate.
                </>
              ) : hundredths(converted.adjustments) !== 0 ? (
                <>
                  Includes{" "}
                  <span className="font-semibold text-foreground tabular-nums">
                    {formatSignedHundredths(
                      hundredths(converted.adjustments),
                      currency,
                    )}
                  </span>{" "}
                  of adjustments to {listOf(adjustedMonths, "conjunction")}.
                </>
              ) : (
                <>
                  Adjustments to {listOf(adjustedMonths, "conjunction")} net to
                  nothing in {currency}.
                </>
              )}
            </p>
          )}
          <p className="max-w-[48ch] text-sm text-muted-foreground">
            <AllocationSentence ledger={ledger} currency={currency} />
          </p>
        </div>
        <dl
          className={cn(
            "grid grid-cols-2 gap-x-7 gap-y-4 sm:grid-cols-3",
            (metered || adjustmentLines.length > 0) && "lg:grid-cols-4",
            metered && adjustmentLines.length > 0 && "xl:grid-cols-5",
          )}
        >
          <Figure
            label="API usage"
            value={
              !hasApi
                ? "—"
                : converted.api === null
                  ? unconverted(["USD"])
                  : formatFee(converted.api, currency)
            }
            note={
              !hasApi
                ? "Nothing billed by usage"
                : totals.apiUnpricedRecords > 0
                  ? "Priced usage; some has no known price"
                  : "Usage no plan paid for"
            }
          />
          {metered && (
            <Figure
              label={meteredName(metered.provider)}
              value={
                hundredths(totals.meteredBilledUsd) === 0
                  ? "—"
                  : converted.metered === null
                    ? unconverted(["USD"])
                    : formatFee(converted.metered, currency)
              }
              note={`${credits} at $0.01 · UTC month`}
              badge={
                metered.provisional ? (
                  <AttentionPill title="Some of these credits may still change: the month is in progress, or GitHub's usage is not imported or settled for every day.">
                    provisional
                  </AttentionPill>
                ) : null
              }
            />
          )}
          <Figure
            label="Subscription fees"
            value={
              totals.fees.length === 0
                ? "—"
                : converted.fees === null
                  ? unconverted(missingFeeRates)
                  : formatFee(converted.fees, currency)
            }
            note={
              overview.subscriptions.length === 0
                ? "No plan covers this month"
                : plural(overview.subscriptions.length, "plan")
            }
          />
          {adjustmentLines.length > 0 && (
            <Figure
              label="Adjustments"
              value={
                converted.adjustments === null
                  ? unconverted(adjustmentMissing)
                  : formatSignedHundredths(
                      hundredths(converted.adjustments),
                      currency,
                    )
              }
              note={`${plural(adjustmentLines.length, "line")} to ${listOf(adjustedMonths, "conjunction")}`}
            />
          )}
          <Figure
            label="At API prices"
            value={formatEstimateIn(totals.usage, money)}
            note={
              billShare === null
                ? "What the usage would cost by the token"
                : `The bill is ${formatShare(billShare)} of it`
            }
            muted
          />
        </dl>
      </div>

      <AllocationStrip
        segments={segments}
        currency={currency}
        highlighted={highlighted}
        onHighlight={onHighlight}
        onSelect={onSelect}
      />

      {metered && unbilled && unbilled.records > 0 && (
        <p className="text-xs text-muted-foreground">
          {meteredName(metered.provider)} usage Toki tracked on no line yet:{" "}
          <span className="tabular-nums">
            {formatEstimateIn(unbilled, money)}
          </span>{" "}
          at API prices, on days GitHub&apos;s credits are not imported or no
          linked account covers. It bills once they are.
        </p>
      )}

      {(overview.personalUsage.length > 0 || rateNote !== null) && (
        <p className="text-xs text-muted-foreground">
          {overview.personalUsage.length > 0 && (
            <>
              Left out of the bill as {PERSONAL_LABEL.toLowerCase()}:{" "}
              <span className="tabular-nums">
                {formatEstimateIn(personal, money)}
              </span>{" "}
              at API prices, from{" "}
              {plural(overview.personalUsage.length, "developer")}.{" "}
            </>
          )}
          {rateNote !== null &&
            `API prices are in dollars, shown here at ${rateNote}.`}
        </p>
      )}
    </section>
  );
}

/** Where the bill goes, in one or two sentences. */
function AllocationSentence({
  ledger,
  currency,
}: {
  ledger: Ledger;
  currency: string;
}) {
  const projects = ledger.projects;
  const assigned = projects.reduce<number | null>(
    (sum, entry) =>
      sum === null || entry.totals.converted === null
        ? null
        : sum + entry.totals.converted,
    0,
  );
  const spinit = spinitShare(ledger);
  // A metered payer's credits outside any repository are Spinit's here, not
  // usage waiting for a project, as the strip draws them.
  const unassigned = statementUnassigned(ledger)
    .unassigned.map((entry) => ({ entry, amount: entry.totals.converted }))
    .filter(
      ({ entry, amount }) =>
        amount !== 0 ||
        // An entry of nothing but unpriced usage still has no project.
        entry.totals.unknownRecords > 0,
    );
  // Null when a kind has a line without an exchange rate.
  const unassignedTotal = unassigned.reduce<number | null>(
    (sum, { amount }) =>
      sum === null || amount === null ? null : sum + amount,
    0,
  );
  const overhead = ledger.overhead;
  const amount = (value: number) => (
    <span className="font-semibold text-foreground tabular-nums">
      {formatHundredths(value, currency)}
    </span>
  );

  if (assigned === null) {
    return <>A line without an exchange rate leaves the split unknown.</>;
  }
  const whole = ledger.totals.converted;
  const unassignedShare =
    whole === null || whole === 0 || unassignedTotal === null
      ? null
      : unassignedTotal / whole;
  const [first, second] = unassigned;
  const spinitParts = (
    [
      ["untracked Copilot", spinit.untracked],
      ["personal and internal Copilot", spinit.personal],
      ["on unlinked, external or unknown accounts", spinit.accounts],
      ["Copilot outside any repository", spinit.unattributed],
    ] satisfies [string, number | null][]
  ).flatMap(([label, value]) =>
    value !== null && value > 0 ? [{ label, value }] : [],
  );
  return (
    <>
      {projects.length === 0 ? (
        "No project can be invoiced yet."
      ) : (
        <>
          {amount(assigned)} goes to {plural(projects.length, "project")}.
        </>
      )}
      {unassigned.length > 0 && unassignedTotal === null && (
        <>
          {" "}
          Usage without a project is unknown in {currency}: a line of it has no
          exchange rate.
        </>
      )}
      {first && unassignedTotal !== null && (
        <>
          {" "}
          {amount(unassignedTotal)}
          {unassignedShare !== null && `, ${formatShare(unassignedShare)},`} has
          no project yet
          {second === undefined ? (
            <>: it {unassignedDetail(first.entry)}.</>
          ) : (
            <>
              :{" "}
              {unassigned.map(({ entry, amount: part }, index) => (
                <React.Fragment key={entry.key}>
                  {index > 0 &&
                    (index === unassigned.length - 1 ? " and " : ", ")}
                  {amount(part ?? 0)} {unassignedDetail(entry)}
                </React.Fragment>
              ))}
              .
            </>
          )}
        </>
      )}
      {overhead && overhead.totals.converted !== null && (
        <> {amount(overhead.totals.converted)} of fees had no usage to bill.</>
      )}
      {spinit.total === null && ledger.spinit.length > 0 && (
        <>
          {" "}
          What stays with Spinit is unknown in {currency}: a line of it has no
          exchange rate.
        </>
      )}
      {spinit.total !== null && spinit.total > 0 && (
        <>
          {" "}
          {amount(spinit.total)} is Spinit&apos;s
          {spinitParts.length === 1 ? (
            <> ({spinitParts[0].label}).</>
          ) : (
            <>
              :{" "}
              {spinitParts.map(({ label, value }, index) => (
                <React.Fragment key={label}>
                  {index > 0 &&
                    (index === spinitParts.length - 1 ? " and " : ", ")}
                  {amount(value)} {label}
                </React.Fragment>
              ))}
              .
            </>
          )}
        </>
      )}
    </>
  );
}

/** Where an Unassigned entry's usage comes from, and whether a mapping can
 * assign it. */
function unassignedDetail(entry: LedgerEntry): string {
  switch (entry.unassigned) {
    case "unattributed":
      return "is outside any repository, which no mapping can assign";
    case "unmappedRepository":
      return "is code review of GitHub repositories nobody has mapped";
    case "unmapped":
    case null:
      return "comes from repositories nobody has mapped";
  }
}

/** One figure beside the headline, with a short note beneath. */
function Figure({
  label,
  value,
  note,
  muted = false,
  badge = null,
}: {
  label: string;
  value: React.ReactNode;
  note: string;
  muted?: boolean;
  /** Beside the label, such as a provisional mark. */
  badge?: React.ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="flex items-center gap-1.5 text-xs text-muted-foreground">
        {label}
        {badge}
      </dt>
      <dd
        className={cn(
          "text-lg leading-snug font-semibold whitespace-nowrap tabular-nums",
          muted && "text-muted-foreground",
        )}
      >
        {value}
      </dd>
      <dd className="text-xs text-muted-foreground tabular-nums">{note}</dd>
    </div>
  );
}
