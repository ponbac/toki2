import { useMemo } from "react";
import { barY, crosshair, defineChart, stack } from "@tanstack/charts";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import type { AiProvider } from "@/lib/api/queries/ai-usage";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
import {
  toolsIn,
  volumeBars,
  type VolumeBar,
  type VolumeMeasure,
  type VolumePeriod,
} from "../../-lib/adoption";
import {
  formatEstimateAmount,
  formatEstimateTick,
  type EstimateMoney,
} from "../../-lib/estimates";
import { formatTokens, shortDate } from "../../-lib/format";
import { PROVIDER_COLOR_VARS } from "../../-lib/providers";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { formatKnownCost, formatTokenCount } from "./format";
import { SectionHead, Segmented } from "./segmented";

const PERIODS = [
  { value: "week", label: "Weeks" },
  { value: "month", label: "Months" },
] as const;

/** A bar's label on the axis: its week's Monday or its month. */
function barLabel(bar: VolumeBar, period: VolumePeriod): string {
  if (period === "week") {
    return bar.partial ? "So far" : shortDate(bar.start);
  }
  const month = new Date(`${bar.start}T00:00:00Z`).toLocaleDateString("en-GB", {
    month: "short",
    timeZone: "UTC",
  });
  return bar.partial ? `${month} so far` : month;
}

/** A bar's total, never showing usage without a known price as zero. */
function formatTotal(
  bar: VolumeBar,
  measure: VolumeMeasure,
  money: EstimateMoney,
): string {
  return measure === "cost"
    ? formatKnownCost(bar.total, bar.unpricedRecords, money)
    : formatTokenCount(bar.total);
}

function formatMeasure(
  value: number,
  measure: VolumeMeasure,
  money: EstimateMoney,
): string {
  return measure === "cost"
    ? formatEstimateAmount(value, money)
    : formatTokenCount(value);
}

/**
 * The company's AI per week or month, stacked by tool: what it would cost at
 * API prices, or every token read and written.
 */
export function VolumeChart({
  adoption,
  measure,
  period,
  onPeriod,
}: {
  adoption: AiAdoption;
  measure: VolumeMeasure;
  period: VolumePeriod;
  onPeriod: (period: VolumePeriod) => void;
}) {
  const money = useEstimateMoney();
  const bars = useMemo(
    () => volumeBars(adoption, measure, period),
    [adoption, measure, period],
  );
  const pending = adoption.credits?.completeThrough ?? null;
  const creditsFrom = adoption.credits?.from ?? null;

  return (
    <section aria-labelledby="adoption-volume" className="flex flex-col gap-3">
      <SectionHead
        id="adoption-volume"
        title={period === "week" ? "AI per week" : "AI per month"}
        description={
          measure === "cost"
            ? "What the company's AI would cost at API prices, by tool. GitHub Copilot is what GitHub charged in credits."
            : "Every token read and written, by tool. Token Ledger only: GitHub reports Copilot in credits, not tokens."
        }
      >
        <Segmented
          label="Bars per"
          options={PERIODS}
          value={period}
          onChange={onPeriod}
        />
      </SectionHead>
      <VolumeBars bars={bars} measure={measure} period={period} money={money} />
      {measure === "cost" && creditsFrom !== null && pending === null ? (
        <p className="text-xs text-muted-foreground">
          No GitHub Copilot credits are imported yet, so Copilot shows no cost
          from {shortDate(creditsFrom)} on.
        </p>
      ) : null}
      {measure === "cost" && pending !== null && pending < adoption.today ? (
        <p className="text-xs text-muted-foreground">
          GitHub Copilot credits are imported through {shortDate(pending)};
          later days may still grow.
        </p>
      ) : null}
      {measure === "cost" &&
      creditsFrom !== null &&
      bars[0] !== undefined &&
      bars[0].start < creditsFrom ? (
        <p className="text-xs text-muted-foreground">
          GitHub Copilot credits start on {shortDate(creditsFrom)}; before that,
          Copilot shows only what Token Ledger saw.
        </p>
      ) : null}
    </section>
  );
}

