import * as React from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  Briefcase,
  ChevronRight,
  CircleDashed,
  Laptop,
  UserRound,
} from "lucide-react";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import type {
  AiCost,
  AiProjectAttribution,
  AiUsageReport,
  AiUsageReportParams,
  AiUsageTotals,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { Segmented } from "@/components/segmented";
import { ChartCard } from "./chart-card";
import { Cost } from "./cost";
import type { FilterPatch } from "./filter-bar";
import { RepoKey } from "./repo-key";
import { addTotals, knownShare, toolTotals } from "../-lib/aggregate";
import { OTHER_COLOR, PROVIDER_COLORS } from "../-lib/colors";
import {
  formatCompact,
  formatCount,
  formatPercent,
  knownUsd,
  modelLabel,
  totalTokens,
} from "../-lib/format";

type Dimension = "project" | "model" | "tool" | "machine";

const DIMENSIONS = [
  { value: "project", label: "Project" },
  { value: "model", label: "Model" },
  { value: "tool", label: "Tool" },
  { value: "machine", label: "Machine" },
] as const satisfies readonly { value: Dimension; label: string }[];

/** What several small rows are called when folded into one. */
const PLURALS: Readonly<Record<Dimension, string>> = {
  project: "projects",
  model: "models",
  tool: "tools",
  machine: "machines",
};

/** A row's share of the known cost below which it folds away. */
const SMALL_SHARE = 0.01;

/** Folding is worth it only when it leaves the list clearly shorter. */
const FOLD_FROM_ROWS = 6;

/** The bar of usage that is no tool's: a project's, or a machine's. */
const PLAIN_COLOR = "hsl(var(--foreground) / 0.55)";

type ProjectKeyUsage = AiUsageReport["projects"][number]["keys"][number];

type Row = Readonly<{
  key: string;
  icon: React.ReactNode;
  label: string;
  /** One line under the label, when the row has no keys to list. */
  detail?: React.ReactNode;
  /** The project keys behind the row, and what they are called together. */
  keys?: Readonly<{
    summary: string;
    items: readonly ProjectKeyUsage[];
    /** Say why each key counts for no project. */
    explain: boolean;
  }>;
  totals: AiUsageTotals;
  color: string;
  /** The filters that show this row alone, and the same filters cleared. */
  filter: FilterPatch;
  clear: FilterPatch;
  selected: boolean;
}>;

/**
 * Where the range's usage went, one dimension at a time: per project, model,
 * tool or machine. Each row is a figure and its bar on the same line, so the
 * list is the chart. Choosing a row filters the whole page to it; small rows
 * fold into one.
 */
export function Breakdown({
  report,
  params,
  machineLabels,
  onFiltersChange,
  className,
}: {
  report: AiUsageReport;
  params: AiUsageReportParams;
  machineLabels: ReadonlyMap<string, string>;
  onFiltersChange: (patch: FilterPatch) => void;
  className?: string;
}) {
  const [dimension, setDimension] = React.useState<Dimension>("project");
  const [open, setOpen] = React.useState<ReadonlySet<string>>(new Set());
  const rows = React.useMemo(
    () => breakdownRows(report, params, machineLabels, dimension),
    [report, params, machineLabels, dimension],
  );
  const total = report.totals.cost;
  const small = rows.filter((row) => isSmall(row, total));
  const folds = rows.length >= FOLD_FROM_ROWS && small.length >= 2;
  const shown = folds ? rows.filter((row) => !isSmall(row, total)) : rows;
  const max = Math.max(0, ...rows.map((row) => knownUsd(row.totals.cost)));
  const toggle = (key: string) =>
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(key)) {
        next.add(key);
      }
      return next;
    });
  const foldKey = `${dimension}:small`;
  const heading =
    DIMENSIONS.find((option) => option.value === dimension)?.label ?? "";

  return (
    <ChartCard
      className={className}
      title="Where it went"
      description="Choose a row to show only its usage on the whole page."
      actions={
        <Segmented
          label="Break down by"
          value={dimension}
          onChange={setDimension}
          options={DIMENSIONS}
        />
      }
    >
      {rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">No usage in this range.</p>
      ) : (
        <table className="w-full table-fixed border-separate border-spacing-0 text-sm">
          <colgroup>
            <col />
            <col className="hidden w-[24%] sm:table-column" />
            <col className="hidden w-16 sm:table-column" />
            <col className="w-32" />
            <col className="w-16" />
          </colgroup>
          <thead>
            <tr className="text-xs text-muted-foreground [&>th]:pb-2 [&>th]:font-medium">
              <th scope="col" className="pl-8 text-left">
                {heading}
              </th>
              <th aria-hidden className="hidden sm:table-cell" />
              <th scope="col" className="hidden text-right sm:table-cell">
                Tokens
              </th>
              <th scope="col" className="text-right">
                Est. cost
              </th>
              <th scope="col" className="text-right">
                {total.kind === "partial" ? (
                  <abbr
                    className="no-underline"
                    title="Share of the known cost; unpriced usage is left out"
                  >
                    Share
                  </abbr>
                ) : (
                  "Share"
                )}
              </th>
            </tr>
          </thead>
          <tbody>
            {shown.map((row) => (
              <BreakdownRow
                key={`${dimension}:${row.key}`}
                row={row}
                total={total}
                max={max}
                open={open.has(`${dimension}:${row.key}`)}
                onToggle={() => toggle(`${dimension}:${row.key}`)}
                onFiltersChange={onFiltersChange}
              />
            ))}
            {folds && (
              <>
                <tr>
                  <NameCell icon={null}>
                    <Expander
                      open={open.has(foldKey)}
                      onToggle={() => toggle(foldKey)}
                      className="text-sm font-medium text-foreground"
                    >
                      {formatCount(small.length)} smaller {PLURALS[dimension]}
                    </Expander>
                    <p className="text-xs text-muted-foreground">
                      each under {formatPercent(SMALL_SHARE)} of the cost
                    </p>
                  </NameCell>
                  <FigureCells
                    totals={small
                      .slice(1)
                      .reduce(
                        (sum, row) => addTotals(sum, row.totals),
                        small[0].totals,
                      )}
                    total={total}
                    max={max}
                    color={OTHER_COLOR}
                  />
                </tr>
                {open.has(foldKey) &&
                  small.map((row) => (
                    <BreakdownRow
                      key={`${dimension}:${row.key}`}
                      row={row}
                      total={total}
                      max={max}
                      open={open.has(`${dimension}:${row.key}`)}
                      onToggle={() => toggle(`${dimension}:${row.key}`)}
                      onFiltersChange={onFiltersChange}
                    />
                  ))}
              </>
            )}
          </tbody>
          <tfoot>
            <tr className="font-semibold [&>td]:border-t [&>td]:border-border/70 [&>td]:pt-3">
              <td className="pl-8">All usage</td>
              <td aria-hidden className="hidden sm:table-cell" />
              <td className="hidden text-right tabular-nums text-muted-foreground sm:table-cell">
                {formatCompact(totalTokens(report.totals.tokens))}
              </td>
              <td className="whitespace-nowrap text-right tabular-nums">
                <Cost cost={total} className="justify-end" />
              </td>
              <td />
            </tr>
          </tfoot>
        </table>
      )}
    </ChartCard>
  );
}

