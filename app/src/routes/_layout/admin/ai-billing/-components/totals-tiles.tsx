import type { AiBillingTotals } from "@/lib/api/queries/ai-admin";
import { allPending, PENDING_RATE_LABEL } from "../../-lib/billing";
import { formatEstimate, formatFee, formatTokens } from "../../-lib/format";

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

/** The month's headline figures: totals in the billing currency first, with
 * the original amounts in their own currencies beneath. */
export function TotalsTiles({
  totals,
  missingRates,
  pendingRates,
}: {
  totals: AiBillingTotals;
  missingRates: readonly string[];
  pendingRates: readonly string[];
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
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
      <Tile label={`Total billed (${billing.currency})`}>
        <Value>
          {converted(billing.billed, billing.currency, pending(missingRates))}
        </Value>
        <Note>
          the sum of every line converted to {billing.currency}, as in the CSV
          {totals.apiUnpricedRecords > 0 && "; unpriced usage is not included"}
        </Note>
      </Tile>
      <Tile label="Subscription fees">
        {totals.fees.length === 0 ? (
          <Value>—</Value>
        ) : (
          <>
            <Value>
              {converted(
                billing.fees,
                billing.currency,
                pending(missingFeeRates),
              )}
            </Value>
            {totals.fees.map((fee) => (
              <Note key={fee.currency}>
                {formatFee(fee.billed, fee.currency)}
                {fee.overhead !== "0.00" &&
                  ` incl. ${formatFee(fee.overhead, fee.currency)} unallocated overhead`}
              </Note>
            ))}
          </>
        )}
      </Tile>
      <Tile label="API usage billed">
        <Value>
          {hasApi
            ? converted(
                billing.api,
                billing.currency,
                pendingRates.includes("USD"),
              )
            : "—"}
        </Value>
        <Note>
          {formatFee(totals.apiBilledUsd, "USD")}: each API line&apos;s estimate
          rounded to whole cents
          {totals.apiUnpricedRecords > 0 &&
            `; plus ${totals.apiUnpricedRecords} unpriced records of unknown cost`}
        </Note>
      </Tile>
      <Tile label="API-equivalent estimate, all usage">
        <Value>{formatEstimate(totals.usage)}</Value>
        <Note>unrounded; what all usage would cost at API prices</Note>
      </Tile>
      <Tile label="Tokens">
        <Value>{formatTokens(totals.usage.tokens.total)}</Value>
        <Note>
          {formatTokens(totals.usage.tokens.input)} input ·{" "}
          {formatTokens(totals.usage.tokens.cacheRead)} cache read ·{" "}
          {formatTokens(totals.usage.tokens.output)} output
        </Note>
      </Tile>
    </div>
  );
}

function Tile({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1 rounded-xl border border-border/60 bg-card/60 p-4">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

function Value({ children }: { children: React.ReactNode }) {
  return <div className="text-xl font-semibold">{children}</div>;
}

function Note({ children }: { children: React.ReactNode }) {
  return <p className="text-xs text-muted-foreground">{children}</p>;
}
