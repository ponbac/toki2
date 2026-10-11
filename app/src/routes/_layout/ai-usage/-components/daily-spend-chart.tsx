import * as React from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ReferenceDot,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipProps,
} from "recharts";
import type { AiUsageReport } from "@/lib/api/queries/ai-usage-report";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  ChartCard,
  ChartTooltipFrame,
  LegendItem,
  Segmented,
} from "./chart-card";
import { dailySpend, type DailyRow, type StackBy } from "../-lib/aggregate";
import { AXIS_TICK, GRID_COLOR, SURFACE_COLOR } from "../-lib/colors";
import {
  formatCompact,
  formatCount,
  formatLongDate,
  formatShortDate,
  formatUsd,
} from "../-lib/format";

type View = "chart" | "table";

/**
 * Estimated spend per local day, stacked by provider or model. Bars show the
 * known cost; a ring marks days with unpriced usage, whose cost is unknown.
 */
export function DailySpendChart({
  report,
  stackBy,
  onStackByChange,
  className,
}: {
  report: AiUsageReport;
  stackBy: StackBy;
  onStackByChange: (stackBy: StackBy) => void;
  className?: string;
}) {
  const [view, setView] = React.useState<View>("chart");
  const { series, rows } = React.useMemo(
    () => dailySpend(report, stackBy),
    [report, stackBy],
  );
  const unknownDays = rows.filter((row) => row.unpricedRecords > 0);
  const labels = new Map(series.map((entry) => [entry.key, entry]));

  return (
    <ChartCard
      className={className}
      title="Estimated spend per day"
      description={`API-equivalent estimate in USD, by local day in ${report.timeZone}. Not a bill.`}
      actions={
        <>
          <Segmented
            label="Stack by"
            value={stackBy}
            onChange={onStackByChange}
            options={[
              { value: "provider", label: "Tool" },
              { value: "model", label: "Model" },
            ]}
          />
          <Segmented
            label="View"
            value={view}
            onChange={setView}
            options={[
              { value: "chart", label: "Chart" },
              { value: "table", label: "Table" },
            ]}
          />
        </>
      }
    >
      {view === "chart" ? (
        <>
          <div className="h-64 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart
                data={[...rows]}
                margin={{ top: 8, right: 8, bottom: 0, left: 0 }}
                barCategoryGap="20%"
              >
                <CartesianGrid vertical={false} stroke={GRID_COLOR} />
                <XAxis
                  dataKey="date"
                  tickFormatter={formatShortDate}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={{ stroke: GRID_COLOR }}
                  minTickGap={16}
                />
                <YAxis
                  tickFormatter={(value: number) => formatAxisUsd(value)}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={false}
                  width={52}
                  allowDecimals
                />
                <Tooltip
                  cursor={{ fill: "hsl(var(--muted) / 0.45)" }}
                  content={<DailyTooltip labels={labels} />}
                />
                {series.map((entry) => (
                  <Bar
                    key={entry.key}
                    dataKey={(row: DailyRow) => row.values[entry.key] ?? 0}
                    name={entry.label}
                    stackId="spend"
                    fill={entry.color}
                    maxBarSize={24}
                    isAnimationActive={false}
                    shape={(props: unknown) => (
                      <StackSegment props={props} seriesKey={entry.key} />
                    )}
                  />
                ))}
                {unknownDays.map((row) => (
                  <ReferenceDot
                    key={row.date}
                    x={row.date}
                    y={row.knownUsd}
                    r={4}
                    fill={SURFACE_COLOR}
                    stroke="hsl(var(--foreground))"
                    strokeWidth={1.5}
                    ifOverflow="extendDomain"
                  />
                ))}
              </BarChart>
            </ResponsiveContainer>
          </div>
          <ul className="flex flex-wrap gap-x-4 gap-y-1">
            {series.map((entry) => (
              <LegendItem
                key={entry.key}
                color={entry.color}
                label={entry.label}
              />
            ))}
            {unknownDays.length > 0 && (
              <LegendItem
                shape="ring"
                color=""
                label="Includes unpriced usage: cost unknown"
              />
            )}
          </ul>
        </>
      ) : (
        <DailyTable rows={rows} />
      )}
    </ChartCard>
  );
}

