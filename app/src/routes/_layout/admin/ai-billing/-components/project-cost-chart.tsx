import { barX, crosshair, defineChart, text } from "@tanstack/charts";
import { decorative } from "@tanstack/charts/mark/decorative";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import { useMemo } from "react";
import type { AiBillingLine } from "@/lib/api/queries/ai-admin";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
import { costByProject } from "../../-lib/billing";
import { formatUsd } from "../../-lib/format";

const BAR_SIZE = 16;
const ROW_HEIGHT = 36;
type Row = Readonly<{ label: string; usd: number; display: string }>;

/**
 * API-equivalent cost per project in the month, one series in one color. The
 * billing table below is its table view and carries every value.
 */
export function ProjectCostChart({
  lines,
}: {
  lines: readonly AiBillingLine[];
}) {
  const { rows, definition } = useMemo(() => {
    const rows = costByProject(lines).map((row, rank) => ({ ...row, rank }));
    const definition = defineChart(
      ({ width }) => ({
        marks: [
          crosshair({
            x: false,
            y: { band: { fill: "hsl(var(--muted) / 0.4)", fillOpacity: 1 } },
          }),
          barX(rows, {
            x: "usd",
            y: "rank",
            fill: "var(--ai-series-1)",
            maxThickness: BAR_SIZE,
            radius: { end: 4 },
          }),
          decorative(
            text(rows, {
              x: "usd",
              y: "rank",
              text: "display",
              anchor: "start",
              dx: 6,
              fontSize: 12,
            }),
          ),
        ],
        scales: {
          x: {
            scale: scaleLinear,
            nice: true,
            grid: { stroke: "hsl(var(--border) / 0.6)", strokeOpacity: 1 },
            axis: {
              ...USAGE_CHART_AXIS,
              line: { stroke: "hsl(var(--border))", strokeOpacity: 1 },
              ticks: { ...USAGE_CHART_AXIS.ticks, format: formatUsd },
            },
          },
          y: {
            scale: () =>
              scaleBand<number>()
                .domain(rows.map((row) => row.rank))
                .padding(0.25),
            axis: {
              ...USAGE_CHART_AXIS,
              ticks: {
                ...USAGE_CHART_AXIS.ticks,
                format: (rank: number) => {
                  const label = rows[rank]?.label ?? "";
                  const limit = width < 480 ? 16 : 32;
                  return label.length > limit
                    ? `${label.slice(0, limit - 1)}…`
                    : label;
                },
              },
            },
          },
        },
        theme: USAGE_CHART_THEME,
      }),
      {
        focus: "group-y",
        focusRing: false,
        tooltip: {
          ...USAGE_CHART_TOOLTIP,
          format: ({ datum: row }) => `${row.label}\n${row.display}`,
        },
      },
    );
    return { rows, definition };
  }, [lines]);
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
      <Chart
        definition={definition}
        height={rows.length * ROW_HEIGHT + 32}
        className="min-w-0"
        ariaLabel="API-equivalent cost by project, largest first"
        ariaDescription="Bar length shows priced usage in USD. Labels include unknown costs where usage could not be priced. The billing table below lists every value."
        renderTooltipBody={({ primaryPoint }) =>
          primaryPoint ? <ProjectTooltip row={primaryPoint.datum} /> : null
        }
      />
    </section>
  );
}

function ProjectTooltip({ row }: { row: Row }) {
  return (
    <div className="rounded-lg border border-border/60 bg-card px-3 py-2 text-sm shadow-elevated">
      <div className="font-semibold">{row.display}</div>
      <div className="text-xs text-muted-foreground">{row.label}</div>
    </div>
  );
}
