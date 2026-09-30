import * as React from "react";
import {
  Bar,
  BarChart,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipProps,
} from "recharts";
import type {
  AiUsageReport,
  AiUsageTotals,
} from "@/lib/api/queries/ai-usage-report";
import { ChartCard, ChartTooltipFrame, Segmented } from "./chart-card";
import { parseMarkGeometry } from "./chart-props";
import {
  projectBars,
  type ProjectBar,
  type ProjectMetric,
} from "../-lib/aggregate";
import { AXIS_TICK, OTHER_COLOR } from "../-lib/colors";
import {
  formatCompact,
  formatCost,
  formatCount,
  knownUsd,
  totalTokens,
} from "../-lib/format";

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
  const bars = React.useMemo(
    () => projectBars(report, metric),
    [report, metric],
  );
  const measure = (totals: AiUsageTotals) =>
    metric === "cost" ? knownUsd(totals.cost) : totalTokens(totals.tokens);

  return (
    <ChartCard
      className={className}
      title={metric === "cost" ? "Spend per project" : "Tokens per project"}
      description={
        metric === "cost"
          ? "API-equivalent estimate in USD. ? marks usage without a known price."
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
        <div style={{ height: bars.length * 34 + 8 }} className="w-full">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart
              data={[...bars]}
              layout="vertical"
              margin={{ top: 0, right: 96, bottom: 0, left: 0 }}
              barCategoryGap={6}
            >
              <XAxis type="number" hide domain={[0, "dataMax"]} />
              <YAxis
                type="category"
                dataKey="label"
                width={148}
                tick={AXIS_TICK}
                tickLine={false}
                axisLine={false}
                tickFormatter={(label: string) =>
                  label.length > 22 ? `${label.slice(0, 21)}…` : label
                }
              />
              <Tooltip
                cursor={{ fill: "hsl(var(--muted) / 0.45)" }}
                content={<ProjectTooltip />}
              />
              <Bar
                dataKey={(bar: ProjectBar) => measure(bar.totals)}
                maxBarSize={20}
                radius={[0, 4, 4, 0]}
                isAnimationActive={false}
                label={(props: unknown) => (
                  <BarTip
                    props={props}
                    text={(index) => {
                      const bar = bars[index];
                      if (!bar) {
                        return "";
                      }
                      return metric === "cost"
                        ? formatCost(bar.totals.cost, true)
                        : formatCompact(totalTokens(bar.totals.tokens));
                    }}
                  />
                )}
              >
                {bars.map((bar) => (
                  <Cell
                    key={bar.key}
                    fill={bar.deemphasised ? OTHER_COLOR : PROJECT_COLOR}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </ChartCard>
  );
}

/** A value label just past the end of a horizontal bar. */
function BarTip({
  props,
  text,
}: {
  props: unknown;
  text: (index: number) => string;
}) {
  const geometry = parseMarkGeometry(props);
  if (!geometry) {
    return null;
  }
  return (
    <text
      x={geometry.x + geometry.width + 6}
      y={geometry.y + geometry.height / 2}
      dominantBaseline="central"
      fontSize={11}
      fill="hsl(var(--foreground))"
      style={{ fontVariantNumeric: "tabular-nums" }}
    >
      {text(geometry.index)}
    </text>
  );
}

function ProjectTooltip({ active, payload }: TooltipProps<number, string>) {
  const bar = payload?.[0]?.payload;
  if (!active || !isProjectBar(bar)) {
    return null;
  }
  const { cost } = bar.totals;
  return (
    <ChartTooltipFrame
      title={bar.label}
      rows={[
        {
          key: "cost",
          color: bar.deemphasised ? OTHER_COLOR : PROJECT_COLOR,
          value: formatCost(cost),
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

function isProjectBar(value: unknown): value is ProjectBar {
  return (
    typeof value === "object" &&
    value !== null &&
    "label" in value &&
    "totals" in value &&
    "deemphasised" in value
  );
}