export function VolumeBars({
  bars,
  measure,
  period,
  money,
}: {
  bars: readonly VolumeBar[];
  measure: VolumeMeasure;
  period: VolumePeriod;
  money: EstimateMoney;
}) {
  const { tools, definition } = useMemo(() => {
    const tools = toolsIn(bars);
    // Costs are charted in the currency they show in, so the axis ticks land
    // on round amounts of it.
    const units =
      measure === "cost" && money.kind === "converted" ? money.perUsd : 1;
    const rows = bars.flatMap((bar) =>
      tools.map((tool) => ({
        bar,
        tool,
        value: (bar.tools[tool] ?? 0) * units,
      })),
    );
    const labels = new Map(
      bars.map((bar) => [bar.start, barLabel(bar, period)]),
    );
    const format = (value: number) =>
      measure === "cost"
        ? formatEstimateTick(value / units, money)
        : formatTokens(value);
    const definition = defineChart({
      marks: [
        crosshair({
          x: { band: { fill: "hsl(var(--muted) / 0.4)", fillOpacity: 1 } },
          y: false,
        }),
        barY(rows, {
          x: (row) => row.bar.start,
          y: "value",
          color: "tool",
          key: (row) => `${row.bar.start}:${row.tool}`,
          layout: stack({ order: tools }),
          maxThickness: period === "week" ? 40 : 96,
          radius: { end: 4 },
          stroke: (row) => (row.value > 0 ? "hsl(var(--card))" : "none"),
          strokeWidth: 2,
        }),
      ],
      scales: {
        x: {
          scale: () =>
            scaleBand<string>()
              .domain(bars.map((bar) => bar.start))
              .padding(0.2),
          axis: {
            ...USAGE_CHART_AXIS,
            line: { stroke: "hsl(var(--border))", strokeOpacity: 1 },
            ticks: {
              ...USAGE_CHART_AXIS.ticks,
              format: (start) => labels.get(start) ?? start,
            },
          },
        },
        y: {
          scale: scaleLinear,
          nice: true,
          grid: { stroke: "hsl(var(--border) / 0.6)", strokeOpacity: 1 },
          axis: {
            ...USAGE_CHART_AXIS,
            ticks: { ...USAGE_CHART_AXIS.ticks, format },
          },
        },
      },
      color: {
        domain: tools,
        range: tools.map((tool) => PROVIDER_COLOR_VARS[tool]),
      },
      theme: USAGE_CHART_THEME,
      focus: "group-x",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        formatGroup: (focused) => {
          const bar = focused[0]?.datum.bar;
          if (!bar) return "";
          return [
            barLabel(bar, period),
            ...tools
              .filter((tool) => (bar.tools[tool] ?? 0) > 0)
              .map(
                (tool) =>
                  `${AI_PROVIDER_LABELS[tool]}: ${formatMeasure(bar.tools[tool] ?? 0, measure, money)}`,
              ),
            `Total: ${formatTotal(bar, measure, money)}`,
          ].join("\n");
        },
      },
    });
    return { tools, definition };
  }, [bars, measure, period, money]);

  if (tools.length === 0) {
    return (
      <p className="rounded-lg border border-dashed border-border/80 p-6 text-center text-sm text-muted-foreground">
        {bars.some((bar) => bar.unpricedRecords > 0)
          ? `The AI used in these ${period === "week" ? "weeks" : "months"} has no known price yet.`
          : `No AI use in these ${period === "week" ? "weeks" : "months"} yet.`}
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <Chart
        definition={definition}
        height={280}
        className="min-w-0"
        ariaLabel={`AI per ${period} by tool, ${measure === "cost" ? "at API prices" : "in tokens"}`}
        ariaDescription={`Each ${period}'s bar stacks the tools' ${measure === "cost" ? "cost" : "tokens"}. The last bar is the current ${period} so far.`}
        renderTooltipBody={({ primaryPoint }) =>
          primaryPoint ? (
            <BarTooltip
              bar={primaryPoint.datum.bar}
              tools={tools}
              measure={measure}
              period={period}
              money={money}
            />
          ) : null
        }
      />
      <ul
        className="flex flex-wrap gap-4 text-xs text-muted-foreground"
        aria-label="Legend"
      >
        {tools.map((tool) => (
          <li key={tool} className="flex items-center gap-1.5">
            <AiProviderIcon provider={tool} />
            {AI_PROVIDER_LABELS[tool]}
          </li>
        ))}
      </ul>
    </div>
  );
}

function BarTooltip({
  bar,
  tools,
  measure,
  period,
  money,
}: {
  bar: VolumeBar;
  tools: readonly AiProvider[];
  measure: VolumeMeasure;
  period: VolumePeriod;
  money: EstimateMoney;
}) {
  const used = tools.filter((tool) => (bar.tools[tool] ?? 0) > 0);
  return (
    <div className="min-w-44 rounded-lg border border-border/60 bg-card px-3 py-2 text-sm shadow-elevated">
      <div className="mb-1 text-xs text-muted-foreground">
        {period === "week"
          ? `Week of ${shortDate(bar.start)}${bar.partial ? ", so far" : ""}`
          : barLabel(bar, period)}
      </div>
      {used.map((tool) => (
        <div key={tool} className="flex items-center gap-2">
          <span
            className="inline-block size-2 rounded-sm"
            style={{ background: PROVIDER_COLOR_VARS[tool] }}
            aria-hidden
          />
          <span className="text-muted-foreground">
            {AI_PROVIDER_LABELS[tool]}
          </span>
          <span className="ml-auto font-semibold tabular-nums">
            {formatMeasure(bar.tools[tool] ?? 0, measure, money)}
          </span>
        </div>
      ))}
      <div className="mt-1 flex justify-between gap-4 border-t border-border/60 pt-1 font-semibold tabular-nums">
        <span>Total</span>
        {formatTotal(bar, measure, money)}
      </div>
    </div>
  );
}
