import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Skeleton } from "@/components/ui/skeleton";
import {
  aiAdminQueries,
  type AiMeteredMonth,
  type AiMeteredPeriod,
  type AiMeteredUnlinkedTracked,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { QueryError } from "../../-components/query-error";
import { monthLabel, plural } from "../../-lib/format";
import {
  billingNote,
  formatCredits,
  noDataReason,
  type NoDataReason,
} from "../../-lib/reconciliation";
import { AccountReconciliation } from "./account-reconciliation";
import { MeteredImportStatus } from "./metered-import-status";
import { PoolCard } from "./pool-card";
import { RepositoryUsage } from "./repository-usage";
import { ResidualDays } from "./residual-days";

/**
 * The GitHub Copilot tab: what GitHub billed in a UTC month, per account,
 * against what Toki tracked, with the import's state. From the metered
 * cutover on, the bill's Copilot lines follow its attribution.
 */
export function CopilotReconciliation({
  month,
  meteredPeriod,
  onMonth,
}: {
  /** The UTC month, `YYYY-MM`. */
  month: string;
  /** The bill's metered cutover; null when none is configured. */
  meteredPeriod: AiMeteredPeriod | null;
  onMonth: (month: string) => void;
}) {
  const query = useQuery({
    ...aiAdminQueries.meteredMonth(month),
    placeholderData: keepPreviousData,
  });
  const developersQuery = useQuery(aiAdminQueries.developers());
  const names = new Map(
    (developersQuery.data ?? []).map((d) => [d.userId, d.fullName] as const),
  );

  if (query.isPending) {
    return (
      <div
        role="status"
        aria-label="Loading GitHub Copilot usage"
        className="flex flex-col gap-4"
      >
        <Skeleton className="h-14 w-full" />
        <Skeleton className="h-28 w-full" />
        <Skeleton className="h-72 w-full" />
      </div>
    );
  }
  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="GitHub Copilot usage"
        onRetry={() => query.refetch()}
      />
    );
  }
  const data = query.data;
  // Accounts are listed for their seats too, so only an imported month has
  // figures worth a table.
  const missing = noDataReason(data, data.month);

  return (
    <div
      className={cn(
        "flex flex-col gap-8 transition-opacity motion-reduce:transition-none",
        query.isPlaceholderData && "opacity-60",
      )}
    >
      <div className="flex flex-col gap-3">
        <MeteredImportStatus data={data} month={month} />
        <MonthChips months={data.months} month={month} onMonth={onMonth} />
      </div>

      {missing !== null ? (
        <div className="rounded-xl border border-dashed border-border/80 p-8 text-center">
          <p className="font-medium">No GitHub data for {monthLabel(month)}</p>
          <p className="text-sm text-muted-foreground">
            {NO_DATA_HINTS[missing](data.import.backfillFrom)}
          </p>
        </div>
      ) : (
        <>
          <PoolCard pool={data.pool} month={data.month} />
          <section
            aria-labelledby="copilot-accounts-heading"
            className="flex flex-col gap-2"
          >
            <div>
              <h3
                id="copilot-accounts-heading"
                className="text-base font-semibold"
              >
                Per account
              </h3>
              <p className="text-sm text-muted-foreground">
                Accounts with credits, tracked usage or a seat in{" "}
                {monthLabel(data.month)}, flagged first. Open one for its models
                and GitHub&apos;s month total.
              </p>
            </div>
            <AccountReconciliation
              accounts={data.accounts}
              totals={data.totals}
              month={data.month}
              today={data.today}
              names={names}
            />
          </section>
          <UnlinkedTracked
            users={data.unlinkedTracked}
            month={data.month}
            names={names}
          />
          <RepositoryUsage
            repositories={data.repositories}
            codeReview={data.codeReview}
            month={data.month}
          />
          <ResidualDays
            days={data.residualDays}
            totals={data.totals}
            month={data.month}
            orgCheck={data.orgCheck}
          />
        </>
      )}

      <Explanation data={data} meteredPeriod={meteredPeriod} />
    </div>
  );
}

const NO_DATA_HINTS: Readonly<
  Record<NoDataReason, (backfillFrom: string) => string>
> = {
  beforeBackfill: (backfillFrom) => `The import starts on ${backfillFrom}.`,
  notStarted: () => "This month hasn't started yet.",
  notImported: () => "Import now to fetch it from GitHub.",
  sourceOff: () => "Connect GitHub to import it.",
};

