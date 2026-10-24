import * as React from "react";
import { Link } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type { AiBillingCompleteness } from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { StatusBadge } from "../../-components/status-badge";
import {
  formatGap,
  isMonday,
  monthDays,
  providersWithOwnGaps,
  type CoverageCell,
} from "../../-lib/coverage";
import { formatCount, monthLabel, plural, shortDate } from "../../-lib/format";
import {
  UPLOAD_VERDICTS,
  VERDICT_DISPLAY,
  holdsBillBack,
  type DeveloperUploads,
  type UploadDay,
  type UploadVerdict,
} from "../../-lib/upload-health";
import {
  COVERAGE_COLOR_SCOPE,
  CoverageStrip,
  MISSING_COLOR,
  NOT_REQUIRED_COLOR,
  PARTIAL_COLOR,
  UPLOADED_COLOR,
} from "./coverage-strip";
import { MachineCoverage, ROW_GRID } from "./machine-coverage";

type Scope = "attention" | "all";

/**
 * Whether billing the month can rely on everyone's uploads: one row per
 * developer, grouped by what their missing days most likely mean, worst
 * first. A row opens to each of their machines and tools.
 */
export function UploadsPanel({
  completeness,
  uploads,
}: {
  completeness: AiBillingCompleteness;
  uploads: readonly DeveloperUploads[];
}) {
  const attention = uploads.filter((u) => holdsBillBack(u.verdict));
  const [scope, setScope] = React.useState<Scope>(
    attention.length > 0 ? "attention" : "all",
  );
  const [open, setOpen] = React.useState<ReadonlySet<number>>(new Set());
  const days = React.useMemo(
    () => monthDays(completeness.month),
    [completeness.month],
  );
  const shown = scope === "attention" ? attention : uploads;
  const toggle = (userId: number) =>
    setOpen((previous) => {
      const next = new Set(previous);
      if (!next.delete(userId)) {
        next.add(userId);
      }
      return next;
    });

  return (
    <section
      aria-labelledby="uploads-heading"
      className={cn("flex flex-col gap-3", COVERAGE_COLOR_SCOPE)}
    >
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex max-w-[70ch] flex-col gap-1">
          <h3 id="uploads-heading" className="text-base font-semibold">
            Uploads
          </h3>
          <p className="text-sm text-muted-foreground">
            {completeness.requiredThrough === null
              ? "No day of this month has passed yet, so nothing is required."
              : `Every machine active in ${monthLabel(completeness.month)} must upload each day through ${shortDate(completeness.requiredThrough)}. A sync covers a day once it is over, and a machine without one in ${completeness.staleAfterDays} days is stale.`}
          </p>
        </div>
        {uploads.length > 0 && (
          <div
            role="group"
            aria-label="Developers shown"
            className="inline-flex h-9 items-center rounded-md bg-muted p-1 text-muted-foreground"
          >
            {(
              [
                ["attention", `Needs attention (${attention.length})`],
                ["all", `Everyone (${uploads.length})`],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                aria-pressed={scope === value}
                onClick={() => setScope(value)}
                className={cn(
                  "inline-flex items-center rounded-sm px-3 py-1 text-sm font-medium whitespace-nowrap transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden motion-reduce:transition-none",
                  scope === value && "bg-background text-foreground shadow-xs",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        )}
      </div>

      {shown.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border/80 p-6 text-center text-sm text-muted-foreground">
          {uploads.length === 0
            ? "Nobody has a machine that syncs AI usage or a subscription this month."
            : "Nothing holds the bill back: every developer's required days are in."}
        </p>
      ) : (
        <div className="rounded-xl border border-border/70 bg-card/60 px-4 pb-3">
          <div className="sticky top-0 z-10 -mx-4 border-b border-border/60 bg-card/95 px-4 pt-3 pb-1.5 backdrop-blur-sm">
            <DayAxis days={days} />
          </div>
          {UPLOAD_VERDICTS.map((verdict) => {
            const members = shown.filter((u) => u.verdict === verdict);
            if (members.length === 0) {
              return null;
            }
            return (
              <div key={verdict} className="pt-3">
                <h4 className="pb-1 text-xs font-semibold text-muted-foreground">
                  {VERDICT_DISPLAY[verdict].heading}
                  <span className="ml-1.5 font-normal tabular-nums">
                    {formatCount(members.length)}
                  </span>
                </h4>
                <ul className="divide-y divide-border/50">
                  {members.map((upload) => (
                    <DeveloperRow
                      key={upload.developer.userId}
                      upload={upload}
                      month={completeness.month}
                      days={days}
                      requiredThrough={completeness.requiredThrough}
                      isOpen={open.has(upload.developer.userId)}
                      onToggle={() => toggle(upload.developer.userId)}
                    />
                  ))}
                </ul>
              </div>
            );
          })}
          <ul className="mt-3 flex flex-wrap gap-x-4 gap-y-1 border-t border-border/50 pt-2.5 text-xs text-muted-foreground">
            <Swatch color={UPLOADED_COLOR} label="Uploaded" />
            <Swatch color={PARTIAL_COLOR} label="A tool missing" />
            <Swatch color={MISSING_COLOR} label="Missing" />
            <Swatch outline={NOT_REQUIRED_COLOR} label="Not required" />
          </ul>
        </div>
      )}
    </section>
  );
}

function DeveloperRow({
  upload,
  month,
  days,
  requiredThrough,
  isOpen,
  onToggle,
}: {
  upload: DeveloperUploads;
  month: string;
  days: readonly string[];
  requiredThrough: string | null;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const { developer } = upload;
  const detailsId = `uploads-${developer.userId}`;
  const cells = React.useMemo(
    () => developerCells(upload, days, requiredThrough),
    [upload, days, requiredThrough],
  );
  const hasMachines = developer.machines.length > 0;

  return (
    <li className="py-1.5">
      <div className={ROW_GRID}>
        <div className="flex min-w-0 items-start gap-1">
          <button
            type="button"
            aria-expanded={isOpen}
            aria-controls={detailsId}
            aria-label={`${isOpen ? "Hide" : "Show"} ${developer.fullName}'s machines`}
            disabled={!hasMachines}
            onClick={onToggle}
            className="mt-0.5 -ml-1 rounded-sm p-0.5 text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden disabled:invisible"
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
            <Link
              to="/admin/ai-billing/$userId"
              params={{ userId: String(developer.userId) }}
              search={{ month }}
              className="truncate text-sm font-medium underline-offset-4 hover:underline"
            >
              {developer.fullName}
            </Link>
            <span className="text-xs text-muted-foreground">
              {describe(upload)}
            </span>
          </div>
        </div>
        {upload.verdict === "neverSynced" ? (
          <span className="text-xs text-muted-foreground">
            Nothing to show until a machine syncs.
          </span>
        ) : (
          <CoverageStrip cells={cells} subject={developer.fullName} />
        )}
      </div>
      {isOpen && (
        <div
          id={detailsId}
          className="mt-1.5 ml-5 border-l border-border/60 pl-3"
        >
          {developer.machines.map((machine) => (
            <MachineCoverage
              key={machine.machineId}
              machine={machine}
              days={days}
              requiredThrough={requiredThrough}
              requiredDays={
                requiredThrough === null
                  ? 0
                  : days.filter((day) => day <= requiredThrough).length
              }
            />
          ))}
        </div>
      )}
    </li>
  );
}

const DAY_TEXT: Record<UploadDay, string> = {
  uploaded: "Uploaded from every machine",
  partial: "Uploaded, but a tool is missing",
  missing: "A machine uploaded nothing",
  notRequired: "Not required",
};

/** A developer's merged days as the strip draws them. */
function developerCells(
  upload: DeveloperUploads,
  days: readonly string[],
  requiredThrough: string | null,
): CoverageCell[] {
  return days.map((day, index) => {
    const state = upload.days[index] ?? "notRequired";
    switch (state) {
      case "uploaded":
      case "partial":
        return { day, state, text: DAY_TEXT[state] };
      case "missing":
        return { day, state: "missing", text: DAY_TEXT[state] };
      case "notRequired":
        return {
          day,
          state: "notRequired",
          text:
            requiredThrough !== null && day > requiredThrough
              ? "Not required yet: a sync covers a day once it is over"
              : "No machine active",
        };
    }
  });
}

/** A developer's month of uploads in a line. */
function describe(upload: DeveloperUploads): string {
  const { developer } = upload;
  switch (upload.verdict) {
    case "neverSynced":
      return developer.hasSubscription
        ? "A plan covers the month, but no machine has synced"
        : "Usage on record, but no machine has synced";
    case "noActivity":
      return "No machine was active this month";
    case "complete":
      return withStale(upload, ["Every required day is in"]);
    case "nothingUploaded": {
      const last = upload.runs.at(-1);
      return withStale(upload, [
        last ? `Nothing through ${shortDate(last.to)}` : "Nothing uploaded",
      ]);
    }
    case "holes":
    case "lateStart":
    case "toolGaps":
    case "awaitingSync":
      break;
  }
  const parts: string[] = [];
  const holes = upload.runs.filter((run) => run.kind === "hole");
  if (holes.length > 0) {
    parts.push(`Missing ${holes.map(formatGap).join(", ")}`);
  }
  for (const run of upload.runs) {
    if (run.kind === "beforeFirstUpload") {
      parts.push(`Nothing before ${shortDate(nextDay(run.to))}`);
    }
  }
  if (upload.toolDays > 0) {
    const tools = [
      ...new Set(
        developer.machines
          .filter((machine) => machine.activeInMonth)
          .flatMap(providersWithOwnGaps)
          .map((provider) => AI_PROVIDER_LABELS[provider.provider]),
      ),
    ];
    parts.push(
      `${tools.length > 0 ? tools.join(" and ") : "A tool"} missing on ${plural(upload.toolDays, "day")}`,
    );
  }
  for (const run of upload.runs) {
    if (run.kind === "awaitingSync") {
      parts.push(`${formatGap(run)} waits for a sync`);
    }
  }
  return withStale(upload, parts);
}

function withStale(upload: DeveloperUploads, parts: readonly string[]): string {
  return [
    ...parts,
    ...(upload.staleMachines > 0
      ? [`${plural(upload.staleMachines, "machine")} stale`]
      : []),
  ].join("; ");
}

/** The local calendar day after `day`, `YYYY-MM-DD`. */
function nextDay(day: string): string {
  const [year, month, date] = day.split("-").map(Number);
  const next = new Date(Date.UTC(year, month - 1, date + 1));
  return next.toISOString().slice(0, 10);
}

/** Day numbers above the strips: the first of the month and every Monday. */
function DayAxis({ days }: { days: readonly string[] }) {
  const firstMonday = days.findIndex(isMonday);
  return (
    <div className={ROW_GRID} aria-hidden>
      <span className="hidden text-xs text-muted-foreground sm:block">
        Day of the month
      </span>
      <div className="relative h-4 text-[10px] text-muted-foreground tabular-nums">
        {days.map((day, index) => {
          const monday = isMonday(day);
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

function Swatch({
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

/** A verdict as a badge, for the people table. */
export function VerdictBadge({ verdict }: { verdict: UploadVerdict }) {
  const display = VERDICT_DISPLAY[verdict];
  return <StatusBadge tone={display.tone} label={display.label} />;
}
