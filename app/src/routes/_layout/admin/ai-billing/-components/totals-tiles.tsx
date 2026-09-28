import type { AiBillingTotals } from "@/lib/api/queries/ai-admin";
import { formatEstimate, formatFee, formatTokens } from "../../-lib/format";

/** The month's headline figures. Fees stay in their own currencies. */
export function TotalsTiles({ totals }: { totals: AiBillingTotals }) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      <Tile label="Subscription fees">
        {totals.fees.length === 0 ? (
          <Value>—</Value>
        ) : (
          totals.fees.map((fee) => (
            <div key={fee.currency}>
              <Value>{formatFee(fee.billed, fee.currency)}</Value>
              {fee.overhead !== "0.00" && (
                <Note>
                  incl. {formatFee(fee.overhead, fee.currency)} unallocated
                  overhead
                </Note>
              )}
            </div>
          ))
        )}
      </Tile>
      <Tile label="API usage billed">
        <Value>{formatFee(totals.apiBilledUsd, "USD")}</Value>
        <Note>
          each API line&apos;s estimate rounded to whole cents, as in the CSV
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
