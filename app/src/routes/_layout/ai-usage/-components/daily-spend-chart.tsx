import * as React from "react";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { barY, crosshair, defineChart, dot, stack } from "@tanstack/charts";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
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
import { SURFACE_COLOR } from "../-lib/colors";
import {
  formatCompact,
  formatCount,
  formatLongDate,
  formatShortDate,
} from "../-lib/format";
import { formatAmount, formatAxisAmount, useMoney } from "../-lib/money";

type View = "chart" | "table";

type SpendPoint = Readonly<{
  key: string;
  row: DailyRow;
  seriesKey: string;
  amount: number;
}>;

/**
 * Estimated spend per local day, stacked by payer, tool or model. Bars show the
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
  // The series the pointer rests on in the legend; the others recede.
  const [hovered, setHovered] = React.useState<string | null>(null);
  const money = useMoney();
  const { series, rows } = React.useMemo(
    () => dailySpend(report, stackBy, money.perUsd),
    [report, stackBy, money.perUsd],
  );
  const unknownDays = rows.filter((row) => row.unpricedRecords > 0);
  const labels = new Map(series.map((entry) => [entry.key, entry]));
  const definition = React.useMemo(() => {
    const points: SpendPoint[] = rows.flatMap((row) =>
      series.map((entry) => ({
        key: `${row.date}:${entry.key}`,
        row,
        seriesKey: entry.key,
        amount: row.values[entry.key] ?? 0,
      })),
    );
    const unknown: SpendPoint[] = rows
      .filter((row) => row.unpricedRecords > 0)
      .map((row) => ({
        key: `${row.date}:unpriced`,
        row,
        seriesKey: "unpriced",
        amount: row.known,
      }));
    return defineChart({
      marks: [
        crosshair({
          x: { band: { fill: "hsl(var(--muted) / 0.45)", fillOpacity: 1 } },
          y: false,
        }),
        barY(points, {
          x: (point) => point.row.date,
          y: "amount",
          color: "seriesKey",
          key: "key",
          layout: stack({ order: series.map((entry) => entry.key) }),
          maxThickness: 24,
          radius: { end: 4 },
          stroke: (point) => (point.amount > 0 ? SURFACE_COLOR : "none"),
          strokeWidth: 1,
        }),
        dot(unknown, {
          x: (point) => point.row.date,
          y: "amount",
          key: "key",
          r: 4,
          fill: SURFACE_COLOR,
          stroke: "hsl(var(--foreground))",
          strokeWidth: 1.5,
        }),
      ],
      scales: {
        x: {
          scale: scaleBand<string>()
            .domain(rows.map((row) => row.date))
            .padding(0.2),
          axis: {
            ...USAGE_CHART_AXIS,
            line: { stroke: "var(--viz-grid)", strokeOpacity: 1 },
            ticks: { size: 0, line: false, format: formatShortDate },
            tickLabels: {
              fontSize: 11,
              opacity: 1,
              dy: 3,
              thin: { minGap: 16, priority: "ends" },
            },
          },
        },
        y: {
          scale: scaleLinear,
          nice: true,
          grid: { strokeOpacity: 1 },
          axis: {
            ...USAGE_CHART_AXIS,
            ticks: {
              size: 0,
              line: false,
              count: 5,
              format: (value) => formatAxisAmount(money, value),
            },
          },
        },
      },
      color: {
        domain: series.map((entry) => entry.key),
        range: series.map((entry) =>
          hovered === null || hovered === entry.key
            ? entry.color
            : `color-mix(in srgb, ${entry.color} 22%, ${SURFACE_COLOR})`,
        ),
      },
      theme: USAGE_CHART_THEME,
      // Bars move to their new heights when the currency or a filter changes.
      svgAnimation: { duration: 350, easing: "ease-out" },
      focus: "group-x",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        formatGroup: (points) => {
          const row = points[0]?.datum.row;
          if (!row) return "";
          return [
            formatLongDate(row.date),
            ...[...series].reverse().flatMap((entry) => {
              const amount = row.values[entry.key] ?? 0;
              return amount > 0
                ? [`${entry.label}: ${formatAmount(money, amount)}`]
                : [];
            }),
            `${row.unpricedRecords > 0 ? "Known subtotal" : "Total"}: ${formatAmount(money, row.known)}`,
            `${formatCompact(row.tokens)} tokens`,
            ...(row.unpricedRecords > 0
              ? [
                  `Plus unknown: ${formatCount(row.unpricedRecords)} unpriced requests`,
                ]
              : []),
          ].join("\n");
        },
      },
    });
  }, [rows, series, money, hovered]);

  return (
    <ChartCard
      className={className}
      title="Estimated spend per day"
      description={`By local day in ${report.timeZone}.`}
      actions={
        <>
          <Segmented
            label="Stack by"
            value={stackBy}
            onChange={onStackByChange}
            options={[
              { value: "payer", label: "Paid by" },
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
          {rows.some((row) => row.records > 0) ? (
            <Chart
              definition={definition}
              height={256}
              className="w-full tabular-nums"
              ariaLabel="Estimated spend per local day"
              ariaDescription="Bars show known estimated cost. Rings mark unpriced usage whose cost is unknown. Use arrow keys to inspect days and Enter to pin details."
              renderTooltipBody={({ points }) => {
                const row = points[0]?.datum.row;
                return row ? <DailyTooltip row={row} labels={labels} /> : null;
              }}
            />
          ) : (
            <p className="text-sm text-muted-foreground">
              No usage in this range.
            </p>
          )}
          <ul className="flex flex-wrap gap-x-4 gap-y-1">
            {series.map((entry) => (
              <LegendItem
                key={entry.key}
                color={entry.color}
                label={entry.label}
                icon={
                  entry.provider && <AiProviderIcon provider={entry.provider} />
                }
                onHover={(on) => setHovered(on ? entry.key : null)}
              />
            ))}
            {unknownDays.length > 0 && (
              <LegendItem
                shape="ring"
                color=""
                label="Includes usage without a known price"
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

function DailyTooltip({
  row,
  labels,
}: {
  row: DailyRow;
  labels: ReadonlyMap<string, { label: string; color: string }>;
}) {
  const money = useMoney();
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
        value: formatAmount(money, entry.value),
        label: entry.label,
      }))}
      footer={
        <>
          <p>
            {row.unpricedRecords > 0 ? "Known subtotal " : "Total "}
            <span className="font-semibold text-foreground">
              {formatAmount(money, row.known)}
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

function DailyTable({ rows }: { rows: readonly DailyRow[] }) {
  const money = useMoney();
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
            <TableHead className="text-right">
              Est. cost ({money.currency})
            </TableHead>
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
                  ? row.known > 0
                    ? `${formatAmount(money, row.known)} + unknown`
                    : "Unknown"
                  : formatAmount(money, row.known)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
