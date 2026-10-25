import * as React from "react";
import { CalendarX2, ChevronRight, Pencil, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type {
  AiAccountDisposition,
  AiAccountSuggestion,
  AiMeteredActivity,
  AiProviderAccount,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { AttentionPill } from "../../-components/attention-pill";
import { StatusBadge } from "../../-components/status-badge";
import { formatDateOf, plural } from "../../-lib/format";
import {
  activityLabel,
  clientLabel,
  daysIn,
  describeRanges,
  evidenceLabel,
  formatPeriod,
  seatPeriod,
  seatSummary,
  targetLabel,
  userName,
} from "../../-lib/provider-accounts";

/** Every column, so full-width rows span them all. */
const COLUMNS = 7;

/** The DOM id of an account's row, for scrolling to it. */
function accountRowId(accountId: number): string {
  return `provider-account-${accountId}`;
}

/** What a row can do with an account and its dispositions. */
export type AccountActions = Readonly<{
  /** Opens the editor for a new disposition over the default period. */
  onSet: (account: AiProviderAccount) => void;
  /** Links the account to a suggested user over the default period. */
  onAccept: (
    account: AiProviderAccount,
    suggestion: AiAccountSuggestion,
  ) => void;
  onEdit: (
    account: AiProviderAccount,
    disposition: AiAccountDisposition,
  ) => void;
  onEnd: (disposition: AiAccountDisposition) => void;
  onDelete: (
    account: AiProviderAccount,
    disposition: AiAccountDisposition,
  ) => void;
}>;

/**
 * One row per account: its login, seat, last activity, whose usage it is
 * today and over which days, and the suggestion. A row opens to its seat
 * history and every disposition.
 */
export function AccountTable({
  accounts,
  names,
  activity,
  today,
  reviewFrom,
  open,
  onToggle,
  editingAccountId,
  editorView,
  busy,
  focus,
  actions,
}: {
  accounts: readonly AiProviderAccount[];
  names: ReadonlyMap<number, string>;
  /** Each user's Copilot-paid usage in the review window. */
  activity: ReadonlyMap<number, AiMeteredActivity>;
  today: string;
  reviewFrom: string;
  open: ReadonlySet<number>;
  onToggle: (accountId: number) => void;
  /** The account whose editor is open, shown beneath its row. */
  editingAccountId: number | null;
  editorView: React.ReactNode;
  busy: boolean;
  /** An account to bring into view; each new object brings it again. */
  focus: Readonly<{ accountId: number }> | null;
  actions: AccountActions;
}) {
  React.useEffect(() => {
    if (focus === null) {
      return;
    }
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
    document.getElementById(accountRowId(focus.accountId))?.scrollIntoView({
      behavior: reduce.matches ? "auto" : "smooth",
      block: "center",
    });
  }, [focus]);

  return (
    <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead>Login</TableHead>
            <TableHead className="hidden sm:table-cell">Seat</TableHead>
            <TableHead className="hidden whitespace-nowrap md:table-cell">
              Last activity
            </TableHead>
            <TableHead>Whose usage</TableHead>
            <TableHead className="hidden sm:table-cell">Days</TableHead>
            <TableHead>Suggestion</TableHead>
            <TableHead className="w-28">
              <span className="sr-only">Actions</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        {accounts.map((account) => (
          <AccountRows
            key={account.accountId}
            account={account}
            names={names}
            activity={activity}
            today={today}
            reviewFrom={reviewFrom}
            isOpen={open.has(account.accountId)}
            onToggle={() => onToggle(account.accountId)}
            editorView={
              editingAccountId === account.accountId ? editorView : null
            }
            busy={busy}
            editing={editingAccountId !== null}
            actions={actions}
          />
        ))}
      </Table>
    </div>
  );
}

function AccountRows({
  account,
  names,
  activity,
  today,
  reviewFrom,
  isOpen,
  onToggle,
  editorView,
  busy,
  editing,
  actions,
}: {
  account: AiProviderAccount;
  names: ReadonlyMap<number, string>;
  activity: ReadonlyMap<number, AiMeteredActivity>;
  today: string;
  reviewFrom: string;
  isOpen: boolean;
  onToggle: () => void;
  editorView: React.ReactNode;
  busy: boolean;
  /** Whether any editor is open, which holds other edits back. */
  editing: boolean;
  actions: AccountActions;
}) {
  const seats = seatSummary(account.seats);
  const latest = seats.latest;
  const current =
    account.dispositions.find(
      (disposition) => disposition.id === account.currentDispositionId,
    ) ?? null;
  const top = account.suggestions[0] ?? null;
  const detailsId = `${accountRowId(account.accountId)}-details`;
  const uncovered = account.uncoveredSeatDays.reduce(
    (sum, range) => sum + daysIn(range.from, range.through),
    0,
  );

  return (
    <TableBody className="border-b border-border/60 last:border-b-0">
      <TableRow
        id={accountRowId(account.accountId)}
        className={cn(
          "scroll-mt-24 align-top",
          (isOpen || editorView) && "border-b-0",
        )}
      >
        <TableCell className="py-2.5">
          <div className="flex items-start gap-1">
            <button
              type="button"
              aria-expanded={isOpen}
              aria-controls={detailsId}
              aria-label={`${isOpen ? "Hide" : "Show"} ${account.login}'s seats and dispositions`}
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
            <div className="flex min-w-0 flex-col">
              <a
                href={`https://github.com/${encodeURIComponent(account.login)}`}
                target="_blank"
                rel="noreferrer"
                className="font-medium underline-offset-2 hover:underline"
              >
                {account.login}
              </a>
              {account.profile?.name && (
                <span className="truncate text-xs text-muted-foreground">
                  {account.profile.name}
                </span>
              )}
              {account.profile?.email && (
                <span className="truncate text-xs text-muted-foreground">
                  {account.profile.email}
                </span>
              )}
              {/* The seat column's summary, on screens too narrow for it. */}
              <span className="text-xs text-muted-foreground tabular-nums sm:hidden">
                {seats.label}
                {seats.cancelsOn && ` · cancels ${seats.cancelsOn}`}
              </span>
            </div>
          </div>
        </TableCell>
        <TableCell className="hidden py-2.5 sm:table-cell">
          <div className="flex flex-col items-start gap-1">
            <span className="whitespace-nowrap tabular-nums">
              {seats.label}
            </span>
            {seats.cancelsOn && (
              <StatusBadge
                tone="warning"
                label={`Cancels ${seats.cancelsOn}`}
              />
            )}
            {seats.earlier > 0 && (
              <span className="text-xs text-muted-foreground">
                +{plural(seats.earlier, "earlier seat")}
              </span>
            )}
          </div>
        </TableCell>
        <TableCell className="hidden py-2.5 md:table-cell">
          {latest?.lastActivityAt ? (
            <div className="flex flex-col">
              <span className="whitespace-nowrap tabular-nums">
                {formatDateOf(latest.lastActivityAt)}
              </span>
              {latest.lastActivityClient && (
                <span
                  className="text-xs text-muted-foreground"
                  title={latest.lastActivityClient}
                >
                  {clientLabel(latest.lastActivityClient)}
                </span>
              )}
            </div>
          ) : (
            <span className="text-muted-foreground">None</span>
          )}
        </TableCell>
        <TableCell className="py-2.5">
          {current ? (
            <span className="font-medium">
              {targetLabel(current.target, names)}
            </span>
          ) : (
            <StatusBadge
              tone={account.needsLook ? "warning" : "neutral"}
              label={
                account.dispositions.length > 0
                  ? "None today"
                  : "No disposition"
              }
            />
          )}
        </TableCell>
        <TableCell className="hidden py-2.5 sm:table-cell">
          <div className="flex flex-col items-start gap-1">
            <span className="whitespace-nowrap tabular-nums">
              {current ? formatPeriod(current) : "—"}
            </span>
            {uncovered > 0 && (
              <AttentionPill
                title={`No disposition: ${describeRanges(account.uncoveredSeatDays)}`}
              >
                {plural(uncovered, "day")} uncovered
              </AttentionPill>
            )}
          </div>
        </TableCell>
        <TableCell className="py-2.5">
          {top ? (
            <Suggestion
              account={account}
              suggestion={top}
              others={account.suggestions.slice(1)}
              names={names}
              activity={activity.get(top.userId) ?? null}
              disabled={busy || editing}
              onAccept={() => actions.onAccept(account, top)}
            />
          ) : account.defaultPeriod ? (
            <span className="text-muted-foreground">No match</span>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </TableCell>
        <TableCell className="py-2.5">
          <div className="flex justify-end gap-1">
            {account.defaultPeriod && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-8 px-2 text-xs"
                aria-label={`Set a disposition for ${account.login}`}
                disabled={busy || editing}
                onClick={() => actions.onSet(account)}
              >
                <Plus className="size-3.5" aria-hidden />
                Set
              </Button>
            )}
            {current && (
              <DispositionButtons
                account={account}
                disposition={current}
                today={today}
                busy={busy}
                editing={editing}
                actions={actions}
              />
            )}
          </div>
        </TableCell>
      </TableRow>
      {editorView && (
        <TableRow
          className={cn("hover:bg-transparent", isOpen && "border-b-0")}
        >
          <TableCell colSpan={COLUMNS} className="pt-0">
            {editorView}
          </TableCell>
        </TableRow>
      )}
      {isOpen && (
        <TableRow id={detailsId} className="hover:bg-transparent">
          <TableCell colSpan={COLUMNS} className="pt-0">
            <AccountDetails
              account={account}
              names={names}
              today={today}
              reviewFrom={reviewFrom}
              busy={busy}
              editing={editing}
              actions={actions}
            />
          </TableCell>
        </TableRow>
      )}
    </TableBody>
  );
}

function Suggestion({
  account,
  suggestion,
  others,
  names,
  activity,
  disabled,
  onAccept,
}: {
  account: AiProviderAccount;
  suggestion: AiAccountSuggestion;
  others: readonly AiAccountSuggestion[];
  names: ReadonlyMap<number, string>;
  activity: AiMeteredActivity | null;
  disabled: boolean;
  onAccept: () => void;
}) {
  const verified = suggestion.confidence === "verified";
  const name = userName(suggestion.userId, names);
  const period = account.defaultPeriod;
  return (
    <div className="flex flex-col items-start gap-1">
      <div className="flex flex-wrap items-center gap-1.5">
        <StatusBadge
          tone={verified ? "good" : "warning"}
          label={verified ? "Verified" : "Probable"}
        />
        <span className="font-medium">{name}</span>
      </div>
      <span className="text-xs text-muted-foreground">
        {evidenceLabel(suggestion.evidence)}
      </span>
      {activity && (
        <span className="text-xs text-muted-foreground">
          {activityLabel(activity)}
        </span>
      )}
      {others.length > 0 && (
        <span
          className="text-xs text-muted-foreground"
          title={others
            .map((other) => userName(other.userId, names))
            .join(", ")}
        >
          +{plural(others.length, "other candidate")}
        </span>
      )}
      {period && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 px-2 text-xs"
          disabled={disabled}
          title={`Link ${account.login} to ${name}, ${formatPeriod(period)}`}
          onClick={onAccept}
        >
          {verified ? "Accept" : "Confirm"}
          <span className="sr-only">
            {" "}
            {name} for {account.login}
          </span>
        </Button>
      )}
    </div>
  );
}

function DispositionButtons({
  account,
  disposition,
  today,
  busy,
  editing,
  actions,
}: {
  account: AiProviderAccount;
  disposition: AiAccountDisposition;
  today: string;
  busy: boolean;
  editing: boolean;
  actions: AccountActions;
}) {
  // Ending is for a disposition still running after today.
  const endable = disposition.validTo === null || disposition.validTo > today;
  const what = `${account.login}'s disposition ${formatPeriod(disposition)}`;
  return (
    <>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="size-8"
        aria-label={`Edit ${what}`}
        title="Edit"
        disabled={busy || editing}
        onClick={() => actions.onEdit(account, disposition)}
      >
        <Pencil className="size-4" />
      </Button>
      {endable && (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-8"
          aria-label={`End ${what}`}
          title="End…"
          disabled={busy || editing}
          onClick={() => actions.onEnd(disposition)}
        >
          <CalendarX2 className="size-4" />
        </Button>
      )}
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="size-8"
        aria-label={`Delete ${what}`}
        title="Delete"
        disabled={busy}
        onClick={() => actions.onDelete(account, disposition)}
      >
        <Trash2 className="size-4" />
      </Button>
    </>
  );
}

function AccountDetails({
  account,
  names,
  today,
  reviewFrom,
  busy,
  editing,
  actions,
}: {
  account: AiProviderAccount;
  names: ReadonlyMap<number, string>;
  today: string;
  reviewFrom: string;
  busy: boolean;
  editing: boolean;
  actions: AccountActions;
}) {
  return (
    <div className="grid gap-4 rounded-md bg-muted/30 p-3 md:grid-cols-2">
      <section className="flex flex-col gap-1.5">
        <h3 className="text-xs font-semibold text-muted-foreground">Seats</h3>
        {account.seats.length === 0 ? (
          <p className="text-sm text-muted-foreground">No seat seen.</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm">
            {[...account.seats].reverse().map((seat) => (
              <li key={seat.createdAt} className="flex flex-col">
                <span className="tabular-nums">{seatPeriod(seat)}</span>
                <span className="text-xs text-muted-foreground">
                  {seat.lastActivityAt
                    ? `Last active ${formatDateOf(seat.lastActivityAt)}${
                        seat.lastActivityClient
                          ? ` in ${clientLabel(seat.lastActivityClient)}`
                          : ""
                      }`
                    : "No activity reported"}
                </span>
              </li>
            ))}
          </ul>
        )}
        {account.uncoveredSeatDays.length > 0 && (
          <p className="text-xs text-muted-foreground">
            Seated without a disposition since {reviewFrom}:{" "}
            {describeRanges(account.uncoveredSeatDays)}
          </p>
        )}
        {account.profile && (
          <p className="text-xs text-muted-foreground">
            Public profile read {formatDateOf(account.profile.fetchedAt)}
            {!account.profile.name && !account.profile.email
              ? ", with no name or email"
              : ""}
            .
          </p>
        )}
      </section>
      <section className="flex flex-col gap-1.5">
        <h3 className="text-xs font-semibold text-muted-foreground">
          Dispositions
        </h3>
        {account.dispositions.length === 0 ? (
          <p className="text-sm text-muted-foreground">None yet.</p>
        ) : (
          <ul className="flex flex-col gap-1.5 text-sm">
            {account.dispositions.map((disposition) => (
              <li
                key={disposition.id}
                className="flex flex-wrap items-center justify-between gap-2"
              >
                <div className="flex flex-col">
                  <span className="flex flex-wrap items-center gap-1.5">
                    <span className="font-medium">
                      {targetLabel(disposition.target, names)}
                    </span>
                    <span className="text-muted-foreground tabular-nums">
                      {formatPeriod(disposition)}
                    </span>
                    {disposition.id === account.currentDispositionId && (
                      <StatusBadge tone="good" label="Today" />
                    )}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    Updated {formatDateOf(disposition.updatedAt)}
                    {disposition.updatedBy !== null &&
                      ` by ${userName(disposition.updatedBy, names)}`}
                  </span>
                </div>
                <div className="flex gap-1">
                  <DispositionButtons
                    account={account}
                    disposition={disposition}
                    today={today}
                    busy={busy}
                    editing={editing}
                    actions={actions}
                  />
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
