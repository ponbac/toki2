import type * as React from "react";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import { formatFee } from "@/lib/ai-subscriptions";
import type {
  AiBillingOverview,
  AiBillingTotals,
  AiBillingUsage,
} from "@/lib/api/queries/ai-admin";
import {
  PENDING_RATE_LABEL,
  allPending,
  formatAmount,
} from "../../-lib/billing";
import { addUsage, emptyUsage } from "../../-lib/daily";
import {
  formatEstimate,
  formatTokens,
  formatUsd,
  monthLabel,
  plural,
} from "../../-lib/format";
import { BILLING_MODE_COLORS } from "../../-lib/providers";
import { billUsdRate } from "../../-lib/subscription-value";

const percent = new Intl.NumberFormat("en-US", {
  style: "percent",
  maximumFractionDigits: 0,
});

/** A converted total, or why it is unknown: a line it includes has no rate,
 * or its rate is still being fetched. */
function converted(amount: string | null, currency: string, pending: boolean) {
  return amount === null ? (
    <span className="text-[#c98500] dark:text-[#fab219]">
      {pending ? PENDING_RATE_LABEL : "No rate"}
    </span>
  ) : (
    formatFee(amount, currency)
  );
}

/**
 * The bill's usage at API prices, put in the billing currency at the bill's own
 * dollar rate so it reads beside the total; the dollar estimate is the
 * original. Without a dollar rate on the bill it stays in dollars.
 */
function apiEquivalent(
  usage: AiBillingUsage,
  usdRate: number | null,
  currency: string,
): { value: string; original: string | null } {
  const unknown = usage.unpricedRecords > 0 && usage.apiEquivalentUsd === 0;
  if (usdRate === null || currency === "USD" || unknown) {
    return { value: formatEstimate(usage), original: null };
  }
  const amount = formatAmount(usage.apiEquivalentUsd * usdRate, currency);
  return {
    value: usage.unpricedRecords > 0 ? `${amount} + unknown` : amount,
    original: `${formatUsd(usage.apiEquivalentUsd)} at ${usdRate.toLocaleString("en-US", { maximumFractionDigits: 4 })} to the dollar`,
  };
}

/**
 * The bill as a share of what its usage would cost at API prices, where both
 * are known in full: every line converted, a dollar rate, and no unpriced
 * usage on either side.
 */
function billShare(
  totals: AiBillingTotals,
  usdRate: number | null,
): number | null {
  const billed = totals.converted.billed;
  const estimate = totals.usage.apiEquivalentUsd;
  if (
    billed === null ||
    usdRate === null ||
    estimate === 0 ||
    totals.usage.unpricedRecords > 0
  ) {
    return null;
  }
  return Number(billed) / (estimate * usdRate);
}

function formatShare(share: number): string {
  return share > 0 && share < 0.01 ? "under 1%" : percent.format(share);
}

/**
 * The month's money in one card: what is billed, beside what all its usage
 * would cost at API prices, then the bill split into API usage and
 * subscription fees with the original amounts beneath.
 */