/** Whether a row is too small to earn a line of its own. Usage of unknown
 * cost never is: it may be anything. */
function isSmall(row: Row, total: AiCost): boolean {
  const share = knownShare(row.totals.cost, total);
  return (
    !row.selected &&
    row.totals.cost.kind === "estimated" &&
    share !== null &&
    share < SMALL_SHARE
  );
}

function BreakdownRow({
  row,
  total,
  max,
  open,
  onToggle,
  onFiltersChange,
}: {
  row: Row;
  total: AiCost;
  max: number;
  open: boolean;
  onToggle: () => void;
  onFiltersChange: (patch: FilterPatch) => void;
}) {
  return (
    <>
      <tr className={cn(row.selected && "[&>td]:bg-primary/[0.06]")}>
        <NameCell icon={row.icon} selected={row.selected}>
          <button
            type="button"
            aria-pressed={row.selected}
            title={
              row.selected
                ? `Showing only ${row.label}; choose to show everything again`
                : `Show only ${row.label}`
            }
            onClick={() =>
              onFiltersChange(row.selected ? row.clear : row.filter)
            }
            className="block max-w-full truncate rounded-sm text-left font-medium underline-offset-2 outline-hidden hover:underline focus-visible:ring-2 focus-visible:ring-ring"
          >
            {row.label}
          </button>
          {row.keys ? (
            <Expander open={open} onToggle={onToggle}>
              {row.keys.summary}
            </Expander>
          ) : (
            row.detail && (
              <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                {row.detail}
              </div>
            )
          )}
        </NameCell>
        <FigureCells
          totals={row.totals}
          total={total}
          max={max}
          color={row.color}
          selected={row.selected}
        />
      </tr>
      {row.keys && (
        <tr>
          <td colSpan={5} className="p-0">
            <Reveal open={open}>
              <ul className="grid gap-x-6 gap-y-1 pb-3 pl-8 pr-1 pt-1 text-xs text-muted-foreground sm:grid-cols-2">
                {row.keys.items.map((item) => (
                  <li
                    key={item.projectKey}
                    className="flex min-w-0 items-baseline justify-between gap-3"
                  >
                    <span className="flex min-w-0 items-baseline gap-1.5">
                      <RepoKey
                        projectKey={item.projectKey}
                        className="self-center"
                      />
                      {row.keys?.explain && (
                        <span className="shrink-0 text-[11px]">
                          {unassignedReason(item)}
                        </span>
                      )}
                    </span>
                    <Cost
                      cost={item.totals.cost}
                      className="shrink-0 tabular-nums"
                    />
                  </li>
                ))}
              </ul>
            </Reveal>
          </td>
        </tr>
      )}
    </>
  );
}

