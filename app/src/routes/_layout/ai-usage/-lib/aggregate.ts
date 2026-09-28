import { AI_PROVIDERS, type AiProvider } from "@/lib/api/contracts/ai-usage";
import type {
  AiCost,
  AiTokenCounts,
  AiUsageReport,
  AiUsageTotals,
} from "@/lib/api/queries/ai-usage-report";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import {
  MODEL_SLOTS,
  OTHER_COLOR,
  PROVIDER_COLORS,
  modelColor,
} from "./colors";
import { eachDate } from "./date-range";
import { knownUsd, modelLabel, totalTokens } from "./format";

/** What the daily spend chart stacks by. */
export type StackBy = "provider" | "model";

/** One stacked series of the daily spend chart. */
export type DailySeries = Readonly<{
  key: string;
  label: string;
  color: string;
}>;

/** One day of the daily spend chart: known USD per series, and what is unknown. */
export type DailyRow = Readonly<{
  date: string;
  values: Readonly<Record<string, number>>;
  knownUsd: number;
  unpricedRecords: number;
  records: number;
  tokens: number;
  /** The topmost series with spend, whose segment gets the rounded end. */
  top: string | null;
}>;

const OTHER_KEY = "other";

/**
 * Colour slots for the models visible under the current filters, ranked by
 * their cost over the whole, unfiltered range. A model keeps its unfiltered
 * slot when that is one of the first `MODEL_SLOTS`, so colours stay put where
 * they can; lower-ranked models take the slots left free, in rank order. Only
 * models beyond every free slot fold into "Other".
 */
export function modelSlots(
  visible: readonly string[],
  unfilteredOrder: readonly string[],
): ReadonlyMap<string, number> {
  const rank = new Map(unfilteredOrder.map((model, index) => [model, index]));
  const ranked = [...new Set(visible)].sort(
    (a, b) =>
      (rank.get(a) ?? Number.MAX_SAFE_INTEGER) -
        (rank.get(b) ?? Number.MAX_SAFE_INTEGER) || a.localeCompare(b),
  );
  const slots = new Map<string, number>();
  const taken = new Set<number>();
  for (const model of ranked) {
    const preferred = rank.get(model);
    if (preferred !== undefined && preferred < MODEL_SLOTS) {
      slots.set(model, preferred);
      taken.add(preferred);
    }
  }
  const free = Array.from({ length: MODEL_SLOTS }, (_, slot) => slot).filter(
    (slot) => !taken.has(slot),
  );
  for (const model of ranked) {
    if (!slots.has(model)) {
      const slot = free.shift();
      if (slot === undefined) {
        break;
      }
      slots.set(model, slot);
    }
  }
  return slots;
}

/**
 * The daily spend chart's series and one row per date of the range, days
 * without usage included. Providers keep fixed colours; models take slots
 * from `modelSlots`, and the rest fold into "Other".
 */
export function dailySpend(
  report: AiUsageReport,
  stackBy: StackBy,
): Readonly<{ series: readonly DailySeries[]; rows: readonly DailyRow[] }> {
  const slots = modelSlots(
    report.days.map((day) => day.model),
    report.options.models,
  );
  const seriesKey = (day: AiUsageReport["days"][number]): string => {
    if (stackBy === "provider") {
      return day.provider;
    }
    return slots.has(day.model) ? `model:${day.model}` : OTHER_KEY;
  };

  const present = new Set(report.days.map(seriesKey));
  const series: DailySeries[] =
    stackBy === "provider"
      ? AI_PROVIDERS.filter((provider) => present.has(provider)).map(
          (provider) => ({
            key: provider,
            label: AI_PROVIDER_LABELS[provider],
            color: PROVIDER_COLORS[provider],
          }),
        )
      : [...slots.entries()]
          .sort(([, a], [, b]) => a - b)
          .map(([model, slot]) => ({
            key: `model:${model}`,
            label: modelLabel(model),
            color: modelColor(slot),
          }));
  if (present.has(OTHER_KEY)) {
    series.push({ key: OTHER_KEY, label: "Other models", color: OTHER_COLOR });
  }

  const byDate = new Map<
    string,
    {
      values: Record<string, number>;
      unpriced: number;
      records: number;
      tokens: number;
    }
  >();
  for (const day of report.days) {
    const entry = byDate.get(day.date) ?? {
      values: {},
      unpriced: 0,
      records: 0,
      tokens: 0,
    };
    const key = seriesKey(day);
    entry.values[key] = (entry.values[key] ?? 0) + knownUsd(day.totals.cost);
    entry.unpriced +=
      day.totals.cost.kind === "partial" ? day.totals.cost.unpricedRecords : 0;
    entry.records += day.totals.records;
    entry.tokens += totalTokens(day.totals.tokens);
    byDate.set(day.date, entry);
  }

  const rows = eachDate(report).map((date): DailyRow => {
    const entry = byDate.get(date);
    const values = entry?.values ?? {};
    const top =
      [...series].reverse().find((entry) => (values[entry.key] ?? 0) > 0)
        ?.key ?? null;
    return {
      date,
      values,
      knownUsd: Object.values(values).reduce((sum, value) => sum + value, 0),
      unpricedRecords: entry?.unpriced ?? 0,
      records: entry?.records ?? 0,
      tokens: entry?.tokens ?? 0,
      top,
    };
  });

  return { series, rows };
}