export function BillSummary({
  month,
  totals,
  exchangeRates,
  missingRates,
  pendingRates,
  personalUsage,
}: {
  month: string;
  totals: AiBillingTotals;
  exchangeRates: AiBillingOverview["exchangeRates"];
  missingRates: readonly string[];
  pendingRates: readonly string[];
  personalUsage: AiBillingOverview["personalUsage"];
}) {
  const billing = totals.converted;
  const missingFeeRates = totals.fees
    .filter(
      (fee) => fee.billed !== "0.00" && missingRates.includes(fee.currency),
    )
    .map((fee) => fee.currency);
  const pending = (currencies: readonly string[]) =>
    currencies.length > 0 && allPending(currencies, pendingRates);
  const hasApi =
    totals.apiBilledUsd !== "0.00" || totals.apiUnpricedRecords > 0;

  const usdRate = billUsdRate(exchangeRates, billing.currency);
  const estimate = apiEquivalent(totals.usage, usdRate, billing.currency);
  const share = billShare(totals, usdRate);
  const tokens = totals.usage.tokens;
  const api = Number(billing.api ?? 0);
  const fees = Number(billing.fees ?? 0);
  const split = billing.api !== null && billing.fees !== null && api + fees > 0;
  const personal = personalUsage.reduce(
    (sum, entry) => addUsage(sum, entry.usage),
    emptyUsage(),
  );

  return (
    <section
      aria-label={`${monthLabel(month)} in total`}
      className="flex flex-col gap-4 rounded-xl border border-border/60 bg-card/60 p-5"
    >
      <dl className="flex flex-wrap gap-x-8 gap-y-4">
        <div className="flex flex-1 basis-44 flex-col gap-1">
          <dt className="text-xs font-medium text-muted-foreground">
            Total billed in {monthLabel(month)}
          </dt>
          <dd className="whitespace-nowrap text-3xl font-semibold tabular-nums leading-9 tracking-tight">
            {converted(billing.billed, billing.currency, pending(missingRates))}
          </dd>
          <Note>
            Every line converted to {billing.currency}, as in the CSV
            {totals.apiUnpricedRecords > 0 &&
              "; unpriced usage is not included"}
          </Note>
        </div>
        <div className="flex flex-1 basis-44 flex-col gap-1">
          <dt className="text-xs font-medium text-muted-foreground">
            At API prices
          </dt>
          <dd className="whitespace-nowrap text-2xl font-semibold tabular-nums leading-9 tracking-tight">
            {estimate.value}
          </dd>
          <Note>
            {estimate.original !== null && `${estimate.original}. `}
            What the usage on this bill would cost at API prices, unrounded
          </Note>
          <Note>
            {formatTokens(tokens.total)} tokens: {formatTokens(tokens.input)}{" "}
            input, {formatTokens(tokens.cacheRead)} cache read,{" "}
            {formatTokens(tokens.output)} output
          </Note>
        </div>
      </dl>

      {share !== null && (
        <p className="text-sm">
          The bill comes to{" "}
          <span className="font-semibold tabular-nums">
            {formatShare(share)}
          </span>{" "}
          of what its usage would cost at API prices.
        </p>
      )}

      <div className="flex flex-col gap-2.5">
        <div
          aria-hidden
          className="flex h-2 gap-0.5 overflow-hidden rounded-full bg-muted"
        >
          {split && api > 0 && (
            <span
              style={{
                flexGrow: api,
                backgroundColor: BILLING_MODE_COLORS.api,
              }}
            />
          )}
          {split && fees > 0 && (
            <span
              style={{
                flexGrow: fees,
                backgroundColor: BILLING_MODE_COLORS.subscription,
              }}
            />
          )}
        </div>
        <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
          <Part
            color={BILLING_MODE_COLORS.api}
            label="API usage"
            value={
              hasApi
                ? converted(
                    billing.api,
                    billing.currency,
                    pendingRates.includes("USD"),
                  )
                : "—"
            }
          >
            <Note>
              {formatFee(totals.apiBilledUsd, "USD")}: each API line&apos;s
              estimate rounded to whole cents
              {totals.apiUnpricedRecords > 0 &&
                `; plus ${plural(totals.apiUnpricedRecords, "unpriced record")} of unknown cost`}
            </Note>
          </Part>
          <Part
            color={BILLING_MODE_COLORS.subscription}
            label="Subscription fees"
            value={
              totals.fees.length === 0
                ? "—"
                : converted(
                    billing.fees,
                    billing.currency,
                    pending(missingFeeRates),
                  )
            }
          >
            {totals.fees.map((fee) => (
              <Note key={fee.currency}>
                {formatFee(fee.billed, fee.currency)}
                {fee.overhead !== "0.00" &&
                  ` incl. ${formatFee(fee.overhead, fee.currency)} unallocated overhead`}
              </Note>
            ))}
          </Part>
        </dl>
      </div>

      {personalUsage.length > 0 && (
        <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
          {PERSONAL_LABEL} usage left out of the bill:{" "}
          <span className="tabular-nums">{formatEstimate(personal)}</span> at
          API prices, for {plural(personalUsage.length, "developer")}.
        </p>
      )}
    </section>
  );
}

/** One part of the bill: its swatch, name and amount, with notes beneath. */
function Part({
  color,
  label,
  value,
  children,
}: {
  color: string;
  label: string;
  value: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <span
          aria-hidden
          className="size-2 shrink-0 rounded-[2px]"
          style={{ backgroundColor: color }}
        />
        {label}
      </dt>
      <dd className="text-sm font-semibold tabular-nums">{value}</dd>
      {children}
    </div>
  );
}

function Note({ children }: { children: React.ReactNode }) {
  return <dd className="text-xs text-muted-foreground">{children}</dd>;
}
