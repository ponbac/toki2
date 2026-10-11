import * as React from "react";
import {
  Bar,
  BarChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipProps,
} from "recharts";
import type {
  AiTokenCounts,
  AiUsageReport,
} from "@/lib/api/queries/ai-usage-report";
import { ChartCard, ChartTooltipFrame, LegendItem } from "./chart-card";
import { tokenMix } from "../-lib/aggregate";
import { AXIS_TICK, SURFACE_COLOR, TOKEN_COLORS } from "../-lib/colors";
import { formatCompact, formatPercent, totalTokens } from "../-lib/format";

const CATEGORIES = [
  { key: "input", label: "Input" },
  { key: "cacheRead", label: "Cache read" },
  { key: "cacheWrite", label: "Cache write" },
  { key: "output", label: "Output" },
] as const satisfies readonly { key: keyof AiTokenCounts; label: string }[];

type MixRow = Readonly<{ label: string; tokens: AiTokenCounts }>;

/**
 * The share of input, cache read, cache write and output tokens, overall and
 * per tool when more than one tool has usage.
 */
export function TokenMixChart({
  report,
  className,
}: {
  report: AiUsageReport;
  className?: string;
}) {
  const rows = React.useMemo(() => tokenMix(report), [report]);
  const overall = report.totals.tokens;
  const total = totalTokens(overall);

  return (
    <ChartCard
      className={className}
      title="Token mix"
      description="Shares of each token category. Input excludes cache reads and writes; reasoning counts as output."
    >
      {total === 0 ? (
        <p className="text-sm text-muted-foreground">
          No recorded tokens in this range.
        </p>
      ) : (
        <>
          <div style={{ height: rows.length * 34 + 8 }} className="w-full">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart
                data={rows.map((row) => ({ ...row }))}
                layout="vertical"
                stackOffset="expand"
                margin={{ top: 0, right: 8, bottom: 0, left: 0 }}
                barCategoryGap={6}
              >
                <XAxis type="number" hide />
                <YAxis
                  type="category"
                  dataKey="label"
                  width={96}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={false}
                />
                <Tooltip
                  cursor={{ fill: "hsl(var(--muted) / 0.45)" }}
                  content={<MixTooltip />}
                />
                {CATEGORIES.map((category) => (
                  <Bar
                    key={category.key}
                    dataKey={(row: MixRow) => row.tokens[category.key]}
                    name={category.label}
                    stackId="mix"
                    fill={TOKEN_COLORS[category.key]}
                    stroke={SURFACE_COLOR}
                    strokeWidth={1}
                    maxBarSize={20}
                    isAnimationActive={false}
                  />
                ))}
              </BarChart>
            </ResponsiveContainer>
          </div>
          <ul className="flex flex-wrap gap-x-4 gap-y-1">
            {CATEGORIES.map((category) => (
              <LegendItem
                key={category.key}
                color={TOKEN_COLORS[category.key]}
                label={
                  <>
                    {category.label}{" "}
                    <span className="font-semibold tabular-nums text-foreground">
                      {formatCompact(overall[category.key])}
                    </span>{" "}
                    ({formatPercent(overall[category.key] / total)})
                  </>
                }
              />
            ))}
          </ul>
        </>
      )}
    </ChartCard>
  );
}

function MixTooltip({ active, payload }: TooltipProps<number, string>) {
  const row = payload?.[0]?.payload;
  if (!active || !isMixRow(row)) {
    return null;
  }
  const total = totalTokens(row.tokens);
  return (
    <ChartTooltipFrame
      title={row.label}
      rows={CATEGORIES.map((category) => ({
        key: category.key,
        color: TOKEN_COLORS[category.key],
        value: formatCompact(row.tokens[category.key]),
        label: `${category.label} · ${formatPercent(total > 0 ? row.tokens[category.key] / total : 0)}`,
      }))}
      footer={`${formatCompact(total)} tokens`}
    />
  );
}

function isMixRow(value: unknown): value is MixRow {
  return (
    typeof value === "object" &&
    value !== null &&
    "label" in value &&
    "tokens" in value
  );
}
