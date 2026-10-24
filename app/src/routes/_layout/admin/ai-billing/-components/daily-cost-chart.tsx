import { barY, crosshair, defineChart, stack } from "@tanstack/charts";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import { useMemo } from "react";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { AI_PROVIDERS, type AiProvider } from "@/lib/api/queries/ai-usage";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
import type { DayPoint } from "../../-lib/daily";
import { shortDate } from "../../-lib/format";
import {
  formatEstimateAmount,
  formatEstimateTick,
} from "../../-lib/estimates";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { PROVIDER_COLOR_VARS } from "../../-lib/providers";

/**
 * API-equivalent cost per local day, stacked by provider. Colors follow the
 * provider; the day table is the chart's table view.
 */
export function DailyCostChart({
  points,
  billing,
}: {
  points: readonly DayPoint[];
  /** How each provider's usage bills on each day, `${day}:${provider}`. */
  billing: ReadonlyMap<string, string>;
}) {
  const money = useEstimateMoney();
  const { providers, definition } = useMemo(() => {
    const providers = AI_PROVIDERS.filter((provider) =>
      points.some((point) => point[provider] > 0),
    );
    const rows = points.flatMap((point) =>
      providers.map((provider) => ({
        point,
        provider,
        usd: point[provider],
      })),
    );
    const labels = new Map(points.map((point) => [point.day, point.label]));
    const definition = defineChart({
      marks: [
        crosshair({
          x: { band: { fill: "hsl(var(--muted) / 0.4)", fillOpacity: 1 } },
          y: false,
        }),
        barY(rows, {
          x: (row) => row.point.day,
          y: "usd",
          color: "provider",
          key: (row) => `${row.point.day}:${row.provider}`,
          layout: stack({ order: AI_PROVIDERS }),
          maxThickness: 24,
          radius: { end: 4 },
          stroke: (row) => (row.usd > 0 ? "hsl(var(--card))" : "none"),
          strokeWidth: 2,
        }),
      ],
      scales: {
        x: {
          scale: () =>
            scaleBand<string>()
              .domain(points.map((point) => point.day))
              .padding(0.2),
          axis: {
            ...USAGE_CHART_AXIS,
            line: { stroke: "hsl(var(--border))", strokeOpacity: 1 },
            ticks: {
              ...USAGE_CHART_AXIS.ticks,
              format: (day) => labels.get(day) ?? day,
            },
          },
        },
        y: {
          scale: scaleLinear,
          nice: true,
          grid: { stroke: "hsl(var(--border) / 0.6)", strokeOpacity: 1 },
          axis: {
            ...USAGE_CHART_AXIS,
            ticks: {
              ...USAGE_CHART_AXIS.ticks,
              format: (usd: number) => formatEstimateTick(usd, money),
            },
          },
        },
      },
      color: {
        domain: providers,
        range: providers.map((provider) => PROVIDER_COLOR_VARS[provider]),
      },
      theme: USAGE_CHART_THEME,
      focus: "group-x",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        formatGroup: (focused) => {
          const point = focused[0]?.datum.point;
          if (!point) return "";
          const used = providers.filter((provider) => point[provider] > 0);
          return [
            shortDate(point.day),
            ...(used.length === 0
              ? ["No priced usage"]
              : used.map(
                  (provider) =>
                    `${AI_PROVIDER_LABELS[provider]}: ${formatEstimateAmount(point[provider], money)} · ${billing.get(`${point.day}:${provider}`) ?? "API"}`,
                )),
          ].join("\n");
        },
      },
    });
    return { providers, definition };
  }, [points, billing, money]);
  if (providers.length === 0) {
    return (
      <p className="rounded-lg border border-dashed border-border/80 p-6 text-center text-sm text-muted-foreground">
        No priced usage to chart. Unpriced usage, whose cost is unknown, is
        listed in the tables below.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <ul
        className="flex flex-wrap gap-4 text-xs text-muted-foreground"
        aria-label="Legend"
      >
        {providers.map((provider) => (
          <li key={provider} className="flex items-center gap-1.5">
            <AiProviderIcon provider={provider} />
            {AI_PROVIDER_LABELS[provider]}
          </li>
        ))}
      </ul>
      <Chart
        definition={definition}
        height={256}
        className="min-w-0"
        ariaLabel="Daily API-equivalent cost by AI provider"
        ariaDescription={`Each day's bar stacks the providers' priced usage in ${money.kind === "converted" ? money.currency : "USD"}. Exact values and unpriced usage are listed in the tables below.`}
        renderTooltipBody={({ primaryPoint }) =>
          primaryPoint ? (
            <DayTooltip
              point={primaryPoint.datum.point}
              providers={providers}
              billing={billing}
            />
          ) : null
        }
      />
    </div>
  );
}

function DayTooltip({
  point,
  providers,
  billing,
}: {
  point: DayPoint;
  providers: readonly AiProvider[];
  billing: ReadonlyMap<string, string>;
}) {
  const money = useEstimateMoney();
  const used = providers.filter((provider) => point[provider] > 0);
  return (
    <div className="min-w-44 rounded-lg border border-border/60 bg-card px-3 py-2 text-sm shadow-elevated">
      <div className="mb-1 text-xs text-muted-foreground">
        {shortDate(point.day)}
      </div>
      {used.length === 0 ? (
        <div className="text-xs text-muted-foreground">No priced usage</div>
      ) : (
        used.map((provider) => (
          <div key={provider} className="flex items-center gap-2">
            <span
              className="inline-block h-0.5 w-3 rounded-full"
              style={{ background: PROVIDER_COLOR_VARS[provider] }}
              aria-hidden
            />
            <span className="font-semibold tabular-nums">
              {formatEstimateAmount(point[provider], money)}
            </span>
            <span className="text-xs text-muted-foreground">
              {AI_PROVIDER_LABELS[provider]} ·{" "}
              {billing.get(`${point.day}:${provider}`) ?? "API"}
            </span>
          </div>
        ))
      )}
    </div>
  );
}