function NameCell({
  icon,
  selected = false,
  children,
}: {
  icon: React.ReactNode;
  selected?: boolean;
  children: React.ReactNode;
}) {
  return (
    <td className={cn("py-2 pl-2 pr-3", selected && "rounded-l-lg")}>
      <div className="flex min-w-0 items-start gap-2.5">
        <span className="mt-0.5 flex size-3.5 shrink-0 items-center justify-center text-muted-foreground">
          {icon}
        </span>
        <div className="min-w-0 flex-1">{children}</div>
      </div>
    </td>
  );
}

/** A row's bar, tokens, cost and share of the known cost. */
function FigureCells({
  totals,
  total,
  max,
  color,
  selected = false,
}: {
  totals: AiUsageTotals;
  total: AiCost;
  max: number;
  color: string;
  selected?: boolean;
}) {
  // Usage with no known cost at all has no share of the known cost to show.
  const share =
    totals.cost.kind === "partial" && totals.cost.knownUsd === 0
      ? null
      : knownShare(totals.cost, total);
  return (
    <>
      <td aria-hidden className="hidden pr-4 sm:table-cell">
        <Bar value={knownUsd(totals.cost)} max={max} color={color} />
      </td>
      <td className="hidden text-right tabular-nums text-muted-foreground sm:table-cell">
        {formatCompact(totalTokens(totals.tokens))}
      </td>
      <td className="whitespace-nowrap text-right tabular-nums">
        <Cost cost={totals.cost} className="justify-end" />
      </td>
      <td
        className={cn(
          "pr-2 text-right tabular-nums text-muted-foreground",
          selected && "rounded-r-lg",
        )}
      >
        {share === null ? "–" : formatPercent(share)}
      </td>
    </>
  );
}

/** A bar that grows to its length, and from one length to the next. */
function Bar({
  value,
  max,
  color,
}: {
  value: number;
  max: number;
  color: string;
}) {
  const reduceMotion = useReducedMotion();
  if (value <= 0 || max <= 0) {
    return null;
  }
  return (
    <motion.div
      className="h-2.5 min-w-0.5 rounded-sm"
      style={{ backgroundColor: color }}
      initial={reduceMotion ? false : { width: 0 }}
      animate={{ width: `${(value / max) * 100}%` }}
      transition={{
        duration: reduceMotion ? 0 : 0.5,
        ease: [0.3, 0.7, 0.2, 1],
      }}
    />
  );
}

/** The control that opens what a row folds away. */
function Expander({
  open,
  onToggle,
  className,
  children,
}: {
  open: boolean;
  onToggle: () => void;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-expanded={open}
      onClick={onToggle}
      className={cn(
        "-ml-0.5 flex max-w-full items-center gap-0.5 rounded-sm text-left text-xs text-muted-foreground outline-hidden hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
        className,
      )}
    >
      <ChevronRight
        aria-hidden
        className={cn(
          "size-3.5 shrink-0 transition-transform motion-reduce:transition-none",
          open && "rotate-90",
        )}
      />
      <span className="truncate">{children}</span>
    </button>
  );
}

