import { CheckCircle2 } from "lucide-react";
import type * as React from "react";
import type {
  AiProviderAccount,
  AiUnlinkedUser,
} from "@/lib/api/queries/ai-admin";
import { AttentionPill } from "../../-components/attention-pill";
import { formatEstimate, plural } from "../../-lib/format";
import {
  describeRanges,
  seatSummary,
  userName,
} from "../../-lib/provider-accounts";

/**
 * What still needs a decision in the review window: accounts with seats or
 * credits on days no disposition covers, and users whose Copilot-paid usage
 * has no link. Picking an entry opens its account's editor.
 */
export function NeedsALook({
  accounts,
  unlinkedUsers,
  reviewFrom,
  names,
  logins,
  disabled,
  onOpenAccount,
}: {
  /** Accounts that need a look, in the page's order. */
  accounts: readonly AiProviderAccount[];
  unlinkedUsers: readonly AiUnlinkedUser[];
  reviewFrom: string;
  names: ReadonlyMap<number, string>;
  logins: ReadonlyMap<number, string>;
  disabled: boolean;
  /** Opens an account's editor, with `userId` as its target when given. */
  onOpenAccount: (accountId: number, userId: number | null) => void;
}) {
  if (accounts.length === 0 && unlinkedUsers.length === 0) {
    return (
      <p className="flex items-center gap-2 rounded-xl border border-border/70 bg-card/60 px-4 py-3 text-sm">
        <CheckCircle2 className="size-4 shrink-0 text-[#0ca30c]" aria-hidden />
        Every account with a seat or credits has a disposition.
      </p>
    );
  }

  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Card
        title="Accounts without a disposition"
        count={accounts.length}
        hint={`Seats held or credits used since ${reviewFrom} on days no disposition covers.`}
        empty="None: every seat and every credit has a disposition on every day."
      >
        {accounts.map((account) => {
          const top = account.suggestions[0];
          return (
            <li key={account.accountId}>
              <button
                type="button"
                disabled={disabled}
                onClick={() => onOpenAccount(account.accountId, null)}
                className="flex w-full flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden disabled:pointer-events-none disabled:opacity-60"
              >
                <span className="flex flex-wrap items-baseline gap-x-2">
                  <span className="font-medium">{account.login}</span>
                  <span className="text-xs text-muted-foreground">
                    {seatSummary(account.seats).label}
                  </span>
                </span>
                {account.uncoveredSeatDays.length > 0 && (
                  <span className="text-xs text-muted-foreground">
                    Seat days uncovered:{" "}
                    {describeRanges(account.uncoveredSeatDays)}
                  </span>
                )}
                {account.uncoveredCreditDays.length > 0 && (
                  <span className="text-xs text-muted-foreground">
                    Credits on days without a disposition:{" "}
                    {describeRanges(account.uncoveredCreditDays)}
                  </span>
                )}
                <span className="text-xs text-muted-foreground">
                  {top
                    ? `Maybe ${userName(top.userId, names)} (${top.confidence})`
                    : "No match among Toki users"}
                </span>
              </button>
            </li>
          );
        })}
      </Card>
      <Card
        title="Toki users with Copilot-paid usage but no link"
        count={unlinkedUsers.length}
        hint={`Copilot-paid usage since ${reviewFrom} on days none of their links covers.`}
        empty="None: all Copilot-paid usage has a linked account."
      >
        {unlinkedUsers.map((user) => (
          <li key={user.userId} className="flex flex-col gap-0.5 px-2 py-1.5">
            <span className="flex flex-wrap items-baseline gap-x-2">
              <span className="font-medium">
                {userName(user.userId, names)}
              </span>
              <span className="text-xs text-muted-foreground tabular-nums">
                {formatEstimate(user.usage)}
              </span>
            </span>
            <span className="text-xs text-muted-foreground">
              {plural(user.uncoveredDays, "day")} without a link,{" "}
              {user.firstDay === user.lastDay
                ? user.firstDay
                : `${user.firstDay} – ${user.lastDay}`}
            </span>
            {user.suggestedAccountIds.length > 0 && (
              <span className="flex flex-wrap gap-1 text-xs text-muted-foreground">
                Maybe
                {user.suggestedAccountIds.map((accountId) => (
                  <button
                    key={accountId}
                    type="button"
                    disabled={disabled}
                    onClick={() => onOpenAccount(accountId, user.userId)}
                    className="font-medium text-foreground underline-offset-2 hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden disabled:pointer-events-none disabled:opacity-60"
                  >
                    {logins.get(accountId) ?? `account ${accountId}`}
                  </button>
                ))}
              </span>
            )}
          </li>
        ))}
      </Card>
    </div>
  );
}

function Card({
  title,
  count,
  hint,
  empty,
  children,
}: {
  title: string;
  count: number;
  hint: string;
  empty: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2 rounded-xl border border-border/70 bg-card/60 p-4">
      <header className="flex flex-col gap-0.5">
        <h2 className="flex items-center gap-2 text-sm font-semibold">
          {title}
          {count > 0 && <AttentionPill>{count}</AttentionPill>}
        </h2>
        <p className="text-xs text-muted-foreground">{hint}</p>
      </header>
      {count === 0 ? (
        <p className="text-sm text-muted-foreground">{empty}</p>
      ) : (
        <ul className="-mx-2 flex max-h-80 flex-col overflow-y-auto">
          {children}
        </ul>
      )}
    </section>
  );
}
