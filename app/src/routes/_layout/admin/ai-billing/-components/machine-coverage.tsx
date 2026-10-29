import * as React from "react";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { shownCoverageStatus } from "@/lib/ai-usage";
import type {
  AiMachineCompleteness,
  AiUploadGap,
} from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import {
  formatGap,
  gapDayCount,
  machineCells,
  providerCells,
  providersWithOwnGaps,
  type ProviderCoverage,
} from "../../-lib/coverage";
import { shortDate } from "../../-lib/format";
import { CoverageStrip } from "./coverage-strip";

/*
 * One machine's uploads for a month, as a developer's row in the uploads
 * check opens to. Its strips share the developer row's columns, so the days
 * line up.
 */

/** A label column and a strip column; one column on a narrow screen. */
export const ROW_GRID =
  "grid gap-x-3 gap-y-1 sm:grid-cols-[13rem_minmax(0,1fr)] sm:items-center";

function formatGaps(gaps: readonly AiUploadGap[]): string {
  return gaps.map(formatGap).join(", ");
}

/** What a machine has uploaded of the month, in words. */
function machineSummary(
  machine: AiMachineCompleteness,
  requiredDays: number,
): string {
  if (!machine.activeInMonth) {
    return "No activity this month: nothing required";
  }
  if (requiredDays === 0) {
    return "No day is required yet";
  }
  if (machine.complete) {
    return `All ${requiredDays} days uploaded`;
  }
  const missing = gapDayCount(machine.gaps);
  return missing > 0
    ? `${requiredDays - missing} of ${requiredDays} days · missing ${formatGaps(machine.gaps)}`
    : `All ${requiredDays} days, but not from every tool`;
}

/**
 * One machine's month: what it uploaded in words and day by day, a strip for
 * each tool whose missing days are its own, and every tool's last read.
 */
export function MachineCoverage({
  machine,
  days,
  requiredThrough,
  requiredDays,
}: {
  machine: AiMachineCompleteness;
  days: readonly string[];
  requiredThrough: string | null;
  requiredDays: number;
}) {
  const cells = React.useMemo(
    () => machineCells(machine, days, requiredThrough),
    [machine, days, requiredThrough],
  );
  return (
    <div className="flex flex-col gap-1 py-1.5">
      <div className={ROW_GRID}>
        <div className="min-w-0 text-xs text-muted-foreground tabular-nums">
          <div className="flex flex-wrap items-center gap-1.5 text-sm font-medium text-foreground">
            {machine.label}
            {machine.stale && <StatusBadge tone="warning" label="Stale" />}
          </div>
          <div>{machineSummary(machine, requiredDays)}</div>
          <div>
            Last sync {shortDate(machine.lastSyncedOn)} · token-ledger{" "}
            {machine.clientVersion}
          </div>
        </div>
        <CoverageStrip cells={cells} subject={machine.label} />
      </div>
      {machine.activeInMonth &&
        providersWithOwnGaps(machine).map((provider) => (
          <ProviderStrip
            key={provider.provider}
            machine={machine}
            provider={provider}
            days={days}
            requiredThrough={requiredThrough}
          />
        ))}
      {machine.providers.length > 0 && (
        <div className={ROW_GRID}>
          <span className="hidden sm:block" />
          <div className="flex flex-wrap gap-1">
            {machine.providers.map((provider) => (
              <StatusBadge
                key={provider.provider}
                icon={
                  <AiProviderIcon
                    provider={provider.provider}
                    className="text-muted-foreground"
                    plain
                  />
                }
                tone={
                  provider.gaps.length > 0 || provider.status === "failed"
                    ? "critical"
                    : provider.pricingStatus === "unavailable"
                      ? "warning"
                      : provider.expected
                        ? "good"
                        : "neutral"
                }
                label={providerLabel(provider)}
                className="max-w-full whitespace-normal"
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/** A provider's own days, for one whose gaps are not simply the machine's. */
function ProviderStrip({
  machine,
  provider,
  days,
  requiredThrough,
}: {
  machine: AiMachineCompleteness;
  provider: ProviderCoverage;
  days: readonly string[];
  requiredThrough: string | null;
}) {
  const cells = React.useMemo(
    () => providerCells(machine, provider, days, requiredThrough),
    [machine, provider, days, requiredThrough],
  );
  const label = AI_PROVIDER_LABELS[provider.provider];
  return (
    <div className={ROW_GRID}>
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground sm:pl-3">
        <AiProviderIcon provider={provider.provider} />
        {label}
      </div>
      <CoverageStrip
        cells={cells}
        subject={`${machine.label}, ${label}`}
        thin
      />
    </div>
  );
}

function providerLabel(provider: ProviderCoverage): string {
  const parts = [AI_PROVIDER_LABELS[provider.provider]];
  if (provider.status) {
    parts.push(`last read ${shownCoverageStatus(provider.status)}`);
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
