import * as React from "react";
import { barX, crosshair, defineChart, stack } from "@tanstack/charts";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import type {
  AiTokenCounts,
  AiUsageReport,
} from "@/lib/api/queries/ai-usage-report";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
import { ChartCard, ChartTooltipFrame, LegendItem } from "./chart-card";
import { tokenMix } from "../-lib/aggregate";
import { SURFACE_COLOR, TOKEN_COLORS } from "../-lib/colors";
import { formatCompact, formatPercent, totalTokens } from "../-lib/format";

const CATEGORIES = [
  { key: "input", label: "Input" },
  { key: "cacheRead", label: "Cache read" },
  { key: "cacheWrite", label: "Cache write" },
  { key: "output", label: "Output" },
] as const satisfies readonly { key: keyof AiTokenCounts; label: string }[];

type MixRow = Readonly<{ key: string; label: string; tokens: AiTokenCounts }>;

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
  const definition = React.useMemo(() => {
    const segments = rows.flatMap((row) =>
      CATEGORIES.map((category) => ({
        key: `${row.key}:${category.key}`,
        row,
        category: category.key,
        value: row.tokens[category.key],
      })),
    );
    const labels = new Map(rows.map((row) => [row.key, row.label]));

    return defineChart({
      marks: [
        crosshair({
          x: false,
          y: { band: { fill: "hsl(var(--muted) / 0.45)", fillOpacity: 1 } },
        }),
        barX(segments, {
          key: "key",
          x: "value",
          y: (segment) => segment.row.key,
          z: "category",
          color: "category",
          layout: stack({
            order: CATEGORIES.map((category) => category.key),
            offset: "normalize",
          }),
          maxThickness: 20,
          stroke: SURFACE_COLOR,
          strokeWidth: 1,
        }),
      ],
      scales: {
        x: { scale: scaleLinear().domain([0, 1]), axis: false },
        y: {
          scale: () =>
            scaleBand<string>()
              .domain(rows.map((row) => row.key))
              .paddingInner(0.12)
              .paddingOuter(0.12),
          axis: {
            ...USAGE_CHART_AXIS,
            ticks: {
              ...USAGE_CHART_AXIS.ticks,
              format: (key) => labels.get(key) ?? key,
            },
          },
        },
      },
      color: {
        domain: CATEGORIES.map((category) => category.key),
        range: CATEGORIES.map((category) => TOKEN_COLORS[category.key]),
      },
      theme: USAGE_CHART_THEME,
      focus: "group-y",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        formatGroup: (points) => {
          const row = points[0]?.datum.row;
          if (!row) return "";
          const total = totalTokens(row.tokens);
          return [
            row.label,
            ...CATEGORIES.map(
              (category) =>
                `${category.label}: ${formatCompact(row.tokens[category.key])} (${formatPercent(total > 0 ? row.tokens[category.key] / total : 0)})`,
            ),
            `${formatCompact(total)} tokens`,
          ].join("\n");
        },
      },
    });
  }, [rows]);

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
          <Chart
            definition={definition}
            height={rows.length * 34 + 8}
            ariaLabel="Token mix by tool"
            ariaDescription="Each bar shows input, cache read, cache write, and output as a share of its tool's tokens."
            className="w-full"
            renderTooltipBody={({ points }) => {
              const row = points[0]?.datum.row;
              return row ? <MixTooltip row={row} /> : null;
            }}
          />
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

function MixTooltip({ row }: { row: MixRow }) {
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
