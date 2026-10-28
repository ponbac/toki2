import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Skeleton } from "@/components/ui/skeleton";
import {
  aiAdminQueries,
  type AiMeteredMonth,
  type AiMeteredPeriod,
} from "@/lib/api/queries/ai-admin";
import { QueryError } from "../../-components/query-error";
import { monthLabel, plural } from "../../-lib/format";
import {
  billingNote,
  formatCredits,
  noDataReason,
  type NoDataReason,
} from "../../-lib/reconciliation";
import { AccountReconciliationRows } from "./account-reconciliation";
import { MeteredImportStatus } from "./metered-import-status";

/**
 * A developer's GitHub Copilot month: their linked accounts' gross credits
 * against what Toki tracked, the import's state, and tracked usage on days
 * no account of theirs is linked.
 */
export function CopilotCard({
  month,
  userId,
  name,
  meteredPeriod,
}: {
  /** The UTC month, `YYYY-MM`. */
  month: string;
  userId: number;
  name: string;
  /** The bill's metered cutover; null when none is configured. */
  meteredPeriod: AiMeteredPeriod | null;
}) {
  const query = useQuery(aiAdminQueries.meteredDeveloper(month, userId));
  const developersQuery = useQuery(aiAdminQueries.developers());
  const names = new Map(
    (developersQuery.data ?? []).map((d) => [d.userId, d.fullName] as const),
  );

  return (
    <section
      aria-labelledby="developer-copilot-heading"
      className="flex flex-col gap-3"
    >
      <div>
        <h3 id="developer-copilot-heading" className="text-lg font-semibold">
          GitHub Copilot
        </h3>
        <p className="text-sm text-muted-foreground">
          What GitHub billed {name}&apos;s linked accounts in{" "}
          {monthLabel(month)} (UTC), against what Toki tracked.{" "}
          {billingNote(meteredPeriod)}
        </p>
      </div>
      {query.isPending ? (
        <div role="status" aria-label="Loading GitHub Copilot usage">
          <Skeleton className="h-24 w-full" />
        </div>
      ) : query.isError ? (
        <QueryError
          error={query.error}
          what={`${name}'s GitHub Copilot usage`}
          onRetry={() => query.refetch()}
        />
      ) : (
        <CardBody data={query.data} month={month} name={name} names={names} />
      )}
    </section>
  );
}

function CardBody({
  data,
  month,
  name,
  names,
}: {
  data: AiMeteredMonth;
  month: string;
  name: string;
  names: ReadonlyMap<number, string>;
}) {
  const missing = noDataReason(data, month);
  return (
    <>
      <MeteredImportStatus data={data} month={month} compact />
      {missing !== null ? (
        <NoData
          reason={missing}
          month={month}
          backfillFrom={data.import.backfillFrom}
        />
      ) : data.accounts.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border/70 px-4 py-4 text-sm text-muted-foreground">
          No GitHub account is linked to {name} in {monthLabel(month)}. Link one
          on{" "}
          <Link
            to="/admin/github-logins"
            className="font-medium text-foreground underline-offset-2 hover:underline"
          >
            GitHub logins
          </Link>
          .
        </p>
      ) : (
        <AccountReconciliationRows
          accounts={data.accounts}
          month={data.month}
          today={data.today}
          names={names}
        />
      )}
      {data.unlinkedTracked.map((user) => (
        <p key={user.userId} className="text-sm text-muted-foreground">
          Toki tracked{" "}
          <span className="font-medium text-foreground tabular-nums">
            {formatCredits(user.tracked)}
          </span>{" "}
          of {name}&apos;s Copilot-paid usage on {plural(user.days, "day")} no
          linked account covers; it isn&apos;t reconciled until one is.
        </p>
      ))}
    </>
  );
}

/** A month the import has no GitHub data for, and why: never a reason to
 * link an account. */
function NoData({
  reason,
  month,
  backfillFrom,
}: {
  reason: NoDataReason;
  month: string;
  backfillFrom: string;
}) {
  return (
    <p className="rounded-xl border border-dashed border-border/70 px-4 py-4 text-sm text-muted-foreground">
      No GitHub data for {monthLabel(month)}.{" "}
      {reason === "beforeBackfill" ? (
        `The import starts on ${backfillFrom}.`
      ) : reason === "notStarted" ? (
        "This month hasn't started yet."
      ) : reason === "sourceOff" ? (
        "GitHub isn't connected."
      ) : (
        <>
          Import it on the{" "}
          <Link
            to="/admin/ai-billing"
            search={{ month, tab: "copilot" }}
            className="font-medium text-foreground underline-offset-2 hover:underline"
          >
            GitHub Copilot tab
          </Link>
          .
        </>
      )}
    </p>
  );
}
