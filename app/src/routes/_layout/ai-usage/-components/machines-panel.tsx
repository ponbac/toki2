import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import type { UseQueryResult } from "@tanstack/react-query";
import { AlertTriangle, Info, Laptop, XCircle } from "lucide-react";
import type * as React from "react";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiUsageMachine,
  AiUsageMachineList,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { ChartCard } from "./chart-card";
import { QueryError } from "./query-state";
import { formatCost, formatCount, formatShortDate } from "../-lib/format";

dayjs.extend(relativeTime);

type Severity = "critical" | "warning" | "note";

type Finding = Readonly<{ key: string; severity: Severity; text: string }>;

/**
 * The machines that sync the user's usage: when each last synced, its
 * token-ledger version, what may be missing from its figures, and all its
 * usage in the selected dates, which the other filters never narrow.
 */
export function MachinesPanel({
  query,
  className,
}: {
  query: UseQueryResult<AiUsageMachineList, Error>;
  className?: string;
}) {
  return (
    <ChartCard
      className={className}
      title="Machines"
      description={
        query.data
          ? `Machines that sync your usage, with all their usage from ${formatShortDate(query.data.from)} to ${formatShortDate(query.data.to)}, whatever the other filters. One without a sync for more than ${query.data.staleAfterDays} days is stale: its recent usage is missing, not zero.`
          : "Machines that sync your usage."
      }
    >
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
      ) : query.data.machines.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No machine has synced yet. Run <code>token-ledger sync</code> with a
          Toki API token from Account settings to upload your usage.
        </p>
      ) : (
        <ul className="space-y-3">
          {query.data.machines.map((machine) => (
            <MachineItem
              key={machine.machineId}
              machine={machine}
              staleAfterDays={query.data.staleAfterDays}
            />
          ))}
        </ul>
      )}
    </ChartCard>
  );
}

function MachineItem({
  machine,
  staleAfterDays,
}: {
  machine: AiUsageMachine;
  staleAfterDays: number;
}) {
  const { usage } = machine;
  const findings = machineFindings(machine, staleAfterDays);
  if (usage?.cost.kind === "partial") {
    findings.push({
      key: "unpriced",
      severity: "warning",
      text: `${formatCount(usage.cost.unpricedRecords)} of its requests in these dates have no known price, so their cost is unknown.`,
    });
  }
  const reading = machine.coverage
    .filter((entry) => entry.status === "ok")
    .map((entry) => AI_PROVIDER_LABELS[entry.provider]);

  return (
    <li className="rounded-xl border border-border/60 bg-muted/20 p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="flex items-center gap-2 font-medium">
            <Laptop aria-hidden className="size-4 text-muted-foreground" />
            <span className="truncate">{machine.label}</span>
            {machine.stale && (
              <span className="inline-flex shrink-0 items-center gap-1 rounded-full border border-primary/50 px-2 py-0.5 text-[11px] font-medium text-primary">
                <AlertTriangle aria-hidden className="size-3" />
                Stale
              </span>
            )}
          </p>
          <p className="text-xs text-muted-foreground">
            Last sync{" "}
            <time
              dateTime={machine.lastSyncedAt}
              title={new Date(machine.lastSyncedAt).toLocaleString()}
            >
              {dayjs(machine.lastSyncedAt).fromNow()}
            </time>{" "}
            · token-ledger {machine.clientVersion} · {machine.timeZone}
          </p>
        </div>
        {usage && (
          <p className="text-right text-xs text-muted-foreground">
            <span className="font-semibold tabular-nums text-foreground">
              {formatCost(usage.cost)}
            </span>{" "}
            est. in these dates
          </p>
        )}
      </div>
      {findings.length > 0 && (
        <ul className="mt-2 space-y-1">
          {findings.map((finding) => (
            <FindingItem key={finding.key} finding={finding} />
          ))}
        </ul>
      )}
      {reading.length > 0 && (
        <p className="mt-2 text-xs text-muted-foreground">
          Reads {reading.join(", ")} history in full.
        </p>
      )}
    </li>
  );
}

/**
 * What may be missing or unknown in a machine's figures. A provider without
 * history is only worth a note when the machine uploaded its usage before;
 * otherwise the tool is simply not used there.
 */
function machineFindings(
  machine: AiUsageMachine,
  staleAfterDays: number,
): Finding[] {
  const findings: Finding[] = [];
  if (machine.stale) {
    findings.push({
      key: "stale",
      severity: "warning",
      text: `Stale: no sync for more than ${staleAfterDays} days. Usage since then is missing, not zero.`,
    });
  }
  for (const entry of machine.coverage) {
    const tool = AI_PROVIDER_LABELS[entry.provider];
    switch (entry.status) {
      case "failed":
        findings.push({
          key: `${entry.provider}-failed`,
          severity: "critical",
          text: `${tool} history could not be read on the last sync; its stored usage was kept.`,
        });
        break;
      case "partial":
        findings.push({
          key: `${entry.provider}-partial`,
          severity: "warning",
          text: `${tool} history was only partly readable on the last sync.`,
        });
        break;
      case "missing":
        if (entry.hasUsage) {
          findings.push({
            key: `${entry.provider}-missing`,
            severity: "warning",
            text: `No ${tool} history was found on the last sync, though earlier usage is stored.`,
          });
        }
        break;
      case "ok":
        break;
    }
    if (entry.unreadable > 0) {
      findings.push({
        key: `${entry.provider}-unreadable`,
        severity: "warning",
        text: `${formatCount(entry.unreadable)} ${tool} history ${entry.unreadable === 1 ? "file" : "files"} could not be read.`,
      });
    }
    if (entry.malformedLines > 0 || entry.skippedRecords > 0) {
      findings.push({
        key: `${entry.provider}-skipped`,
        severity: "note",
        text: `${tool}: ${formatCount(entry.malformedLines)} malformed lines and ${formatCount(entry.skippedRecords)} records skipped.`,
      });
    }
    if (entry.pricing?.status === "unavailable") {
      findings.push({
        key: `${entry.provider}-pricing`,
        severity: "warning",
        text: `Prices were unavailable for ${tool}, so its costs from this machine are unknown.`,
      });
    }
  }
  return findings;
}

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
