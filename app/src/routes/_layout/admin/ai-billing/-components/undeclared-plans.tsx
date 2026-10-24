import { Link } from "@tanstack/react-router";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type { AiBillingOverview } from "@/lib/api/queries/ai-admin";
import type { AiSubscriptionMismatch } from "@/lib/api/queries/ai-usage";
import { billableTotals, formatConverted } from "../../-lib/billing";
import { plural, shortDate } from "../../-lib/format";

/**
 * Paid plans the tools reported in the month that nobody declared. Until one
 * is declared, the usage it paid for bills at API prices: for a heavy user,
 * many times the plan's fee. Declaring happens on the subscriptions page.
 */
export function UndeclaredPlans({
  overview,
  mismatches,
}: {
  overview: AiBillingOverview;
  mismatches: readonly AiSubscriptionMismatch[];
}) {
  if (mismatches.length === 0) {
    return null;
  }
  const names = new Map(overview.developers.map((d) => [d.userId, d.fullName]));
  const rows = mismatches
    .map((mismatch) => {
      const lines = overview.lines.filter(
        (line) =>
          line.userId === mismatch.userId &&
          line.provider === mismatch.provider &&
          line.billingMode === "api",
      );
      const totals = billableTotals(lines);
      return { mismatch, lines, totals };
    })
    .sort((a, b) => (b.totals.converted ?? 0) - (a.totals.converted ?? 0));

  return (
    <section
      aria-labelledby="undeclared-heading"
      className="rounded-xl border border-[#c98500]/40 bg-[#c98500]/[0.06] p-4 dark:border-[#fab219]/40 dark:bg-[#fab219]/[0.06]"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex max-w-[70ch] gap-2.5">
          <AlertTriangle
            className="mt-0.5 size-4 shrink-0 text-[#c98500] dark:text-[#fab219]"
            aria-hidden
          />
          <div>
            <h3 id="undeclared-heading" className="text-sm font-semibold">
              Plans nobody declared
            </h3>
            <p className="text-sm text-muted-foreground">
              These tools reported a paid plan, but no subscription is declared
              for those days, so their usage bills at API prices instead of a
              share of the plan&apos;s fee.
            </p>
          </div>
        </div>
        <Button asChild variant="outline" size="sm" className="shrink-0">
          <Link to="/admin/ai-subscriptions">Declare subscriptions</Link>
        </Button>
      </div>
      <ul className="mt-3 divide-y divide-border/60 pl-6.5">
        {rows.map(({ mismatch, lines, totals }) => (
          <li
            key={`${mismatch.userId}-${mismatch.provider}-${mismatch.firstDay}`}
            className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-0.5 py-2 text-sm"
          >
            <span className="flex min-w-0 flex-wrap items-center gap-x-1.5">
              <span className="font-medium">
                {names.get(mismatch.userId) ?? `User ${mismatch.userId}`}
              </span>
              <span className="inline-flex items-center gap-1 text-muted-foreground">
                <AiProviderIcon provider={mismatch.provider} />
                {AI_PROVIDER_LABELS[mismatch.provider]} reported{" "}
                {mismatch.plans.join(", ")}{" "}
                {mismatch.firstDay === mismatch.lastDay
                  ? `on ${shortDate(mismatch.firstDay)}`
                  : `on ${plural(mismatch.days, "day")} from ${shortDate(mismatch.firstDay)} to ${shortDate(mismatch.lastDay)}`}
              </span>
            </span>
            {lines.length > 0 && (
              <span className="whitespace-nowrap tabular-nums">
                <span className="font-semibold">
                  {formatConverted(
                    totals,
                    overview.billingCurrency,
                    overview.pendingRates,
                  )}
                </span>{" "}
                <span className="text-muted-foreground">at API prices</span>
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
