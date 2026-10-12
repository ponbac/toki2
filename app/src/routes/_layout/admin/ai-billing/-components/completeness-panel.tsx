import * as React from "react";
import type { UseQueryResult } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { AiProviderIcon } from "@/components/ai-provider-icon";
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
import {
  coverageTotals,
  gapDayCount,
  isMonday,
  machineCells,
  monthDays,
  needsAttention,
  providerCells,
  providersWithOwnGaps,
  type CoverageTotals,
  type ProviderCoverage,
} from "../../-lib/coverage";
import { formatCount, monthLabel, plural, shortDate } from "../../-lib/format";
import {
  COVERAGE_COLOR_SCOPE,
  CoverageStrip,
  MISSING_COLOR,
  NOT_REQUIRED_COLOR,
  UPLOADED_COLOR,
} from "./coverage-strip";

const READINESS = {
  ready: { tone: "good", label: "Ready" },
  incomplete: { tone: "warning", label: "Incomplete" },
  noSync: { tone: "critical", label: "Not synced" },
  noActivity: { tone: "neutral", label: "No activity" },
} as const;

/** Whose machines the panel lists. */
type Scope = "attention" | "all";

/** A filter, not tabs: both choices show the same panel. */
const SCOPES = [
  { value: "attention", label: "Needs attention" },
  { value: "all", label: "Everyone" },
] as const satisfies readonly { value: Scope; label: string }[];

/** A label column and a strip column; one column on a narrow screen. */
const ROW_GRID =
  "grid gap-x-3 gap-y-1 sm:grid-cols-[13rem_minmax(0,1fr)] sm:items-center";

/** `Aug 5–19`, or `Aug 31` for one day. */
function formatGap(gap: AiUploadGap): string {
  return gap.from === gap.to
    ? shortDate(gap.from)
    : `${shortDate(gap.from)}–${shortDate(gap.to)}`;
}

function formatGaps(gaps: readonly AiUploadGap[]): string {
  return gaps.map(formatGap).join(", ");
}

/**
 * Whether billing a month can rely on everyone's uploads, day by day: what
 * the Uploads check opens to. The check's row carries the verdict.
 */
export function CompletenessPanel({
  query,
}: {
  query: UseQueryResult<AiBillingCompleteness, Error>;
}) {
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
  return <Completeness completeness={query.data} />;
}

function Completeness({
  completeness,
}: {
  completeness: AiBillingCompleteness;
}) {
  const [scope, setScope] = React.useState<Scope>("attention");
  const days = React.useMemo(
    () => monthDays(completeness.month),
    [completeness.month],
  );
  const totals = React.useMemo(
    () => coverageTotals(completeness, days),
    [completeness, days],
  );
  const developers = completeness.developers;
  const attention = developers.filter(needsAttention);
  const idle = developers.filter((d) => d.readiness === "noActivity");
  const allReady = developers.length > 0 && attention.length === 0;
  const shown = scope === "attention" ? attention : developers;

  return (
    <div className={cn("flex flex-col gap-3", COVERAGE_COLOR_SCOPE)}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1 basis-64">
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
        </div>
        {developers.length > 0 && (
          <div
            role="group"
            aria-label="Developers shown"
            className="inline-flex h-10 items-center rounded-md bg-muted p-1 text-muted-foreground"
          >
            {SCOPES.map((option) => (
              <button
                key={option.value}
                type="button"
                aria-pressed={scope === option.value}
                onClick={() => setScope(option.value)}
                className={cn(
                  "inline-flex items-center justify-center whitespace-nowrap rounded-sm px-3 py-1.5 text-sm font-medium ring-offset-background transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                  scope === option.value &&
                    "bg-background text-foreground shadow-sm",
                )}
              >
                {option.label}
              </button>
            ))}
          </div>
        )}
      </div>

      {totals.required > 0 && <CoverageSummary totals={totals} />}

      {shown.length > 0 ? (
        <div className="flex flex-col">
          <DayAxis days={days} />
          {shown.map((developer) => (
            <DeveloperCoverage
              key={developer.userId}
              developer={developer}
              month={completeness.month}
              days={days}
              requiredThrough={completeness.requiredThrough}
              requiredDays={totals.requiredDays}
            />
          ))}
          <ul className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
            <LegendSwatch color={UPLOADED_COLOR} label="Uploaded" />
            <LegendSwatch color={MISSING_COLOR} label="Missing" />
            <LegendSwatch outline={NOT_REQUIRED_COLOR} label="Not required" />
          </ul>
        </div>
      ) : (
        allReady && (
          <p className="text-sm text-muted-foreground">
            Every active machine has uploaded every required day.
          </p>
        )
      )}

      {idle.length > 0 && (
        <p className="text-xs text-muted-foreground">
          No activity this month:{" "}
          {idle.map((developer) => developer.fullName).join(", ")}. Their
          machines neither uploaded usage nor synced after the month began.
        </p>
      )}
    </div>
  );
}