/** Content that opens and closes in place, at once under reduced motion. */
function Reveal({
  open,
  children,
}: {
  open: boolean;
  children: React.ReactNode;
}) {
  const reduceMotion = useReducedMotion();
  return (
    <AnimatePresence initial={false}>
      {open && (
        <motion.div
          className="overflow-hidden"
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: reduceMotion ? 0 : 0.22, ease: "easeOut" }}
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** Why a project key's usage counts for no project, in a few words. */
function unassignedReason(attribution: AiProjectAttribution): string {
  switch (attribution.status) {
    case "stale":
      return "mapping outside this company";
    case "unconfigured":
      return "time tracking not configured";
    case "unmapped":
      return attribution.mappable ? "not mapped yet" : "cannot be mapped";
    case "unattributed":
      return "no project metadata";
    case "mapped":
    case "personal":
      return "";
  }
}

function repositories(count: number): string {
  return `${formatCount(count)} ${count === 1 ? "repository" : "repositories"}`;
}

/** The rows of one dimension, as the report orders them: costliest first. */
function breakdownRows(
  report: AiUsageReport,
  params: AiUsageReportParams,
  machineLabels: ReadonlyMap<string, string>,
  dimension: Dimension,
): readonly Row[] {
  switch (dimension) {
    case "project": {
      const clear = {
        projectId: undefined,
        unassigned: undefined,
        personal: undefined,
      };
      return report.projects.map((group): Row => {
        const base = { totals: group.totals, clear };
        if (group.project) {
          const [only] = group.keys;
          return {
            ...base,
            key: group.project.projectId,
            icon: <Briefcase aria-hidden className="size-3.5" />,
            label: group.project.projectName,
            ...(group.keys.length === 1 && only
              ? { detail: <RepoKey projectKey={only.projectKey} /> }
              : {
                  keys: {
                    summary: repositories(group.keys.length),
                    items: group.keys,
                    explain: false,
                  },
                }),
            color: PLAIN_COLOR,
            filter: { ...clear, projectId: group.project.projectId },
            selected: params.projectId === group.project.projectId,
          };
        }
        return group.personal
          ? {
              ...base,
              key: "personal",
              icon: <UserRound aria-hidden className="size-3.5" />,
              label: PERSONAL_LABEL,
              keys: {
                summary: `${repositories(group.keys.length)}, left out of the bill`,
                items: group.keys,
                explain: false,
              },
              color: OTHER_COLOR,
              filter: { ...clear, personal: true },
              selected: params.personal === true,
            }
          : {
              ...base,
              key: "unassigned",
              icon: (
                <CircleDashed aria-hidden className="size-3.5 text-primary" />
              ),
              label: "Unassigned",
              keys: {
                summary: `${repositories(group.keys.length)} without a project`,
                items: group.keys,
                explain: true,
              },
              color: OTHER_COLOR,
              filter: { ...clear, unassigned: true },
              selected: params.unassigned === true,
            };
      });
    }
    case "model":
      return report.models.map((entry) => ({
        key: `${entry.provider}:${entry.model}`,
        icon: <AiProviderIcon provider={entry.provider} />,
        label: modelLabel(entry.model),
        detail: `${AI_PROVIDER_LABELS[entry.provider]}, ${formatCount(entry.totals.records)} ${entry.totals.records === 1 ? "request" : "requests"}`,
        totals: entry.totals,
        color: PROVIDER_COLORS[entry.provider],
        filter: { provider: entry.provider, model: entry.model },
        clear: { provider: undefined, model: undefined },
        selected:
          params.provider === entry.provider && params.model === entry.model,
      }));
    case "tool":
      return toolTotals(report).map(({ provider, totals }) => ({
        key: provider,
        icon: <AiProviderIcon provider={provider} />,
        label: AI_PROVIDER_LABELS[provider],
        detail: `${formatCount(totals.records)} ${totals.records === 1 ? "request" : "requests"}`,
        totals,
        color: PROVIDER_COLORS[provider],
        filter: { provider },
        clear: { provider: undefined },
        selected: params.provider === provider,
      }));
    case "machine":
      return [...report.machines]
        .sort((a, b) => knownUsd(b.totals.cost) - knownUsd(a.totals.cost))
        .map((machine) => ({
          key: machine.machineId,
          icon: <Laptop aria-hidden className="size-3.5" />,
          label: machineLabels.get(machine.machineId) ?? "Unknown machine",
          detail: `${formatCount(machine.totals.records)} ${machine.totals.records === 1 ? "request" : "requests"}`,
          totals: machine.totals,
          color: PLAIN_COLOR,
          filter: { machineId: machine.machineId },
          clear: { machineId: undefined },
          selected: params.machineId === machine.machineId,
        }));
  }
}