/** Adds two costs: known only when both are. */
export function addCosts(a: AiCost, b: AiCost): AiCost {
  if (a.kind === "estimated" && b.kind === "estimated") {
    return { kind: "estimated", usd: a.usd + b.usd };
  }
  const unpriced = (cost: AiCost) =>
    cost.kind === "partial" ? cost.unpricedRecords : 0;
  return {
    kind: "partial",
    knownUsd: knownUsd(a) + knownUsd(b),
    unpricedRecords: unpriced(a) + unpriced(b),
  };
}

/** Adds token counts category by category. */
export function addTokens(a: AiTokenCounts, b: AiTokenCounts): AiTokenCounts {
  return {
    input: a.input + b.input,
    cacheRead: a.cacheRead + b.cacheRead,
    cacheWrite: a.cacheWrite + b.cacheWrite,
    output: a.output + b.output,
  };
}

/** Adds usage totals. */
export function addTotals(a: AiUsageTotals, b: AiUsageTotals): AiUsageTotals {
  return {
    tokens: addTokens(a.tokens, b.tokens),
    records: a.records + b.records,
    cost: addCosts(a.cost, b.cost),
  };
}

/** One bar of the per-project chart. */
export type ProjectBar = Readonly<{
  key: string;
  label: string;
  /** Unassigned usage, or several small projects folded together. */
  deemphasised: boolean;
  totals: AiUsageTotals;
}>;

/** What the per-project chart measures. */
export type ProjectMetric = "cost" | "tokens";

/**
 * The per-project chart's bars, largest first by `metric`: at most `limit`
 * projects, with the rest folded into one bar. Unassigned usage is one bar.
 */
export function projectBars(
  report: AiUsageReport,
  metric: ProjectMetric,
  limit = 8,
): readonly ProjectBar[] {
  const measure = (totals: AiUsageTotals) =>
    metric === "cost" ? knownUsd(totals.cost) : totalTokens(totals.tokens);
  const bars = report.projects
    .map(
      (group): ProjectBar => ({
        key: group.project?.projectId ?? "unassigned",
        label: group.project?.projectName ?? "Unassigned",
        deemphasised: group.project === null,
        totals: group.totals,
      }),
    )
    .sort((a, b) => measure(b.totals) - measure(a.totals));
  if (bars.length <= limit) {
    return bars;
  }

  const rest = bars.slice(limit - 1);
  const folded = rest
    .slice(1)
    .reduce((sum, bar) => addTotals(sum, bar.totals), rest[0]?.totals ?? EMPTY);
  return [
    ...bars.slice(0, limit - 1),
    {
      key: "other-projects",
      label: `${rest.length} more`,
      deemphasised: true,
      totals: folded,
    },
  ];
}

const EMPTY: AiUsageTotals = {
  tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 },
  records: 0,
  cost: { kind: "estimated", usd: 0 },
};

/** Token counts overall and per provider with usage, in provider order. */
export function tokenMix(
  report: AiUsageReport,
): readonly Readonly<{ key: string; label: string; tokens: AiTokenCounts }>[] {
  const byProvider = new Map<AiProvider, AiTokenCounts>();
  for (const model of report.models) {
    byProvider.set(
      model.provider,
      addTokens(
        byProvider.get(model.provider) ?? EMPTY.tokens,
        model.totals.tokens,
      ),
    );
  }
  const providers = AI_PROVIDERS.flatMap((provider) => {
    const tokens = byProvider.get(provider);
    return tokens
      ? [{ key: provider, label: AI_PROVIDER_LABELS[provider], tokens }]
      : [];
  });
  return providers.length > 1
    ? [{ key: "all", label: "All", tokens: report.totals.tokens }, ...providers]
    : providers;
}

/** One cell of the hour-of-week heatmap. */
export type HourCell = Readonly<{
  weekday: number;
  hour: number;
  totals: AiUsageTotals | null;
}>;

/** The 7 × 24 heatmap cells, Monday first, with null for hours without usage. */
export function hourGrid(
  report: AiUsageReport,
): readonly (readonly HourCell[])[] {
  const cells = new Map(
    report.hours.map((hour) => [`${hour.weekday}:${hour.hour}`, hour.totals]),
  );
  return Array.from({ length: 7 }, (_, day) =>
    Array.from({ length: 24 }, (_, hour) => ({
      weekday: day + 1,
      hour,
      totals: cells.get(`${day + 1}:${hour}`) ?? null,
    })),
  );
}

/** The share of `part` in `whole` known cost, or null without known cost. */
export function knownShare(part: AiCost, whole: AiCost): number | null {
  const total = knownUsd(whole);
  return total > 0 ? knownUsd(part) / total : null;
}
