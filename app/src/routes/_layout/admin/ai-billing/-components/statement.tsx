import type * as React from "react";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import { formatFee } from "@/lib/ai-subscriptions";
import type {
  AiBillingOverview,
  AiBillingUsage,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import {
  PENDING_RATE_LABEL,
  allPending,
  formatAmount,
  formatHundredths,
} from "../../-lib/billing";
import { addUsage, emptyUsage } from "../../-lib/daily";
import {
  formatEstimate,
  formatShare,
  monthLabel,
  plural,
} from "../../-lib/format";
import {
  shareOf,
  type AllocationSegment,
  type Ledger,
} from "../../-lib/ledger";
import { billUsdRate } from "../../-lib/subscription-value";
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

/** What all the usage on the bill would cost at API prices, put in the
 * billing currency at the bill's own dollar rate where it has one. */
function atApiPrices(
  usage: AiBillingUsage,
  usdRate: number | null,
  currency: string,
): string {
  const unknown = usage.unpricedRecords > 0 && usage.apiEquivalentUsd === 0;
  if (usdRate === null || currency === "USD" || unknown) {
    return formatEstimate(usage);
  }
  const amount = formatAmount(usage.apiEquivalentUsd * usdRate, currency);
  return usage.unpricedRecords > 0 ? `${amount} + unknown` : amount;
}

/**
 * The month's bill as a statement: what it comes to, how much of it goes to
 * projects that can be invoiced and how much has no project yet, drawn as one
 * strip, with the bill split into API usage and subscription fees beside it.
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
  const usdRate = billUsdRate(overview.exchangeRates, currency);
  const billed = converted.billed;
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
          {billed === null ? (
            <p className="text-3xl font-semibold">
              {unconverted(overview.missingRates)}
            </p>
          ) : (
            <Headline amount={formatFee(billed, currency)} />
          )}
          <p className="max-w-[48ch] text-sm text-muted-foreground">
            <AllocationSentence ledger={ledger} currency={currency} />
          </p>
        </div>
        <dl className="grid grid-cols-2 gap-x-7 gap-y-4 sm:grid-cols-3">
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
              hasApi
                ? `${formatFee(totals.apiBilledUsd, "USD")}${totals.apiUnpricedRecords > 0 ? " + unknown" : ""}`
                : "Nothing billed by usage"
            }
          />
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
          <Figure
            label="At API prices"
            value={atApiPrices(totals.usage, usdRate, currency)}
            note={
              billShare === null
                ? usdRate !== null && currency !== "USD"
                  ? formatEstimate(totals.usage)
                  : "What the usage would cost by the token"
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

      {overview.personalUsage.length > 0 && (
        <p className="text-xs text-muted-foreground">
          Left out of the bill as {PERSONAL_LABEL.toLowerCase()}:{" "}
          <span className="tabular-nums">{formatEstimate(personal)}</span> at
          API prices, from {plural(overview.personalUsage.length, "developer")}.
          {usdRate !== null &&
            currency !== "USD" &&
            ` Dollar estimates convert at ${usdRate.toLocaleString("en-US", { maximumFractionDigits: 4 })} ${currency}.`}
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
  const unassigned = ledger.unassigned;
  const overhead = ledger.overhead;
  const amount = (value: number) => (
    <span className="font-semibold text-foreground tabular-nums">
      {formatHundredths(value, currency)}
    </span>
  );

  if (assigned === null) {
    return <>A line without an exchange rate leaves the split unknown.</>;
  }
  const unassignedShare = unassigned ? shareOf(unassigned, ledger) : null;
  return (
    <>
      {projects.length === 0 ? (
        "No project can be invoiced yet."
      ) : (
        <>
          {amount(assigned)} goes to {plural(projects.length, "project")}.
        </>
      )}
      {unassigned && unassigned.totals.converted !== null && (
        <>
          {" "}
          {amount(unassigned.totals.converted)}
          {unassignedShare !== null && `, ${formatShare(unassignedShare)},`} has
          no project yet.
        </>
      )}
      {overhead && overhead.totals.converted !== null && (
        <> {amount(overhead.totals.converted)} of fees had no usage to bill.</>
      )}
    </>
  );
}

/** One figure beside the headline, with a short note beneath. */
function Figure({
  label,
  value,
  note,
  muted = false,
}: {
  label: string;
  value: React.ReactNode;
  note: string;
  muted?: boolean;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
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
