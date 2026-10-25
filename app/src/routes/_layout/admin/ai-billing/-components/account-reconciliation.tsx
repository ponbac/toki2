import * as React from "react";
import { ChevronRight } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type {
  AiMeteredAccount,
  AiMeteredFlag,
  AiMeteredGroup,
  AiMeteredTotals,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { AttentionPill } from "../../-components/attention-pill";
import { StatusBadge } from "../../-components/status-badge";
import { formatCount, monthLabel, plural } from "../../-lib/format";
import {
  EXTERNAL_OWNER_LABEL,
  FLAG_HINTS,
  FLAG_LABELS,
  creditHundredths,
  creditsAsUsd,
  flagTone,
  formatCreditNumber,
  formatCredits,
  formatRatio,
  formatShare,
  groupKindLabel,
  ownerDays,
  ownerLabel,
} from "../../-lib/reconciliation";
import { CheckLine } from "./check-line";

/** Every column, so full-width rows span them all. */
const COLUMNS = 7;

const AMOUNT = "text-right tabular-nums whitespace-nowrap";

/**
 * Every listed account's month: GitHub's gross credits (G) against Toki's
 * tracked Copilot usage (T), what the T/G rule would attribute, what stays
 * untracked, and its flags; then the totals and the credits no account
 * explains (unknown account).
 */
export function AccountReconciliation({
  accounts,
  totals,
  month,
  today,
  names,
}: {
  accounts: readonly AiMeteredAccount[];
  totals: AiMeteredTotals;
  month: string;
  /** Today's UTC day, `YYYY-MM-DD`. */
  today: string;
  names: ReadonlyMap<number, string>;
}) {
  if (accounts.length === 0) {
    return (
      <p className="rounded-xl border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
        No GitHub account has credits, tracked usage or a seat in{" "}
        {monthLabel(month)}.
      </p>
    );
  }
  const unknown = creditHundredths(totals.unknownAccount);
  const gaps = creditHundredths(totals.incompleteImport);
  return (
    <AccountReconciliationRows
      accounts={accounts}
      month={month}
      today={today}
      names={names}
      footer={
        <>
          <TableRow className="border-t-2 border-border/80 bg-muted/30 font-semibold hover:bg-muted/30">
            <TableCell className="py-2.5">All accounts</TableCell>
            <TableCell className={AMOUNT} title={creditsAsUsd(totals.gross)}>
              {formatCreditNumber(totals.gross)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden md:table-cell")}>
              <TrackedAmount
                tracked={totals.tracked}
                oldBasis={totals.trackedOldBasis}
              />
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden md:table-cell")}>
              {formatCreditNumber(totals.attributed)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden sm:table-cell")}>
              {formatCreditNumber(totals.untracked)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden lg:table-cell")}>
              —
            </TableCell>
            <TableCell />
          </TableRow>
          <TableRow className="hover:bg-transparent">
            <TableCell className="py-2.5">
              <div className="flex flex-col">
                <span className="font-medium">Unknown account</span>
                <span className="text-xs text-muted-foreground">
                  Org total minus the per-account sum, on settled and
                  provisional days
                  {creditHundredths(totals.unknownAccountSettled) !== unknown &&
                    ` (settled days: ${formatCredits(totals.unknownAccountSettled)})`}
                  .
                </span>
                {gaps !== 0 && (
                  <span className="text-xs text-muted-foreground">
                    {formatCredits(totals.incompleteImport)} more on days not
                    fully imported: an import gap, not unknown account.
                  </span>
                )}
              </div>
            </TableCell>
            <TableCell
              className={AMOUNT}
              title={creditsAsUsd(totals.unknownAccount)}
            >
              {formatCreditNumber(totals.unknownAccount)}
            </TableCell>
            <TableCell className="hidden md:table-cell" />
            <TableCell className="hidden md:table-cell" />
            <TableCell className="hidden sm:table-cell" />
            <TableCell className="hidden lg:table-cell" />
            <TableCell>
              {unknown !== 0 && (
                <StatusBadge
                  tone="warning"
                  label={
                    unknown > 0
                      ? "Credits no account explains"
                      : "Accounts above the org total"
                  }
                />
              )}
            </TableCell>
          </TableRow>
        </>
      }
    />
  );
}

/**
 * The account table itself, also on a developer's card: one row per account
 * that opens to its groups and GitHub check. `footer` rows go beneath.
 */
export function AccountReconciliationRows({
  accounts,
  month,
  today,
  names,
  footer,
}: {
  accounts: readonly AiMeteredAccount[];
  month: string;
  /** Today's UTC day, `YYYY-MM-DD`. */
  today: string;
  names: ReadonlyMap<number, string>;
  footer?: React.ReactNode;
}) {
  const [open, setOpen] = React.useState<ReadonlySet<number>>(new Set());
  const toggle = (accountId: number) =>
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(accountId)) {
        next.add(accountId);
      }
      return next;
    });

  return (
    <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead>Account</TableHead>
            <TableHead className="text-right whitespace-nowrap">
              <abbr title="GitHub's gross credits at the $0.01 list rate, before the pool discount">
                GitHub gross
              </abbr>
            </TableHead>
            <TableHead className="hidden text-right whitespace-nowrap md:table-cell">
              <abbr title="Toki's Copilot-priced estimate of the linked user's Copilot-paid usage">
                Toki tracked
              </abbr>
            </TableHead>
            <TableHead className="hidden text-right md:table-cell">
              <abbr title="What the T/G rule would bill: min(G, T) per UTC day and model">
                Attributed
              </abbr>
            </TableHead>
            <TableHead className="hidden text-right sm:table-cell">
              <abbr title="G minus attributed on linked days: Spinit overhead">
                Untracked
              </abbr>
            </TableHead>
            <TableHead className="hidden text-right whitespace-nowrap lg:table-cell">
              <abbr title="Attributed over trackable gross on linked days (cloud agent and code review left out)">
                Tracked share
              </abbr>
            </TableHead>
            <TableHead>Flags</TableHead>
          </TableRow>
        </TableHeader>
        {accounts.map((account) => (
          <AccountRows
            key={account.accountId}
            account={account}
            month={month}
            today={today}
            names={names}
            isOpen={open.has(account.accountId)}
            onToggle={() => toggle(account.accountId)}
          />
        ))}
        {footer && <TableBody>{footer}</TableBody>}
      </Table>
      <p className="border-t border-border/60 px-4 py-2 text-xs text-muted-foreground">
        Credits, a credit being $0.01 at GitHub&apos;s list rate. Hover an
        amount for dollars.
      </p>
    </div>
  );
}

function AccountRows({
  account,
  month,
  today,
  names,
  isOpen,
  onToggle,
}: {
  account: AiMeteredAccount;
  month: string;
  today: string;
  names: ReadonlyMap<number, string>;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const detailsId = `metered-account-${account.accountId}-details`;
  return (
    <TableBody className="border-b border-border/60 last:border-b-0">
      <TableRow className={cn("align-top", isOpen && "border-b-0")}>
        <TableCell className="py-2.5">
          <div className="flex items-start gap-1">
            <button
              type="button"
              aria-expanded={isOpen}
              aria-controls={detailsId}
              aria-label={`${isOpen ? "Hide" : "Show"} ${account.login}'s models and GitHub check`}
              onClick={onToggle}
              className="-ml-1.5 rounded-md p-1 text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
            >
              <ChevronRight
                className={cn(
                  "size-4 transition-transform duration-150 motion-reduce:transition-none",
                  isOpen && "rotate-90",
                )}
                aria-hidden
              />
            </button>
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="font-medium">{account.login}</span>
              <Owners
                account={account}
                month={month}
                today={today}
                names={names}
              />
              {account.daysNotImported > 0 && (
                <span
                  className="text-xs text-muted-foreground"
                  title="GitHub's usage of these days isn't imported for the account yet, so Toki's tracked usage on them isn't compared."
                >
                  {plural(account.daysNotImported, "day")} not imported yet
                </span>
              )}
            </div>
          </div>
        </TableCell>
        <TableCell
          className={cn(AMOUNT, "py-2.5 font-medium")}
          title={creditsAsUsd(account.gross)}
        >
          {formatCreditNumber(account.gross)}
        </TableCell>
        <TableCell className={cn(AMOUNT, "hidden py-2.5 md:table-cell")}>
          <TrackedAmount
            tracked={account.tracked}
            oldBasis={account.trackedOldBasis}
          />
        </TableCell>
        <TableCell
          className={cn(AMOUNT, "hidden py-2.5 md:table-cell")}
          title={creditsAsUsd(account.attributed)}
        >
          {formatCreditNumber(account.attributed)}
        </TableCell>
        <TableCell
          className={cn(AMOUNT, "hidden py-2.5 sm:table-cell")}
          title={creditsAsUsd(account.untracked)}
        >
          {formatCreditNumber(account.untracked)}
        </TableCell>
        <TableCell className={cn(AMOUNT, "hidden py-2.5 lg:table-cell")}>
          {formatShare(account.trackedShare)}
        </TableCell>
        <TableCell className="py-2.5">
          <Flags flags={account.flags} />
        </TableCell>
      </TableRow>
      {isOpen && (
        <TableRow id={detailsId} className="hover:bg-transparent">
          <TableCell colSpan={COLUMNS} className="pt-0">
            <AccountDetails account={account} names={names} month={month} />
          </TableCell>
        </TableRow>
      )}
    </TableBody>
  );
}

/** Whose the account was in the month, and days with credits but no link. */
function Owners({
  account,
  month,
  today,
  names,
}: {
  account: AiMeteredAccount;
  month: string;
  today: string;
  names: ReadonlyMap<number, string>;
}) {
  if (account.owners.length === 0) {
    // Only unlinked credits ask for a link; a seat alone is noted.
    return (
      <span>
        {creditHundredths(account.unlinked) > 0 ? (
          <StatusBadge tone="warning" label="Unlinked" />
        ) : (
          <StatusBadge tone="neutral" label="No disposition" />
        )}
      </span>
    );
  }
  return (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
      {account.owners.map((owner) => {
        const days = ownerDays(owner, month, today);
        const label = ownerLabel(owner, names);
        return (
          <span
            key={`${owner.from}-${owner.through}`}
            className="inline-flex items-center gap-1"
          >
            {label === EXTERNAL_OWNER_LABEL ? (
              <StatusBadge tone="neutral" label={EXTERNAL_OWNER_LABEL} />
            ) : (
              <span className="text-foreground">{label}</span>
            )}
            {days && <span className="tabular-nums">{days}</span>}
          </span>
        );
      })}
      {account.unlinkedDays > 0 && (
        <AttentionPill title="Days with credits that no disposition covers">
          {plural(account.unlinkedDays, "day")} unlinked
        </AttentionPill>
      )}
    </span>
  );
}

/** T, with how much of it is on an old pricing basis. */
function TrackedAmount({
  tracked,
  oldBasis,
}: {
  tracked: string;
  oldBasis: string;
}) {
  return (
    <div className="flex flex-col items-end">
      <span title={creditsAsUsd(tracked)}>{formatCreditNumber(tracked)}</span>
      {creditHundredths(oldBasis) > 0 && (
        <span
          className="text-xs font-normal text-muted-foreground"
          title="Priced on an older basis than its machine uses now"
        >
          {formatCreditNumber(oldBasis)} on an old pricing basis
        </span>
      )}
    </div>
  );
}

function Flags({ flags }: { flags: readonly AiMeteredFlag[] }) {
  if (flags.length === 0) {
    return <span className="text-muted-foreground">—</span>;
  }
  return (
    <span className="flex flex-wrap gap-1">
      {flags.map((flag) => (
        <span key={flag} title={FLAG_HINTS[flag]}>
          <StatusBadge tone={flagTone(flag)} label={FLAG_LABELS[flag]} />
        </span>
      ))}
    </span>
  );
}

function AccountDetails({
  account,
  names,
  month,
}: {
  account: AiMeteredAccount;
  names: ReadonlyMap<number, string>;
  month: string;
}) {
  const parts = [
    ["Billed per repository (code review)", account.repositoryBilled],
    ["Untrackable (cloud agent)", account.untrackable],
    ["On days without a disposition", account.unlinked],
    ["On external days", account.external],
    ["Tracked before the reprice floor", account.trackedBeforeFloor],
  ] as const;
  const shown = parts.filter(([, amount]) => creditHundredths(amount) !== 0);
  return (
    <div className="flex flex-col gap-3 rounded-md bg-muted/30 p-3">
      {account.owners.length > 0 && (
        <p className="text-xs text-muted-foreground">
          {account.owners
            .map(
              (owner) =>
                `${ownerLabel(owner, names)} ${owner.from} – ${owner.through}`,
            )
            .join("; ")}
          {account.unlinkedDays > 0 &&
            `; ${plural(account.unlinkedDays, "day")} with credits and no disposition`}
          .
        </p>
      )}
      {(shown.length > 0 || account.unpricedRecords > 0) && (
        <ul className="flex flex-wrap gap-x-5 gap-y-1 text-xs">
          {shown.map(([label, amount]) => (
            <li key={label}>
              <span className="text-muted-foreground">{label}:</span>{" "}
              <span className="tabular-nums">{formatCredits(amount)}</span>
            </li>
          ))}
          {account.unpricedRecords > 0 && (
            <li>
              <span className="text-muted-foreground">Unpriced records:</span>{" "}
              <span className="tabular-nums">
                {formatCount(account.unpricedRecords)}
              </span>{" "}
              <span className="text-muted-foreground">(no weight)</span>
            </li>
          )}
        </ul>
      )}
      <Groups groups={account.groups} month={month} />
      <CheckLine check={account.payerCheck} />
    </div>
  );
}

function Groups({
  groups,
  month,
}: {
  groups: readonly AiMeteredGroup[];
  month: string;
}) {
  if (groups.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No credits or tracked usage in {monthLabel(month)}.
      </p>
    );
  }
  return (
    <Table className="text-xs">
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          <TableHead className="h-8">Model</TableHead>
          <TableHead className="hidden h-8 sm:table-cell">Kind</TableHead>
          <TableHead className="h-8 text-right">G</TableHead>
          <TableHead className="h-8 text-right">T</TableHead>
          <TableHead className="hidden h-8 text-right md:table-cell">
            Attributed
          </TableHead>
          <TableHead className="hidden h-8 text-right md:table-cell">
            Untracked
          </TableHead>
          <TableHead className="hidden h-8 text-right sm:table-cell">
            G/T
          </TableHead>
          <TableHead className="h-8">Flags</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {groups.map((group) => (
          <TableRow
            key={`${group.sku ?? ""}:${group.model}:${group.kind}`}
            className="align-top hover:bg-transparent"
          >
            <TableCell className="py-1.5">
              <div className="flex flex-col">
                <span className="flex flex-wrap items-center gap-1.5">
                  <span className="font-mono font-medium">
                    {group.model || "(no model)"}
                  </span>
                  {group.status === "free" && (
                    <StatusBadge tone="neutral" label="Free" />
                  )}
                  {group.status === "provisional" && (
                    <StatusBadge tone="warning" label="Provisional rate" />
                  )}
                </span>
                <span className="text-muted-foreground">
                  {group.sku ?? "Not billed by GitHub"}
                  {group.labels.length > 0 &&
                    ` · ${group.labels.map((label) => `“${label}”`).join(", ")}`}
                </span>
                {creditHundredths(group.autoGross) > 0 && (
                  <span className="text-muted-foreground">
                    {formatCredits(group.autoGross)} through Auto
                  </span>
                )}
              </div>
            </TableCell>
            <TableCell className="hidden py-1.5 sm:table-cell">
              {groupKindLabel(group.kind)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "py-1.5")}>
              {formatCreditNumber(group.gross)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "py-1.5")}>
              {formatCreditNumber(group.tracked)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden py-1.5 md:table-cell")}>
              {formatCreditNumber(group.attributed)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden py-1.5 md:table-cell")}>
              {formatCreditNumber(group.untracked)}
            </TableCell>
            <TableCell className={cn(AMOUNT, "hidden py-1.5 sm:table-cell")}>
              {formatRatio(group.ratio)}
            </TableCell>
            <TableCell className="py-1.5">
              <Flags flags={group.flags} />
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
