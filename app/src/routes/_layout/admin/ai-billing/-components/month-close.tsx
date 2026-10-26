import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronRight,
  CircleDashed,
  Lock,
  ShieldCheck,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { apiErrorToast } from "@/lib/api/errors";
import {
  CLOSE_CHECKS,
  MAX_WAIVER_REASON_CHARS,
  checkWaivable,
  importRunningSince,
  waiverReasonProblem,
} from "@/lib/api/contracts/ai-month-close";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import {
  aiAdminQueries,
  type AiCloseCheck,
  type AiCloseCheckId,
  type AiCloseReadiness,
  type AiMonthCloseStatus,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { QueryError } from "../../-components/query-error";
import { GITHUB_REPOSITORIES_ID } from "../../-components/repository-mappings";
import { formatInstant, listOf, monthLabel, plural } from "../../-lib/format";
import {
  CLOSE_CHECK_LABELS,
  NOT_APPLICABLE_NOTE,
  checkTargets,
  latestReadiness,
  problemText,
  type CloseCheckTarget,
} from "../../-lib/month-close";

/** The tabs of the AI billing page a check's problems are resolved on. */
export type CloseTab = Extract<CloseCheckTarget, "copilot" | "data">;

/** How many of a check's problems show before "and N more". */
const PROBLEMS_SHOWN = 5;

/**
 * Month close: in progress, the server's readiness checklist with waivers and
 * the close itself, or, once closed, the close with its frozen readiness, the
 * re-open and the history of earlier closes. The server evaluates every check;
 * a failed one is waived with a reason the close records, except while an
 * import of the month is running, which no waiver covers.
 */
export function MonthClosePanel({
  month,
  onOpenTab,
}: {
  month: string;
  onOpenTab: (tab: CloseTab) => void;
}) {
  const query = useQuery(aiAdminQueries.monthClose(month));

  return (
    <section
      aria-labelledby="month-close-heading"
      className="flex flex-col gap-3"
    >
      <h2 id="month-close-heading" className="text-sm font-semibold">
        Month close
      </h2>
      {query.isPending ? (
        <div
          role="status"
          aria-label="Checking whether the month can close"
          className="space-y-2"
        >
          <Skeleton className="h-5 w-72" />
          <Skeleton className="h-40 w-full" />
        </div>
      ) : query.isError ? (
        <QueryError
          error={query.error}
          what="the month's close"
          onRetry={() => query.refetch()}
        />
      ) : query.data.close !== null ? (
        <ClosedMonth status={query.data} close={query.data.close} />
      ) : query.data.readiness === null ? (
        <p className="text-sm text-muted-foreground">
          {monthLabel(month)} can be closed once it is over in UTC and{" "}
          {query.data.timeZone}
          {query.data.overAt !== null &&
            ` (${formatInstant(query.data.overAt)})`}
          .
        </p>
      ) : (
        <OpenMonth
          key={month}
          status={query.data}
          readiness={query.data.readiness}
          onOpenTab={onOpenTab}
        />
      )}
    </section>
  );
}

type WaiverDraft = Readonly<{ on: boolean; reason: string }>;

/** An open month that is over: its checklist, waivers and the close. */
function OpenMonth({
  status,
  readiness: evaluated,
  onOpenTab,
}: {
  status: AiMonthCloseStatus;
  readiness: AiCloseReadiness;
  onOpenTab: (tab: CloseTab) => void;
}) {
  const month = status.month;
  // A refused close answers with the readiness it evaluated, which can be
  // newer than the page's.
  const [refused, setRefused] = React.useState<AiCloseReadiness | null>(null);
  const readiness = latestReadiness(evaluated, refused) ?? evaluated;
  const [waivers, setWaivers] = React.useState<
    Partial<Record<AiCloseCheckId, WaiverDraft>>
  >({});
  const close = aiAdminMutations.useCloseMonth({
    onSuccess: (result) => {
      if (result.kind === "notReady") {
        setRefused(result.readiness);
        toast.warning(
          `${monthLabel(month)} is not ready to close: a check failed when the server checked again.`,
        );
        return;
      }
      toast.success(
        `Closed ${monthLabel(month)}${result.status.close ? ` (snapshot #${result.status.close.closeId})` : ""}`,
      );
    },
    onError: apiErrorToast(`Failed to close ${monthLabel(month)}`),
  });

  // A running import of the month blocks the close whatever is waived.
  const importRunning = importRunningSince(readiness);
  const failed = readiness.checks.filter((check) => check.status === "failed");
  const unwaived = failed.filter((check) => {
    const waiver = waivers[check.id];
    return (
      !checkWaivable(check) ||
      !waiver?.on ||
      waiverReasonProblem(waiver.reason) !== null
    );
  });
  const setWaiver = (id: AiCloseCheckId, next: WaiverDraft) =>
    setWaivers((previous) => ({ ...previous, [id]: next }));

  const submit = () => {
    const confirmed = window.confirm(
      `Close ${monthLabel(month)}? Its bill is frozen as it is now. Later changes post as adjustments on the next open month's bill.`,
    );
    if (!confirmed) {
      return;
    }
    close.mutate({
      month,
      waivers: failed.filter(checkWaivable).flatMap((check) => {
        const waiver = waivers[check.id];
        return waiver?.on ? [{ check: check.id, reason: waiver.reason }] : [];
      }),
    });
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="max-w-3xl text-sm text-muted-foreground">
        Closing freezes {monthLabel(month)}&apos;s whole AI bill as it is now:
        API lines, subscription fees, Copilot lines and the adjustments it
        carries. The page, its developers and the CSV are then served as
        invoiced, and a later change bills once, as an adjustment on the next
        open month. Checked {formatInstant(readiness.evaluatedAt)}.
      </p>
      <Checklist
        readiness={readiness}
        onOpenTab={onOpenTab}
        waivers={waivers}
        onWaiver={setWaiver}
        disabled={close.isPending}
      />
      <div className="flex flex-wrap items-center gap-3">
        <Button
          type="button"
          disabled={
            importRunning !== null || unwaived.length > 0 || close.isPending
          }
          onClick={submit}
        >
          <Lock className="size-4" />
          {close.isPending ? "Closing..." : `Close ${monthLabel(month)}`}
        </Button>
        <p className="text-sm text-muted-foreground" aria-live="polite">
          {importRunning !== null
            ? `An import of ${monthLabel(month)} is running (started ${formatInstant(importRunning)}). A running import cannot be waived: close the month once it finishes.`
            : unwaived.length > 0
              ? `Fix or waive ${unwaived.length === 1 ? "the failed check" : `the ${unwaived.length} failed checks`} to close: ${listOf(
                  unwaived.map((check) => CLOSE_CHECK_LABELS[check.id].label),
                  "conjunction",
                )}.`
              : failed.length > 0
                ? `${plural(failed.length, "check")} waived; the reasons are kept with the close.`
                : "Every check passes."}
        </p>
      </div>
      <CloseHistory history={status.history} />
    </div>
  );
}

const STATUS_ICONS = {
  passed: {
    icon: CheckCircle2,
    className: "text-[#0ca30c]",
    label: "Passed",
  },
  failed: {
    icon: AlertTriangle,
    className: "text-[#c98500] dark:text-[#fab219]",
    label: "Failed",
  },
  waived: {
    icon: ShieldCheck,
    className: "text-muted-foreground",
    label: "Waived",
  },
  notApplicable: {
    icon: CircleDashed,
    className: "text-muted-foreground",
    label: "Not applicable",
  },
} as const satisfies Record<AiCloseCheck["status"], unknown>;

/** The checks in the server's order, each with its problems, where they are
 * resolved, and, while the month is open, a waiver for a failed one. */
function Checklist({
  readiness,
  onOpenTab,
  waivers,
  onWaiver,
  disabled = false,
}: {
  readiness: AiCloseReadiness;
  onOpenTab?: (tab: CloseTab) => void;
  /** Absent for a closed month's frozen readiness. */
  waivers?: Partial<Record<AiCloseCheckId, WaiverDraft>>;
  onWaiver?: (id: AiCloseCheckId, waiver: WaiverDraft) => void;
  disabled?: boolean;
}) {
  const checks = CLOSE_CHECKS.flatMap((id) =>
    readiness.checks.filter((check) => check.id === id),
  );
  return (
    <ul className="divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card">
      {checks.map((check) => (
        <CheckRow
          key={check.id}
          check={check}
          onOpenTab={onOpenTab}
          waiver={waivers?.[check.id]}
          onWaiver={onWaiver && ((next) => onWaiver(check.id, next))}
          disabled={disabled}
        />
      ))}
    </ul>
  );
}

function CheckRow({
  check,
  onOpenTab,
  waiver,
  onWaiver,
  disabled,
}: {
  check: AiCloseCheck;
  onOpenTab?: (tab: CloseTab) => void;
  waiver: WaiverDraft | undefined;
  onWaiver?: (waiver: WaiverDraft) => void;
  disabled: boolean;
}) {
  const [showAll, setShowAll] = React.useState(false);
  const status = STATUS_ICONS[check.status];
  const label = CLOSE_CHECK_LABELS[check.id];
  const problems = showAll
    ? check.problems
    : check.problems.slice(0, PROBLEMS_SHOWN);
  const hidden = check.problems.length - problems.length;
  const reasonId = `waiver-${check.id}`;
  const problemId = `${reasonId}-problem`;
  // A check holding a running import keeps no waiver: the server drops it.
  const waivable = checkWaivable(check);
  const waiving = waivable && waiver?.on === true;
  const reasonProblem =
    waiving && waiver.reason !== "" ? waiverReasonProblem(waiver.reason) : null;
  const failing = check.status === "failed" || check.status === "waived";

  return (
    <li
      className={cn(
        "flex flex-col gap-2 px-3.5 py-3",
        check.status === "failed" &&
          !waiving &&
          "bg-[#c98500]/[0.06] dark:bg-[#fab219]/[0.06]",
      )}
    >
      <div className="flex flex-wrap items-start gap-x-3 gap-y-1">
        <status.icon
          className={cn("mt-0.5 size-4 shrink-0", status.className)}
          aria-hidden
        />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <p className="text-sm font-semibold">
            <span className="sr-only">{status.label}: </span>
            {label.label}
            {check.status === "waived" && (
              <span className="ml-2 text-xs font-normal text-muted-foreground">
                waived
              </span>
            )}
          </p>
          {!failing && (
            <p className="text-xs text-muted-foreground">
              {check.status === "notApplicable"
                ? NOT_APPLICABLE_NOTE
                : label.passed}
            </p>
          )}
          {failing && problems.length > 0 && (
            <ul className="flex flex-col gap-0.5 text-xs text-muted-foreground">
              {problems.map((problem, index) => (
                <li key={index}>{problemText(problem)}</li>
              ))}
              {hidden > 0 && (
                <li>
                  <button
                    type="button"
                    className="font-medium text-foreground underline-offset-2 hover:underline"
                    onClick={() => setShowAll(true)}
                  >
                    and {hidden} more
                  </button>
                </li>
              )}
            </ul>
          )}
          {check.waiver !== null && (
            <p className="text-xs whitespace-pre-line">
              <span className="font-medium">Waived: </span>
              {check.waiver.reason}
            </p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {failing &&
            checkTargets(check).map((target) => (
              <OpenLink key={target} target={target} onOpenTab={onOpenTab} />
            ))}
          {waivable && onWaiver && (
            <label className="flex items-center gap-2 text-xs font-medium">
              <Switch
                aria-label={`Waive ${label.label}`}
                checked={waiver?.on ?? false}
                disabled={disabled}
                onCheckedChange={(on) =>
                  onWaiver({ on, reason: waiver?.reason ?? "" })
                }
              />
              Waive
            </label>
          )}
          {check.status === "failed" && !waivable && onWaiver && (
            <p className="text-xs text-muted-foreground">
              A running import cannot be waived
            </p>
          )}
        </div>
      </div>
      {waiving && onWaiver && (
        <div className="flex flex-col gap-1 pl-7">
          <label htmlFor={reasonId} className="text-xs font-medium">
            Why the {label.label} check can be waived for this month
          </label>
          <Textarea
            id={reasonId}
            value={waiver.reason}
            disabled={disabled}
            maxLength={MAX_WAIVER_REASON_CHARS * 2}
            onChange={(event) =>
              onWaiver({ on: true, reason: event.target.value })
            }
            placeholder="Such as: a retired machine that can never upload again."
            className="min-h-16 text-sm"
            aria-invalid={reasonProblem !== null}
            aria-describedby={problemId}
          />
          <p
            id={problemId}
            className={cn(
              "text-xs",
              reasonProblem === null
                ? "text-muted-foreground"
                : "text-[#c98500] dark:text-[#fab219]",
            )}
          >
            {reasonProblem ??
              `${[...waiver.reason.trim()].length} of ${MAX_WAIVER_REASON_CHARS} characters, kept with the close.`}
          </p>
        </div>
      )}
    </li>
  );
}

/** Opens where a check's problems are resolved. */
function OpenLink({
  target,
  onOpenTab,
}: {
  target: CloseCheckTarget;
  onOpenTab?: (tab: CloseTab) => void;
}) {
  const className =
    "inline-flex items-center gap-0.5 text-xs font-medium text-foreground underline-offset-2 hover:underline";
  switch (target) {
    case "github-logins":
      return (
        <Link to="/admin/github-logins" className={className}>
          GitHub logins
          <ChevronRight className="size-3.5" aria-hidden />
        </Link>
      );
    case "github-repositories":
      return (
        <Link
          to="/admin/project-mappings"
          hash={GITHUB_REPOSITORIES_ID}
          className={className}
        >
          GitHub repositories
          <ChevronRight className="size-3.5" aria-hidden />
        </Link>
      );
    case "copilot":
    case "data":
      return onOpenTab ? (
        <button
          type="button"
          className={className}
          onClick={() => onOpenTab(target)}
        >
          {target === "copilot" ? "GitHub Copilot" : "Uploads and rates"}
          <ChevronRight className="size-3.5" aria-hidden />
        </button>
      ) : null;
  }
}

/** A closed month: who closed it, its frozen readiness, the re-open and the
 * history of earlier closes. */
function ClosedMonth({
  status,
  close,
}: {
  status: AiMonthCloseStatus;
  close: NonNullable<AiMonthCloseStatus["close"]>;
}) {
  const month = status.month;
  const reopen = aiAdminMutations.useReopenMonth({
    onSuccess: () =>
      toast.success(`${monthLabel(month)} is open again and computed live`),
    onError: apiErrorToast(`Failed to re-open ${monthLabel(month)}`),
  });
  const held = close.heldBy.length > 0;
  const waived = close.readiness.checks.filter(
    (check) => check.status === "waived",
  ).length;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-sm">
        <Lock className="mr-1.5 inline size-4 align-[-3px]" aria-hidden />
        Closed {formatInstant(close.closedAt)}
        {close.closedBy !== null && ` by ${close.closedBy}`} · snapshot #
        {close.closeId}. Changes post as adjustments on{" "}
        {monthLabel(close.adjustmentsPostOn)}&apos;s bill.
      </p>
      <details className="group rounded-xl border border-border/70 bg-card">
        <summary className="flex cursor-pointer list-none items-center gap-2 px-3.5 py-2.5 text-sm font-medium [&::-webkit-details-marker]:hidden">
          <ChevronRight
            className="size-4 transition-transform group-open:rotate-90 motion-reduce:transition-none"
            aria-hidden
          />
          Readiness when it closed
          <span className="font-normal text-muted-foreground">
            {waived === 0
              ? "every check passed"
              : `${plural(waived, "check")} waived`}
            , checked {formatInstant(close.readiness.evaluatedAt)}
          </span>
        </summary>
        <div className="border-t border-border/60 p-2">
          <Checklist readiness={close.readiness} />
        </div>
      </details>
      <div className="flex flex-wrap items-center gap-3">
        <Button
          type="button"
          variant="outline"
          disabled={held || reopen.isPending}
          onClick={() => {
            if (
              window.confirm(
                `Re-open ${monthLabel(month)}? Its bill is computed live again.`,
              )
            ) {
              reopen.mutate({ month });
            }
          }}
        >
          {reopen.isPending ? "Re-opening..." : `Re-open ${monthLabel(month)}`}
        </Button>
        {held && (
          <p className="text-sm text-muted-foreground">
            Adjustments to {monthLabel(month)} are frozen in{" "}
            {listOf(close.heldBy.map(monthLabel), "conjunction")}: re-open
            {close.heldBy.length === 1 ? " that month" : " those months"} first.
          </p>
        )}
      </div>
      <CloseHistory history={status.history} />
    </div>
  );
}

/** Earlier closes of the month, re-opened since. */
function CloseHistory({ history }: { history: AiMonthCloseStatus["history"] }) {
  const reopened = history.filter((entry) => entry.reopenedAt !== null);
  if (reopened.length === 0) {
    return null;
  }
  return (
    <div className="flex flex-col gap-1">
      <h3 className="text-xs font-semibold text-muted-foreground">
        Earlier closes
      </h3>
      <ul className="flex flex-col gap-0.5 text-xs text-muted-foreground">
        {reopened.map((entry) => (
          <li key={entry.closeId}>
            Snapshot #{entry.closeId}: closed {formatInstant(entry.closedAt)}
            {entry.closedBy !== null && ` by ${entry.closedBy}`}, re-opened{" "}
            {entry.reopenedAt !== null && formatInstant(entry.reopenedAt)}
            {entry.reopenedBy !== null && ` by ${entry.reopenedBy}`}.
          </li>
        ))}
      </ul>
    </div>
  );
}
