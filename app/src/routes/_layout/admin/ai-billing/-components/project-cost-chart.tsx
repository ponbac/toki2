import {
  Bar,
  BarChart,
  CartesianGrid,
  LabelList,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipProps,
} from "recharts";
import { z } from "zod";
import type { AiBillingLine } from "@/lib/api/queries/ai-admin";
import { costByProject } from "../../-lib/billing";
import { formatUsd } from "../../-lib/format";

const BAR_SIZE = 16;
const ROW_HEIGHT = 36;
const AXIS_TICK = { fill: "hsl(var(--muted-foreground))", fontSize: 11 };

type Row = { label: string; usd: number; display: string };

/**
 * API-equivalent cost per project in the month, one series in one color. The
 * billing table below is its table view and carries every value.
 */
export function ProjectCostChart({
  lines,
}: {
  lines: readonly AiBillingLine[];
}) {
  const rows = costByProject(lines);
  if (rows.length === 0) {
    return null;
  }

  return (
    <section
      aria-labelledby="project-cost-heading"
      className="rounded-xl border border-border/60 bg-card/60 p-4"
    >
      <h2 id="project-cost-heading" className="text-sm font-semibold">
        API-equivalent cost by project
      </h2>
      <p className="mb-3 text-xs text-muted-foreground">
        What each project&apos;s usage would cost at API prices, whether a
        subscription or API billing pays for it.
      </p>
      <div style={{ height: rows.length * ROW_HEIGHT + 32 }}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={rows}
            layout="vertical"
            margin={{ top: 0, right: 96, bottom: 0, left: 0 }}
            accessibilityLayer
          >
            <CartesianGrid
              horizontal={false}
              stroke="hsl(var(--border) / 0.6)"
              strokeWidth={1}
            />
            <XAxis
              type="number"
              tick={AXIS_TICK}
              tickFormatter={(value: number) => formatUsd(value)}
              axisLine={{ stroke: "hsl(var(--border))" }}
              tickLine={false}
            />
            <YAxis
              type="category"
              dataKey="label"
              width={180}
              tick={{ ...AXIS_TICK, fill: "hsl(var(--foreground))" }}
              axisLine={false}
              tickLine={false}
            />
            <Tooltip
              cursor={{ fill: "hsl(var(--muted) / 0.4)" }}
              content={<ProjectTooltip rows={rows} />}
            />
            <Bar
              dataKey="usd"
              fill="var(--ai-series-1)"
              barSize={BAR_SIZE}
              radius={[0, 4, 4, 0]}
              isAnimationActive={false}
            >
              <LabelList dataKey="display" content={valueLabel} />
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </section>
  );
}

/** The geometry recharts hands a label; anything else draws nothing. */
const labelSchema = z.object({
  x: z.number(),
  y: z.number(),
  width: z.number(),
  height: z.number(),
  value: z.string(),
});

/** The value at the bar's tip, on one line whatever the bar's length. */
function valueLabel(props: unknown) {
  const parsed = labelSchema.safeParse(props);
  if (!parsed.success) {
    return null;
  }
  const { x, y, width, height, value } = parsed.data;
  return (
    <text
      x={x + width + 6}
      y={y + height / 2}
      dominantBaseline="central"
      fill="hsl(var(--foreground))"
      fontSize={12}
    >
      {value}
    </text>
  );
}

function ProjectTooltip({
  active,
  label,
  rows,
}: TooltipProps<number, string> & { rows: readonly Row[] }) {
  const row = rows.find((candidate) => candidate.label === label);
  if (!active || !row) {
    return null;
  }
  return (
    <div className="rounded-lg border border-border/60 bg-card px-3 py-2 text-sm shadow-elevated">
      <div className="font-semibold">{row.display}</div>
      <div className="text-xs text-muted-foreground">{row.label}</div>
    </div>
  );
}