function formatAxisUsd(value: number): string {
  if (value === 0) {
    return "$0";
  }
  return value >= 100 ? `$${Math.round(value)}` : formatUsd(value);
}

/** A bar segment with a surface gap above it, rounded only at the stack's top. */
function StackSegment({
  props,
  seriesKey,
}: {
  props: unknown;
  seriesKey: string;
}) {
  const segment = parseSegment(props);
  if (!segment || segment.height <= 0) {
    return null;
  }
  const { x, y, width, height, fill, top } = segment;
  const rounded = top === seriesKey;
  const gap = rounded ? 0 : Math.min(2, height / 2);
  const radius = rounded ? Math.min(4, height, width / 2) : 0;
  const top_ = y + gap;
  const bottom = y + height;

  return (
    <path
      fill={fill}
      d={
        radius > 0
          ? `M${x},${bottom} V${top_ + radius} Q${x},${top_} ${x + radius},${top_} H${x + width - radius} Q${x + width},${top_} ${x + width},${top_ + radius} V${bottom} Z`
          : `M${x},${bottom} V${top_} H${x + width} V${bottom} Z`
      }
    />
  );
}

/**
 * Reads the geometry recharts passes to a custom bar shape. Recharts types
 * these props loosely, so they are parsed rather than trusted.
 */
function parseSegment(props: unknown): {
  x: number;
  y: number;
  width: number;
  height: number;
  fill: string;
  top: string | null;
} | null {
  if (typeof props !== "object" || props === null) {
    return null;
  }
  const record: Record<string, unknown> = { ...props };
  const payload = record.payload;
  const { x, y, width, height, fill } = record;
  if (
    typeof x !== "number" ||
    typeof y !== "number" ||
    typeof width !== "number" ||
    typeof height !== "number" ||
    typeof fill !== "string" ||
    typeof payload !== "object" ||
    payload === null
  ) {
    return null;
  }
  const top = "top" in payload ? payload.top : null;
  return {
    x,
    y,
    width,
    height,
    fill,
    top: typeof top === "string" ? top : null,
  };
}

function DailyTooltip({
  active,
  payload,
  labels,
}: TooltipProps<number, string> & {
  labels: ReadonlyMap<string, { label: string; color: string }>;
}) {
  const row = payload?.[0]?.payload;
  if (!active || !isDailyRow(row)) {
    return null;
  }
  const entries = [...labels.entries()]
    .map(([key, entry]) => ({ key, ...entry, value: row.values[key] ?? 0 }))
    .filter((entry) => entry.value > 0)
    .reverse();

  return (
    <ChartTooltipFrame
      title={formatLongDate(row.date)}
      rows={entries.map((entry) => ({
        key: entry.key,
        color: entry.color,
        value: formatUsd(entry.value),
        label: entry.label,
      }))}
      footer={
        <>
          <p>
            {row.unpricedRecords > 0 ? "Known subtotal " : "Total "}
            <span className="font-semibold text-foreground">
              {formatUsd(row.knownUsd)}
            </span>
            {" · "}
            {formatCompact(row.tokens)} tokens
          </p>
          {row.unpricedRecords > 0 && (
            <p>
              Plus unknown: {formatCount(row.unpricedRecords)} unpriced{" "}
              {row.unpricedRecords === 1 ? "request" : "requests"}
            </p>
          )}
        </>
      }
    />
  );
}

function isDailyRow(value: unknown): value is DailyRow {
  return (
    typeof value === "object" &&
    value !== null &&
    "date" in value &&
    "values" in value &&
    "knownUsd" in value
  );
}

function DailyTable({ rows }: { rows: readonly DailyRow[] }) {
  const used = rows.filter((row) => row.records > 0);
  if (used.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">No usage in this range.</p>
    );
  }

  return (
    <div className="max-h-72 overflow-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Day</TableHead>
            <TableHead className="text-right">Tokens</TableHead>
            <TableHead className="text-right">Est. cost (USD)</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {used.map((row) => (
            <TableRow key={row.date}>
              <TableCell>{formatLongDate(row.date)}</TableCell>
              <TableCell className="text-right tabular-nums">
                {formatCompact(row.tokens)}
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {row.unpricedRecords > 0
                  ? row.knownUsd > 0
                    ? `${formatUsd(row.knownUsd)} + unknown`
                    : "Unknown"
                  : formatUsd(row.knownUsd)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
