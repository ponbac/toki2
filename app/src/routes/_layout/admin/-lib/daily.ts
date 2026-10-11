import { AI_PROVIDERS, type AiProvider } from "@/lib/api/queries/ai-usage";
import type { AiBillingUsage, AiDayUsage } from "@/lib/api/queries/ai-admin";

/*
 * Aggregation of one developer's day usage for the drill-down. Rows are never
 * finer than a local day: the server sends no hours or sessions.
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
  usage: AiBillingUsage;
};

/** Usage summed per key, largest API-equivalent cost first. */
export function breakdown(
  rows: readonly AiDayUsage[],
  key: (row: AiDayUsage) => string,
  describe: (row: AiDayUsage) => { label: string; detail?: string },
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

/** One day of the chart: API-equivalent USD per provider, and which
 * providers sit at the top and bottom of its stack. */
export type DayPoint = Record<AiProvider, number> & {
  day: string;
  label: string;
  top: AiProvider | null;
  bottom: AiProvider | null;
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

export function dailySeries(
  rows: readonly AiDayUsage[],
  firstDay: string,
  lastDay: string,
): DayPoint[] {
  return daysOf(firstDay, lastDay).map((day) => {
    const point: DayPoint = {
      day,
      label: String(Number(day.slice(8))),
      top: null,
      bottom: null,
      codex: 0,
      claude: 0,
      grok: 0,
      copilot: 0,
    };
    for (const row of rows) {
      if (row.day === day) {
        point[row.provider] += row.usage.apiEquivalentUsd;
      }
    }
    const stacked = AI_PROVIDERS.filter((provider) => point[provider] > 0);
    point.bottom = stacked[0] ?? null;
    point.top = stacked[stacked.length - 1] ?? null;
    return point;
  });
}
