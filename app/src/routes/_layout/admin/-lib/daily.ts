import type { AiProvider } from "@/lib/api/queries/ai-usage";
import type { AiBillingUsage, AiDayUsage } from "@/lib/api/queries/ai-admin";

/*
 * Aggregation of one developer's day usage for the drill-down. Rows are never
 * finer than a day: a local day, or a UTC day for a metered payer's usage.
 * The server sends no hours or sessions.
 */

export type UsageFilters = Readonly<{
  provider: AiProvider | null;
  model: string | null;
  machineId: string | null;
  /** A time-tracking project id, or `UNASSIGNED_FILTER`. */
  project: string | null;
}>;

export const UNASSIGNED_FILTER = "unassigned";

export const NO_FILTERS: UsageFilters = {
  provider: null,
  model: null,
  machineId: null,
  project: null,
};

export function projectFilterValue(row: AiDayUsage): string {
  return row.project?.projectId ?? UNASSIGNED_FILTER;
}

export function filterDays(
  rows: readonly AiDayUsage[],
  filters: UsageFilters,
): AiDayUsage[] {
  return rows.filter(
    (row) =>
      (filters.provider === null || row.provider === filters.provider) &&
      (filters.model === null || row.model === filters.model) &&
      (filters.machineId === null || row.machineId === filters.machineId) &&
      (filters.project === null || projectFilterValue(row) === filters.project),
  );
}

export function emptyUsage(): AiBillingUsage {
  return {
    apiEquivalentUsd: 0,
    unpricedRecords: 0,
    records: 0,
    tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, total: 0 },
  };
}

export function addUsage(a: AiBillingUsage, b: AiBillingUsage): AiBillingUsage {
  return {
    apiEquivalentUsd: a.apiEquivalentUsd + b.apiEquivalentUsd,
    unpricedRecords: a.unpricedRecords + b.unpricedRecords,
    records: a.records + b.records,
    tokens: {
      input: a.tokens.input + b.tokens.input,
      cacheRead: a.tokens.cacheRead + b.tokens.cacheRead,
      cacheWrite: a.tokens.cacheWrite + b.tokens.cacheWrite,
      output: a.tokens.output + b.tokens.output,
      total: a.tokens.total + b.tokens.total,
    },
  };
}

export type Breakdown = {
  key: string;
  label: string;
  detail?: string;
  /** Set when the group is one provider's usage. */
  provider?: AiProvider;
  usage: AiBillingUsage;
};

/** Usage summed per key, largest API-equivalent cost first. */
export function breakdown(
  rows: readonly AiDayUsage[],
  key: (row: AiDayUsage) => string,
  describe: (row: AiDayUsage) => Omit<Breakdown, "key" | "usage">,
): Breakdown[] {
  const groups = new Map<string, Breakdown>();
  for (const row of rows) {
    const id = key(row);
    const group = groups.get(id) ?? {
      key: id,
      ...describe(row),
      usage: emptyUsage(),
    };
    groups.set(id, { ...group, usage: addUsage(group.usage, row.usage) });
  }
  return [...groups.values()].sort(
    (a, b) =>
      b.usage.apiEquivalentUsd - a.usage.apiEquivalentUsd ||
      b.usage.tokens.total - a.usage.tokens.total ||
      a.label.localeCompare(b.label),
  );
}

/** One day of the chart: API-equivalent USD per provider. */
export type DayPoint = Record<AiProvider, number> & {
  day: string;
  label: string;
};

/** Every local day from `firstDay` through `lastDay`, `YYYY-MM-DD`. */
export function daysOf(firstDay: string, lastDay: string): string[] {
  const days: string[] = [];
  const [year, month, day] = firstDay.split("-").map(Number);
  const cursor = new Date(Date.UTC(year, month - 1, day));
  for (let guard = 0; guard < 62; guard++) {
    const iso = cursor.toISOString().slice(0, 10);
    days.push(iso);
    if (iso >= lastDay) {
      break;
    }
    cursor.setUTCDate(cursor.getUTCDate() + 1);
  }
  return days;
}

/** What the daily chart stacks by: the provider who paid for the usage, or
 * the tool that made it. */
export const STACK_BY = ["payer", "provider"] as const;
export type StackBy = (typeof STACK_BY)[number];

/** The provider a row's usage stacks under: its payer (the tool itself
 * unless another provider paid for it), or the tool. */
export function stackProvider(row: AiDayUsage, stackBy: StackBy): AiProvider {
  return stackBy === "payer"
    ? (row.billingProvider ?? row.provider)
    : row.provider;
}

/**
 * How each group of rows bills, by `key`: at API prices, by a subscription
 * (named by its plan), from a metered payer's credits, or mixed when its rows
 * bill in more than one way. A group stays mixed once it is, whatever order
 * its rows come in.
 */
export function dayBilling(
  rows: readonly AiDayUsage[],
  key: (row: AiDayUsage) => string,
  plans: ReadonlyMap<number, string>,
  meteredLabel: (row: AiDayUsage) => string,
): Map<string, string> {
  const billing = new Map<string, string>();
  const modes = new Map<string, string>();
  for (const row of rows) {
    const id = key(row);
    if (billing.get(id) === MIXED_BILLING) {
      continue;
    }
    const mode = `${row.billingMode}:${row.subscriptionId ?? ""}`;
    if (modes.has(id) && modes.get(id) !== mode) {
      billing.set(id, MIXED_BILLING);
      continue;
    }
    modes.set(id, mode);
    billing.set(
      id,
      row.billingMode === "metered"
        ? meteredLabel(row)
        : row.subscriptionId === null
          ? "API"
          : `Subscription · ${plans.get(row.subscriptionId) ?? "declared plan"}`,
    );
  }
  return billing;
}

/** A group whose rows bill in more than one way. */
export const MIXED_BILLING = "Mixed billing";

export function dailySeries(
  rows: readonly AiDayUsage[],
  firstDay: string,
  lastDay: string,
  stackBy: StackBy = "provider",
): DayPoint[] {
  return daysOf(firstDay, lastDay).map((day) => {
    const point: DayPoint = {
      day,
      label: String(Number(day.slice(8))),
      codex: 0,
      claude: 0,
      grok: 0,
      copilot: 0,
      pi: 0,
      opencode: 0,
    };
    for (const row of rows) {
      if (row.day === day) {
        point[stackProvider(row, stackBy)] += row.usage.apiEquivalentUsd;
      }
    }
    return point;
  });
}
