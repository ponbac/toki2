import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import type { UseQueryResult } from "@tanstack/react-query";
import {
  AlertTriangle,
  Check,
  Globe,
  Info,
  Laptop,
  Plus,
  XCircle,
} from "lucide-react";
import type * as React from "react";
import { AiProviderLabel } from "@/components/ai-provider-icon";
import { Button } from "@/components/ui/button";
import { shownCoverageStatus, type ShownCoverageStatus } from "@/lib/ai-usage";
import type {
  AiUsageMachine,
  AiUsageMachineList,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { Cost } from "./cost";
import { QueryError } from "./query-state";
import { formatShortDate } from "../-lib/format";
import {
  machineFindings,
  type Finding,
  type Severity,
} from "../-lib/machine-findings";
import { MoneyContext, type Money } from "../-lib/money";
import { useDisplayMoney } from "../-lib/use-display-money";

dayjs.extend(relativeTime);

/**
 * The machines that sync the user's usage, one card each: when it last
 * synced, its token-ledger version, how completely it read each tool's
 * history, what may be missing from its figures, and all its usage in the
 * selected dates, which the other filters never narrow.
 */
export function MachinesPanel({
  query,
  onAddMachine,
}: {
  query: UseQueryResult<AiUsageMachineList, Error>;
  onAddMachine: () => void;
}) {
  // The rate of the dates on screen, also while another range loads.
  const { money } = useDisplayMoney(query.data?.to);
  return (
    <section aria-label="Machines" className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <p className="max-w-3xl text-sm text-muted-foreground">
          {query.data
            ? `All usage of each machine from ${formatShortDate(query.data.from)} to ${formatShortDate(query.data.to)}, whatever the other filters. One without a sync for more than ${query.data.staleAfterDays} days is stale: its recent usage is missing, not zero.`
            : "Machines that sync your usage."}
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onAddMachine}
        >
          <Plus />
          Add machine
        </Button>
      </div>
      {query.isPending ? (
        <p className="text-sm text-muted-foreground" role="status">
          Loading machines…
        </p>
      ) : query.isError ? (
        <QueryError
          message="Could not load your machines."
          error={query.error}
          onRetry={() => void query.refetch()}
        />
      ) : (
        <ul className="grid items-start gap-5 sm:grid-cols-2 xl:grid-cols-3">
          {query.data.machines.map((machine) => (
            <MachineItem
              key={machine.machineId}
              machine={machine}
              staleAfterDays={query.data.staleAfterDays}
              money={money}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function MachineItem({
  machine,
  staleAfterDays,
  money,
}: {
  machine: AiUsageMachine;
  staleAfterDays: number;
  /** Null while the exchange rate loads: the cost is left out until then. */
  money: Money | null;
}) {
  const { usage } = machine;
  const findings = machineFindings(machine, staleAfterDays);
  // A tool the machine never had usage of is simply not used there.
  const tools = machine.coverage.filter(
    (entry) => entry.status !== "missing" || entry.hasUsage,
  );

  return (
    <li className="rounded-2xl border border-border/70 bg-card p-5 shadow-xs">
      <div className="flex items-start justify-between gap-3">
        <p className="flex min-w-0 items-center gap-2 font-semibold">
          <Laptop aria-hidden className="size-4 text-muted-foreground" />
          <span className="truncate">{machine.label}</span>
        </p>
        <p className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
          <SyncDot
            stale={machine.stale}
            fresh={dayjs().diff(machine.lastSyncedAt, "hour") < FRESH_HOURS}
          />
          <span className="sr-only">
            {machine.stale ? "Stale. Last sync " : "Last sync "}
          </span>
          <time
            dateTime={machine.lastSyncedAt}
            title={new Date(machine.lastSyncedAt).toLocaleString()}
          >
            {dayjs(machine.lastSyncedAt).fromNow()}
          </time>
        </p>
      </div>
      {usage && money && (
        <MoneyContext.Provider value={money}>
          <p className="mt-3 text-2xl font-semibold tabular-nums tracking-tight">
            <Cost cost={usage.cost} />
          </p>
        </MoneyContext.Provider>
      )}
      <p className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
        <span>token-ledger {machine.clientVersion}</span>
        <span className="flex items-center gap-1">
          <Globe aria-hidden className="size-3" />
          {machine.timeZone}
        </span>
      </p>
      {tools.length > 0 && (
        <ul className="mt-3 flex flex-wrap gap-1.5" aria-label="History read">
          {tools.map((entry) => {
            const status = shownCoverageStatus(entry.status);
            const { icon: Icon, label, className } = COVERAGE[status];
            return (
              <li
                key={entry.provider}
                className={cn(
                  "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs",
                  status === "ok"
                    ? "border-border/70"
                    : status === "failed"
                      ? "border-destructive/40"
                      : "border-primary/40",
                )}
              >
                <AiProviderLabel
                  provider={entry.provider}
                  iconClassName="size-3"
                />
                <Icon aria-hidden className={cn("size-3", className)} />
                <span className="sr-only">{label}</span>
              </li>
            );
          })}
        </ul>
      )}
      {findings.length > 0 && (
        <ul className="mt-3 space-y-1">
          {findings.map((finding) => (
            <FindingItem key={finding.key} finding={finding} />
          ))}
        </ul>
      )}
    </li>
  );
}

/** A sync within this many hours counts as fresh. */
const FRESH_HOURS = 24;

/**
 * How recent the machine's last sync is: a quiet pulse while it is fresh, a
 * still dot after that, and the attention colour once it is stale. It says
 * nothing of whether the machine is syncing right now.
 */
function SyncDot({ stale, fresh }: { stale: boolean; fresh: boolean }) {
  return (
    <span aria-hidden className="relative flex size-2">
      {fresh && !stale && (
        <span className="absolute inset-0 animate-ping rounded-full bg-[#0ca30c] opacity-60 [animation-duration:2.6s] motion-reduce:hidden" />
      )}
      <span
        className={cn(
          "relative size-2 rounded-full",
          stale ? "bg-primary" : "bg-[#0ca30c]",
        )}
      />
    </span>
  );
}

/** How completely a machine read one tool's history, as a mark. */
const COVERAGE: Readonly<
  Record<
    ShownCoverageStatus,
    { icon: React.ElementType; label: string; className: string }
  >
> = {
  ok: { icon: Check, label: "read", className: "text-[#0ca30c]" },
  missing: {
    icon: AlertTriangle,
    label: "no history found",
    className: "text-primary",
  },
  failed: {
    icon: XCircle,
    label: "could not be read",
    className: "text-destructive",
  },
};

const SEVERITY: Readonly<
  Record<
    Severity,
    { icon: React.ElementType; label: string; className: string }
  >
> = {
  critical: {
    icon: XCircle,
    label: "Problem",
    className: "text-destructive",
  },
  warning: {
    icon: AlertTriangle,
    label: "Warning",
    className: "text-primary",
  },
  note: { icon: Info, label: "Note", className: "text-muted-foreground" },
};

function FindingItem({ finding }: { finding: Finding }) {
  const { icon: Icon, label, className } = SEVERITY[finding.severity];
  return (
    <li className="flex items-start gap-2 text-xs">
      <Icon aria-hidden className={cn("mt-px size-3.5 shrink-0", className)} />
      <span>
        <span className="sr-only">{label}: </span>
        {finding.text}
      </span>
    </li>
  );
}