/** How many of the month's required machine-days are in, with its denominator. */
function CoverageSummary({ totals }: { totals: CoverageTotals }) {
  const uploaded = totals.required - totals.missing;
  return (
    <div>
      <p className="text-sm tabular-nums">
        <span className="text-lg font-semibold">
          {formatCount(uploaded)} of {formatCount(totals.required)}
        </span>{" "}
        required machine-days uploaded
      </p>
      <p className="text-xs tabular-nums text-muted-foreground">
        {plural(totals.machines, "active machine")} ×{" "}
        {plural(totals.requiredDays, "required day")}
        {totals.providerOnly > 0 &&
          ` · on ${formatCount(totals.providerOnly)} more, a tool is missing`}
      </p>
      <div
        role="meter"
        aria-label="Required machine-days uploaded"
        aria-valuemin={0}
        aria-valuemax={totals.required}
        aria-valuenow={uploaded}
        className="mt-1.5 h-1.5 overflow-hidden rounded-full"
        style={{
          backgroundColor: `color-mix(in srgb, ${MISSING_COLOR} 35%, hsl(var(--muted)))`,
        }}
      >
        <div
          className="h-full"
          style={{
            width: `${(uploaded / totals.required) * 100}%`,
            backgroundColor: "var(--viz-1)",
          }}
        />
      </div>
    </div>
  );
}

/** Day numbers above the strips: the first of the month and every Monday. */
function DayAxis({ days }: { days: readonly string[] }) {
  const firstMonday = days.findIndex(isMonday);
  return (
    <div className={ROW_GRID} aria-hidden>
      <span className="hidden sm:block" />
      <div className="relative h-4 text-[10px] tabular-nums text-muted-foreground">
        {days.map((day, index) => {
          const monday = isMonday(day);
          // The first of the month gives way to a Monday right beside it.
          if (!monday && (index !== 0 || firstMonday <= 2)) {
            return null;
          }
          return (
            <span
              key={day}
              className="absolute top-0 whitespace-nowrap"
              style={{ left: `${(index / days.length) * 100}%` }}
            >
              {monday && <span className="hidden sm:inline">Mon </span>}
              {index + 1}
            </span>
          );
        })}
      </div>
    </div>
  );
}

function LegendSwatch({
  color,
  outline,
  label,
}: {
  color?: string;
  outline?: string;
  label: string;
}) {
  return (
    <li className="flex items-center gap-1.5">
      <span
        aria-hidden
        className="size-2.5 shrink-0 rounded-[2px]"
        style={
          outline
            ? { boxShadow: `inset 0 0 0 1px ${outline}` }
            : { backgroundColor: color }
        }
      />
      {label}
    </li>
  );
}

function DeveloperCoverage({
  developer,
  month,
  days,
  requiredThrough,
  requiredDays,
}: {
  developer: AiDeveloperCompleteness;
  month: string;
  days: readonly string[];
  requiredThrough: string | null;
  requiredDays: number;
}) {
  return (
    <div className="mt-2 border-t border-border/50 pt-2.5">
      <div className="flex flex-wrap items-center gap-2 text-sm font-semibold">
        <Link
          to="/admin/ai-billing/$userId"
          params={{ userId: String(developer.userId) }}
          search={{ month }}
          className="underline-offset-4 hover:underline"
        >
          {developer.fullName}
        </Link>
        <StatusBadge {...READINESS[developer.readiness]} />
      </div>
      {developer.machines.length === 0 ? (
        <p className="mt-1 text-sm text-muted-foreground">
          {developer.hasSubscription
            ? "No machine has synced AI usage, though a subscription covers this month."
            : "No machine has synced AI usage."}
        </p>
      ) : (
        developer.machines.map((machine) => (
          <MachineCoverage
            key={machine.machineId}
            machine={machine}
            days={days}
            requiredThrough={requiredThrough}
            requiredDays={requiredDays}
          />
        ))
      )}
    </div>
  );
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

function MachineCoverage({
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
        <div className="min-w-0 text-xs tabular-nums text-muted-foreground">
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
                    : provider.status === "partial" ||
                        provider.pricingStatus === "unavailable"
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
