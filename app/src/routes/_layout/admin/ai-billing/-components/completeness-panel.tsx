import * as React from "react";
import type { UseQueryResult } from "@tanstack/react-query";
import { ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiBillingCompleteness,
  AiDeveloperCompleteness,
  AiMachineCompleteness,
  AiUploadGap,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { QueryError } from "../../-components/query-error";
import { StatusBadge } from "../../-components/status-badge";
import { monthLabel, shortDate } from "../../-lib/format";

const READINESS = {
  ready: { tone: "good", label: "Ready" },
  incomplete: { tone: "warning", label: "Incomplete" },
  noSync: { tone: "critical", label: "Not synced" },
  noActivity: { tone: "neutral", label: "No activity" },
} as const;

/** Readiness that holds billing back. */
function needsAttention(developer: AiDeveloperCompleteness): boolean {
  return (
    developer.readiness === "noSync" || developer.readiness === "incomplete"
  );
}

/** `Aug 5–19`, or `Aug 31` for one day. */
function formatGap(gap: AiUploadGap): string {
  return gap.from === gap.to
    ? shortDate(gap.from)
    : `${shortDate(gap.from)}–${shortDate(gap.to)}`;
}

function formatGaps(gaps: readonly AiUploadGap[]): string {
  return gaps.map(formatGap).join(", ");
}

/** Whether billing a month can rely on everyone's uploads. */
export function CompletenessPanel({
  query,
}: {
  query: UseQueryResult<AiBillingCompleteness, Error>;
}) {
  const [open, setOpen] = React.useState(false);

  if (query.isPending) {
    return (
      <p className="text-sm text-muted-foreground" role="status">
        Checking data completeness...
      </p>
    );
  }
  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="data completeness"
        onRetry={() => query.refetch()}
      />
    );
  }

  const completeness = query.data;
  const developers = completeness.developers;
  const attention = developers.filter(needsAttention);
  const idle = developers.filter((d) => d.readiness === "noActivity");
  const allReady = developers.length > 0 && attention.length === 0;

  return (
    <section
      aria-labelledby="completeness-heading"
      className={cn(
        "rounded-xl border bg-card/60 p-4",
        allReady ? "border-border/60" : "border-[#c98500]/50",
        query.isPlaceholderData && "opacity-60",
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="space-y-1">
          <h2
            id="completeness-heading"
            className="flex items-center gap-2 text-sm font-semibold"
          >
            Data completeness
            {developers.length === 0 ? (
              <StatusBadge tone="neutral" label="No developers" />
            ) : allReady ? (
              <StatusBadge tone="good" label="Complete" />
            ) : (
              <StatusBadge
                tone="warning"
                label={`${attention.length} of ${developers.length} need attention`}
              />
            )}
          </h2>
          <p className="text-sm text-muted-foreground">
            {completeness.inProgress
              ? `${monthLabel(completeness.month)} is still in progress; its figures will change. `
              : ""}
            {developers.length === 0
              ? "Nobody has a machine that syncs AI usage or a subscription in this month."
              : completeness.requiredThrough === null
                ? "No day of this month has passed yet."
                : `Every machine active in the month must have uploaded every day through ${shortDate(completeness.requiredThrough)}; a sync covers a day only once the day is over. Machines without a sync in ${completeness.staleAfterDays} days are stale.`}
          </p>
          {attention.length > 0 && (
            <ul className="mt-2 space-y-1 text-sm">
              {attention.map((developer) => (
                <li
                  key={developer.userId}
                  className="flex flex-wrap items-center gap-2"
                >
                  <StatusBadge {...READINESS[developer.readiness]} />
                  <span className="font-medium">{developer.fullName}</span>
                  <span className="text-muted-foreground">
                    {attentionReason(developer)}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {idle.length > 0 && (
            <p className="mt-2 text-xs text-muted-foreground">
              No activity this month:{" "}
              {idle.map((developer) => developer.fullName).join(", ")}. Their
              machines neither uploaded usage nor synced after the month began.
            </p>
          )}
        </div>
        {developers.length > 0 && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-expanded={open}
            aria-controls="completeness-machines"
            onClick={() => setOpen((current) => !current)}
          >
            {open ? "Hide machines" : "Show machines"}
            <ChevronDown
              className={cn(
                "size-4 transition-transform",
                open && "rotate-180",
              )}
            />
          </Button>
        )}
      </div>

      {open && (
        <div id="completeness-machines" className="mt-4">
          <MachinesTable developers={developers} />
        </div>
      )}
    </section>
  );
}

/** What is missing for a developer who holds billing back. */
function attentionReason(developer: AiDeveloperCompleteness): string {
  if (developer.machines.length === 0) {
    return developer.hasSubscription
      ? "has a subscription this month but has never synced usage."
      : "has never synced usage.";
  }
  return developer.machines
    .filter((machine) => machine.activeInMonth && !machine.complete)
    .map((machine) => `${machine.label}: ${missingText(machine)}`)
    .join("; ");
}

/** The days a machine has not uploaded, as a sentence fragment. */
function missingText(machine: AiMachineCompleteness): string {
  const everything = formatGaps(machine.gaps);
  const parts =
    machine.gaps.length > 0 ? [`nothing uploaded for ${everything}`] : [];
  for (const provider of machine.providers) {
    const gaps = formatGaps(provider.gaps);
    if (provider.gaps.length > 0 && gaps !== everything) {
      parts.push(
        `${AI_PROVIDER_LABELS[provider.provider]} not uploaded for ${gaps}`,
      );
    }
  }
  return `${parts.join("; ")}.`;
}

function MachinesTable({
  developers,
}: {
  developers: AiBillingCompleteness["developers"];
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Developer</TableHead>
          <TableHead>Machine</TableHead>
          <TableHead>Last sync</TableHead>
          <TableHead>This month</TableHead>
          <TableHead>Providers</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {developers.map((developer) =>
          developer.machines.length === 0 ? (
            <TableRow key={developer.userId}>
              <TableCell className="font-medium">
                <DeveloperCell developer={developer} />
              </TableCell>
              <TableCell colSpan={4} className="text-muted-foreground">
                No machine has synced AI usage.
              </TableCell>
            </TableRow>
          ) : (
            developer.machines.map((machine, index) => (
              <TableRow key={machine.machineId}>
                <TableCell className="align-top font-medium">
                  {index === 0 && <DeveloperCell developer={developer} />}
                </TableCell>
                <TableCell className="align-top">
                  <div>{machine.label}</div>
                  <div className="text-xs text-muted-foreground">
                    token-ledger {machine.clientVersion}
                  </div>
                </TableCell>
                <TableCell className="align-top">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="tabular-nums">
                      {shortDate(machine.lastSyncedOn)}
                    </span>
                    {machine.stale && (
                      <StatusBadge tone="warning" label="Stale" />
                    )}
                  </div>
                </TableCell>
                <TableCell className="align-top">
                  <MachineMonth machine={machine} />
                </TableCell>
                <TableCell className="align-top">
                  <div className="flex flex-col items-start gap-1">
                    {machine.providers.map((provider) => (
                      <StatusBadge
                        key={provider.provider}
                        tone={
                          provider.gaps.length > 0 ||
                          provider.status === "failed"
                            ? "critical"
                            : provider.status === "partial" ||
                                provider.pricingStatus === "unavailable"
                              ? "warning"
                              : provider.expected
                                ? "good"
                                : "neutral"
                        }
                        label={providerLabel(provider)}
                      />
                    ))}
                  </div>
                </TableCell>
              </TableRow>
            ))
          ),
        )}
      </TableBody>
    </Table>
  );
}

function providerLabel(
  provider: AiMachineCompleteness["providers"][number],
): string {
  const parts = [AI_PROVIDER_LABELS[provider.provider]];
  if (provider.status) {
    parts.push(`last read ${provider.status}`);
  }
  if (provider.pricingStatus === "unavailable") {
    parts.push("no prices");
  }
  if (provider.gaps.length > 0) {
    parts.push(`missing ${formatGaps(provider.gaps)}`);
  } else if (!provider.expected) {
    parts.push("not in use");
  }
  return parts.join(" · ");
}

function DeveloperCell({ developer }: { developer: AiDeveloperCompleteness }) {
  return (
    <div className="flex flex-col items-start gap-1">
      <span>{developer.fullName}</span>
      <StatusBadge {...READINESS[developer.readiness]} />
    </div>
  );
}

function MachineMonth({ machine }: { machine: AiMachineCompleteness }) {
  if (!machine.activeInMonth) {
    return <StatusBadge tone="neutral" label="No activity" />;
  }
  if (machine.complete) {
    return <StatusBadge tone="good" label="All days uploaded" />;
  }
  return (
    <div className="flex flex-col items-start gap-1">
      <StatusBadge tone="critical" label="Days missing" />
      <span className="text-xs text-muted-foreground">
        {missingText(machine)}
      </span>
    </div>
  );
}
