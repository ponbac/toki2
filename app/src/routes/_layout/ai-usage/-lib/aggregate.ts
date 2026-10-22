import { AI_PROVIDERS, type AiProvider } from "@/lib/api/contracts/ai-usage";
import type {
  AiCost,
  AiTokenCounts,
  AiUsageReport,
  AiUsageTotals,
} from "@/lib/api/queries/ai-usage-report";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import {
  MODEL_TINTS,
  OTHER_COLOR,
  PROVIDER_COLORS,
  modelColor,
} from "./colors";
import { eachDate } from "./date-range";
import { knownUsd, modelLabel, totalTokens } from "./format";

/**
 * What the daily spend chart can stack by: the provider whose spend it is,
 * the tool that made the requests, or the model.
 */
export const STACK_BY = ["payer", "provider", "model"] as const;

export type StackBy = (typeof STACK_BY)[number];

/** One stacked series of the daily spend chart. */
export type DailySeries = Readonly<{
  key: string;
  label: string;
  color: string;
  /** Set when the series is one provider's usage. */
  provider?: AiProvider;
}>;

/**
 * One day of the daily spend chart: the known cost per series, in the display
 * currency, and what is unknown.
 */
export type DailyRow = Readonly<{
  date: string;
  values: Readonly<Record<string, number>>;
  known: number;
  unpricedRecords: number;
  records: number;
  tokens: number;
}>;

const OTHER_KEY = "other";

/** Whether `a` is more usage than `b`: by known cost, then by requests. */
function outweighs(a: AiUsageTotals, b: AiUsageTotals): boolean {
  const cost = knownUsd(a.cost) - knownUsd(b.cost);
  return cost !== 0 ? cost > 0 : a.records > b.records;
}

/** The tool a model's colour comes from, and its tint among that tool's models. */
export type ModelTint = Readonly<{ provider: AiProvider; rank: number }>;

/**
 * The tints of the models visible under the current filters. A model takes
 * the hue of the tool with most of its usage, and ranks among that tool's
 * visible models by its cost over the whole, unfiltered range, so its hue
 * never changes and its tint only when a costlier sibling is filtered out.
 * Models beyond a tool's `MODEL_TINTS` are absent: they fold into "Other".
 */
export function modelTints(
  report: AiUsageReport,
): ReadonlyMap<string, ModelTint> {
  const owners = new Map<string, AiUsageReport["models"][number]>();
  for (const entry of report.models) {
    const owner = owners.get(entry.model);
    if (!owner || outweighs(entry.totals, owner.totals)) {
      owners.set(entry.model, entry);
    }
  }
  const rank = new Map(
    report.options.models.map((model, index) => [model, index]),
  );
  const ranked = [...owners.keys()].sort(
    (a, b) =>
      (rank.get(a) ?? Number.MAX_SAFE_INTEGER) -
        (rank.get(b) ?? Number.MAX_SAFE_INTEGER) || a.localeCompare(b),
  );
  const tints = new Map<string, ModelTint>();
  const used = new Map<AiProvider, number>();
  for (const model of ranked) {
    const provider = owners.get(model)?.provider;
    if (provider === undefined) {
      continue;
    }
    const next = used.get(provider) ?? 0;
    if (next < MODEL_TINTS) {
      tints.set(model, { provider, rank: next });
      used.set(provider, next + 1);
    }
  }
  return tints;
}

/**
 * The daily spend chart's series and one row per date of the range, days
 * without usage included. Providers keep fixed colours; models take tints of
 * their tool's colour from `modelTints`, stacked tool by tool, and the rest
 * fold into "Other". Amounts are in the display currency, `perUsd` to the
 * dollar, so that the chart's axis has round steps in it.
 */
export function dailySpend(
  report: AiUsageReport,
  stackBy: StackBy,
  perUsd: number,
): Readonly<{ series: readonly DailySeries[]; rows: readonly DailyRow[] }> {
  const tints = modelTints(report);
  const seriesKey = (day: AiUsageReport["days"][number]): string => {
    if (stackBy !== "model") {
      return day[stackBy];
    }
    return tints.has(day.model) ? `model:${day.model}` : OTHER_KEY;
  };

  const present = new Set(report.days.map(seriesKey));
  const series: DailySeries[] =
    stackBy !== "model"
      ? AI_PROVIDERS.filter((provider) => present.has(provider)).map(
          (provider) => ({
            key: provider,
            label: AI_PROVIDER_LABELS[provider],
            color: PROVIDER_COLORS[provider],
            provider,
          }),
        )
      : [...tints.entries()]
          .sort(
            ([, a], [, b]) =>
              AI_PROVIDERS.indexOf(a.provider) -
                AI_PROVIDERS.indexOf(b.provider) || a.rank - b.rank,
          )
          .map(([model, tint]) => ({
            key: `model:${model}`,
            label: modelLabel(model),
            color: modelColor(tint.provider, tint.rank),
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
    entry.values[key] =
      (entry.values[key] ?? 0) + knownUsd(day.totals.cost) * perUsd;
    entry.unpriced +=
      day.totals.cost.kind === "partial" ? day.totals.cost.unpricedRecords : 0;
    entry.records += day.totals.records;
    entry.tokens += totalTokens(day.totals.tokens);
    byDate.set(day.date, entry);
  }

  const rows = eachDate(report).map((date): DailyRow => {
    const entry = byDate.get(date);
    const values = entry?.values ?? {};
    return {
      date,
      values,
      known: Object.values(values).reduce((sum, value) => sum + value, 0),
      unpricedRecords: entry?.unpriced ?? 0,
      records: entry?.records ?? 0,
      tokens: entry?.tokens ?? 0,
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

const EMPTY: AiUsageTotals = {
  tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 },
  records: 0,
  cost: { kind: "estimated", usd: 0 },
};

/** Usage per tool with any, costliest first, summed over its models. */
export function toolTotals(
  report: AiUsageReport,
): readonly Readonly<{ provider: AiProvider; totals: AiUsageTotals }>[] {
  const byProvider = new Map<AiProvider, AiUsageTotals>();
  for (const model of report.models) {
    byProvider.set(
      model.provider,
      addTotals(byProvider.get(model.provider) ?? EMPTY, model.totals),
    );
  }
  return [...byProvider.entries()]
    .map(([provider, totals]) => ({ provider, totals }))
    .sort((a, b) => knownUsd(b.totals.cost) - knownUsd(a.totals.cost));
}

/** How many local days of the report have any usage. */
export function activeDays(report: AiUsageReport): number {
  return new Set(report.days.map((day) => day.date)).size;
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
