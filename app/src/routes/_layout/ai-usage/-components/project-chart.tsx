import * as React from "react";
import { barX, crosshair, defineChart, text } from "@tanstack/charts";
import { decorative } from "@tanstack/charts/mark/decorative";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import type { AiUsageReport } from "@/lib/api/queries/ai-usage-report";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
import { ChartCard, ChartTooltipFrame, Segmented } from "./chart-card";
import {
  projectBars,
  type ProjectBar,
  type ProjectMetric,
} from "../-lib/aggregate";
import { OTHER_COLOR } from "../-lib/colors";
import {
  formatCompact,
  formatCount,
  knownUsd,
  totalTokens,
} from "../-lib/format";
import { formatCost, useMoney } from "../-lib/money";

const PROJECT_COLOR = "var(--viz-1)";

/**
 * Spend or tokens per time-tracking project, largest first, with unassigned
 * usage as one grey bar. Spend bars show the known cost; "?" marks an unknown
 * remainder.
 */
export function ProjectChart({
  report,
  className,
}: {
  report: AiUsageReport;
  className?: string;
}) {
  const [metric, setMetric] = React.useState<ProjectMetric>("cost");
  const money = useMoney();
  const bars = React.useMemo(
    () => projectBars(report, metric),
    [report, metric],
  );
  const definition = React.useMemo(() => {
    const measure = (bar: ProjectBar) =>
      metric === "cost"
        ? knownUsd(bar.totals.cost) * money.perUsd
        : totalTokens(bar.totals.tokens);
    const labels = new Map(bars.map((bar) => [bar.key, bar.label]));

    return defineChart(({ width }) => ({
      marks: [
        crosshair({
          x: false,
          y: { band: { fill: "hsl(var(--muted) / 0.45)", fillOpacity: 1 } },
        }),
        barX(bars, {
          key: "key",
          x: measure,
          y: "key",
          fill: (bar) => (bar.deemphasised ? OTHER_COLOR : PROJECT_COLOR),
          maxThickness: 20,
          radius: { end: 4 },
        }),
        decorative(
          text(bars, {
            key: "key",
            x: measure,
            y: "key",
            text: (bar) =>
              metric === "cost"
                ? formatCost(money, bar.totals.cost, true)
                : formatCompact(totalTokens(bar.totals.tokens)),
            anchor: "start",
            dx: 6,
            fontSize: 11,
            fill: "hsl(var(--foreground))",
          }),
        ),
      ],
      scales: {
        x: { scale: scaleLinear, axis: false },
        y: {
          scale: () =>
            scaleBand<string>()
              .domain(bars.map((bar) => bar.key))
              .paddingInner(0.12)
              .paddingOuter(0.12),
          axis: {
            ...USAGE_CHART_AXIS,
            ticks: {
              ...USAGE_CHART_AXIS.ticks,
              format: (key: string) => {
                const label = labels.get(key) ?? key;
                const limit = width < 480 ? 14 : 22;
                return label.length > limit
                  ? `${label.slice(0, limit - 1)}…`
                  : label;
              },
            },
          },
        },
      },
      theme: USAGE_CHART_THEME,
      focus: "group-y",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        format: ({ datum: bar }: { datum: ProjectBar }) =>
          [
            bar.label,
            `Estimated cost: ${formatCost(money, bar.totals.cost)}`,
            `${formatCompact(totalTokens(bar.totals.tokens))} tokens`,
            ...(bar.totals.cost.kind === "partial"
              ? [
                  `${formatCount(bar.totals.cost.unpricedRecords)} unpriced requests: cost unknown`,
                ]
              : []),
          ].join("\n"),
      },
    }));
  }, [bars, metric, money]);

  return (
    <ChartCard
      className={className}
      title={metric === "cost" ? "Spend per project" : "Tokens per project"}
      description={
        metric === "cost"
          ? `API-equivalent estimate in ${money.currency}. ? marks usage without a known price.`
          : "All token categories. Unassigned usage is not mapped to a project."
      }
      actions={
        <Segmented
          label="Measure"
          value={metric}
          onChange={setMetric}
          options={[
            { value: "cost", label: "Spend" },
            { value: "tokens", label: "Tokens" },
          ]}
        />
      }
    >
      {bars.length === 0 ? (
        <p className="text-sm text-muted-foreground">No usage in this range.</p>
      ) : (
        <Chart
          definition={definition}
          height={bars.length * 34 + 8}
          ariaLabel={
            metric === "cost" ? "Spend per project" : "Tokens per project"
          }
          ariaDescription="Projects ranked from largest to smallest, including unassigned usage."
          className="w-full tabular-nums"
          renderTooltipBody={({ points }) => {
            const bar = points[0]?.datum;
            return bar ? <ProjectTooltip bar={bar} /> : null;
          }}
        />
      )}
    </ChartCard>
  );
}

function ProjectTooltip({ bar }: { bar: ProjectBar }) {
  const money = useMoney();
  const { cost } = bar.totals;
  return (
    <ChartTooltipFrame
      title={bar.label}
      rows={[
        {
          key: "cost",
          color: bar.deemphasised ? OTHER_COLOR : PROJECT_COLOR,
          value: formatCost(money, cost),
          label: "est. cost",
        },
        {
          key: "tokens",
          color: "transparent",
          value: formatCompact(totalTokens(bar.totals.tokens)),
          label: "tokens",
        },
      ]}
      footer={
        cost.kind === "partial"
          ? `${formatCount(cost.unpricedRecords)} unpriced requests: cost unknown`
          : undefined
      }
    />
  );
}