/** Shortcuts to the months GitHub data was imported for. */
function MonthChips({
  months,
  month,
  onMonth,
}: {
  months: readonly string[];
  month: string;
  onMonth: (month: string) => void;
}) {
  if (months.length === 0) {
    return null;
  }
  return (
    <nav
      aria-label="Months with GitHub data"
      className="flex flex-wrap items-center gap-1.5 text-sm"
    >
      <span className="text-muted-foreground">Months with GitHub data:</span>
      {months.map((option) => (
        <button
          key={option}
          type="button"
          aria-current={option === month ? "true" : undefined}
          onClick={() => onMonth(option)}
          className={cn(
            "rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden motion-reduce:transition-none",
            option === month
              ? "border-primary bg-primary/10 text-foreground"
              : "border-border/70 text-muted-foreground hover:text-foreground",
          )}
        >
          {monthLabel(option)}
        </button>
      ))}
    </nav>
  );
}

/** Toki users with Copilot-paid usage on days no account of theirs is
 * linked: their usage can't be reconciled until it is. */
function UnlinkedTracked({
  users,
  month,
  names,
}: {
  users: readonly AiMeteredUnlinkedTracked[];
  month: string;
  names: ReadonlyMap<number, string>;
}) {
  if (users.length === 0) {
    return null;
  }
  return (
    <section
      aria-labelledby="copilot-unlinked-heading"
      className="flex flex-col gap-2 rounded-xl border border-border/70 bg-card/60 p-4"
    >
      <h3 id="copilot-unlinked-heading" className="text-sm font-semibold">
        Tracked by Toki, no linked account
      </h3>
      <p className="text-xs text-muted-foreground">
        Copilot-paid usage on days none of the user&apos;s GitHub accounts is
        linked. Link their account on{" "}
        <Link
          to="/admin/github-logins"
          className="font-medium text-foreground underline-offset-2 hover:underline"
        >
          GitHub logins
        </Link>{" "}
        to reconcile it.
      </p>
      <ul className="flex flex-wrap gap-x-5 gap-y-1 text-sm">
        {users.map((user) => (
          <li key={user.userId} className="tabular-nums">
            <Link
              to="/admin/ai-billing/$userId"
              params={{ userId: String(user.userId) }}
              search={{ month }}
              className="font-medium underline-offset-2 hover:underline"
            >
              {names.get(user.userId) ?? `User ${String(user.userId)}`}
            </Link>{" "}
            <span className="text-muted-foreground">
              {formatCredits(user.tracked)} on {plural(user.days, "day")}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Explanation({
  data,
  meteredPeriod,
}: {
  data: AiMeteredMonth;
  meteredPeriod: AiMeteredPeriod | null;
}) {
  return (
    <section
      aria-label="How the reconciliation works"
      className="rounded-xl border border-border/60 bg-card/60 p-4"
    >
      <ul className="max-w-4xl list-disc space-y-2 pl-5 text-sm text-muted-foreground">
        <li>
          Days are UTC calendar days and months UTC months, as GitHub bills. A
          credit is $0.01 at GitHub&apos;s list rate; GitHub gross (G) is before
          the pool discount{data.namespace && ` of ${data.namespace}`}.
        </li>
        <li>
          Toki tracked (T) is Toki&apos;s Copilot-priced estimate of the usage
          Copilot paid for, of the user an account is linked to on each day.
          Unpriced usage carries no weight, and free models (next edit
          suggestions, gpt-4o-mini) carry none either.
        </li>
        <li>
          <span className="font-medium text-foreground">Attributed</span> is
          min(G, T) per day and model: what the T/G rule bills to the
          user&apos;s destinations, which are customer projects, Unassigned, or
          personal and internal usage that stays with Spinit.{" "}
          <span className="font-medium text-foreground">Untracked</span> is the
          rest of G on linked days, Spinit overhead, never spread over
          customers. Cloud agent credits are always untracked; code review
          credits are billed per repository instead.
        </li>
        <li>
          The flags comparing G with T (GitHub billed nothing, below 85%) count
          settled days only, once GitHub has had its settle days to post them.
          Days not imported for an account yet aren&apos;t compared at all.
        </li>
        <li>
          Credits on days without a disposition are unlinked, those of an
          account that isn&apos;t a Toki user&apos;s are external, and credits
          no listed account explains are unknown account. All stay with Spinit.
          Unknown account rests on the org total, so a day settles only once the
          org total equals GitHub&apos;s month total.
        </li>
        <li>
          {billingNote(meteredPeriod)}
          {meteredPeriod?.applies &&
            " The bill's unknown-account line leaves code review out, which is billed by repository, so it can differ from the unknown account here."}
        </li>
      </ul>
    </section>
  );
}
