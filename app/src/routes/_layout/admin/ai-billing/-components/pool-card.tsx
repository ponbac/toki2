import { AlertTriangle, CheckCircle2, CircleDollarSign } from "lucide-react";
import type { AiMeteredPool } from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import { formatDateOf, monthLabel, shortDate } from "../../-lib/format";
import {
  formatCreditNumber,
  formatQuantity,
  formatUsdAmount,
} from "../../-lib/reconciliation";

/**
 * GitHub's month summary: the organization's Copilot credits with the pool
 * discount, seats, what GitHub bills so far, and the day the pool ran out.
 */
export function PoolCard({
  pool,
  month,
}: {
  pool: AiMeteredPool | null;
  month: string;
}) {
  if (pool === null) {
    return (
      <p className="rounded-xl border border-dashed border-border/70 px-4 py-3 text-sm text-muted-foreground">
        GitHub&apos;s summary for {monthLabel(month)} isn&apos;t imported: no
        pool or seat figures yet.
      </p>
    );
  }
  const { credits, seats } = pool;
  return (
    <section
      aria-labelledby="copilot-pool-heading"
      className="flex flex-col gap-3 rounded-xl border border-border/70 bg-card/60 p-4"
    >
      <header className="flex flex-wrap items-baseline justify-between gap-2">
        <h3
          id="copilot-pool-heading"
          className="flex items-center gap-2 text-sm font-semibold"
        >
          <CircleDollarSign
            className="size-4 text-muted-foreground"
            aria-hidden
          />
          The organization&apos;s credits and pool
        </h3>
        <span className="text-xs text-muted-foreground">
          From GitHub&apos;s summary, fetched{" "}
          <time dateTime={pool.fetchedAt}>{formatDateOf(pool.fetchedAt)}</time>
        </span>
      </header>
      <dl className="grid gap-x-6 gap-y-3 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label="Gross credits"
          value={`${formatCreditNumber(credits.grossCredits)} credits`}
          detail={formatUsdAmount(credits.grossUsd)}
        />
        <Figure
          label="Pool discount"
          value={`${formatCreditNumber(credits.discountCredits)} credits`}
          detail={`${formatUsdAmount(credits.discountUsd)} the seats' pool covers`}
        />
        <Figure
          label="Net credits"
          value={`${formatCreditNumber(credits.netCredits)} credits`}
          detail={`${formatUsdAmount(credits.netUsd)} billed beyond the pool`}
        />
        <Figure
          label="Seats"
          value={
            seats ? `${formatQuantity(seats.userMonths)} user-months` : "None"
          }
          exact={seats ? `${seats.userMonths} user-months` : undefined}
          detail={
            seats
              ? `${formatUsdAmount(seats.grossUsd)} gross, ${formatUsdAmount(seats.netUsd)} net`
              : "No seat line in the summary"
          }
        />
      </dl>
      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-sm">
        {pool.exhaustedOn ? (
          <span className="flex items-center gap-1.5">
            <AlertTriangle
              className="size-4 shrink-0 text-[#c98500] dark:text-[#fab219]"
              aria-hidden
            />
            Pool ran out on {shortDate(pool.exhaustedOn)} (UTC); credits since
            are billed at the list rate.
          </span>
        ) : (
          <span className="flex items-center gap-1.5">
            <CheckCircle2
              className="size-4 shrink-0 text-[#0ca30c]"
              aria-hidden
            />
            The pool still covers this month&apos;s credits.
          </span>
        )}
        <span>
          <span className="text-muted-foreground">
            GitHub&apos;s bill so far:
          </span>{" "}
          <span className="font-semibold tabular-nums">
            {formatUsdAmount(pool.payerNetUsd)}
          </span>
        </span>
      </div>
      {pool.unrecognized.length > 0 && (
        <p className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          <StatusBadge tone="warning" label="Unrecognized summary lines" />
          {pool.unrecognized
            .map((line) => `${line.product} / ${line.sku} (${line.unit})`)
            .join(", ")}
          , counted in the bill so far but in no figure above.
        </p>
      )}
    </section>
  );
}

function Figure({
  label,
  value,
  exact,
  detail,
}: {
  label: string;
  value: string;
  /** The value exactly as GitHub gave it, when `value` rounds it. */
  exact?: string;
  detail: string;
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="text-base font-semibold tabular-nums" title={exact}>
        {value}
      </dd>
      <dd className="text-xs text-muted-foreground tabular-nums">{detail}</dd>
    </div>
  );
}
