import {
  Bar,
  BarChart,
  CartesianGrid,
  Rectangle,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipProps,
} from "recharts";
import { z } from "zod";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { AI_PROVIDERS, type AiProvider } from "@/lib/api/queries/ai-usage";
import type { DayPoint } from "../../-lib/daily";
import { formatUsd, shortDate } from "../../-lib/format";
import { PROVIDER_COLOR_VARS } from "../../-lib/providers";

const AXIS_TICK = { fill: "hsl(var(--muted-foreground))", fontSize: 11 };
const GAP = 2;
const RADIUS = 4;

/** The geometry recharts hands a bar shape; anything else draws nothing. */
const segmentSchema = z.object({
  x: z.number(),
  y: z.number(),
  width: z.number(),
  height: z.number(),
  fill: z.string(),
  payload: z.object({
    top: z.string().nullable(),
    bottom: z.string().nullable(),
  }),
});

/** A stacked segment: rounded only at the top of its day's stack, and
 * separated from the segment below it by a gap of surface. */
function segment(provider: AiProvider) {
  return (props: unknown) => {
    const parsed = segmentSchema.safeParse(props);
    if (!parsed.success || parsed.data.height <= 0) {
      return <g />;
    }
    const { x, y, width, height, fill, payload } = parsed.data;
    const gap = payload.bottom === provider ? 0 : Math.min(GAP, height / 2);
    return (
      <Rectangle
        x={x}
        y={y}
        width={width}
        height={height - gap}
        fill={fill}
        radius={payload.top === provider ? [RADIUS, RADIUS, 0, 0] : 0}
      />
    );
  };
}

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
  const providers = AI_PROVIDERS.filter((provider) =>
    points.some((point) => point[provider] > 0),
  );
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
            <span
              className="inline-block size-2.5 rounded-sm"
              style={{ background: PROVIDER_COLOR_VARS[provider] }}
              aria-hidden
            />
            {AI_PROVIDER_LABELS[provider]}
          </li>
        ))}
      </ul>
      <div className="h-64">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={[...points]}
            margin={{ top: 8, right: 8, bottom: 0, left: 0 }}
            accessibilityLayer
          >
            <CartesianGrid vertical={false} stroke="hsl(var(--border) / 0.6)" />
            <XAxis
              dataKey="label"
              tick={AXIS_TICK}
              tickLine={false}
              axisLine={{ stroke: "hsl(var(--border))" }}
              interval="preserveStartEnd"
            />
            <YAxis
              tick={AXIS_TICK}
              tickLine={false}
              axisLine={false}
              width={56}
              tickFormatter={(value: number) => formatUsd(value)}
            />
            <Tooltip
              cursor={{ fill: "hsl(var(--muted) / 0.4)" }}
              content={
                <DayTooltip
                  points={points}
                  providers={providers}
                  billing={billing}
                />
              }
            />
            {providers.map((provider) => (
              <Bar
                key={provider}
                dataKey={provider}
                stackId="cost"
                fill={PROVIDER_COLOR_VARS[provider]}
                maxBarSize={24}
                shape={segment(provider)}
                isAnimationActive={false}
              />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

function DayTooltip({
  active,
  label,
  points,
  providers,
  billing,
}: TooltipProps<number, string> & {
  points: readonly DayPoint[];
  providers: readonly AiProvider[];
  billing: ReadonlyMap<string, string>;
}) {
  const point = points.find((candidate) => candidate.label === label);
  if (!active || !point) {
    return null;
  }
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
              {formatUsd(point[provider])}
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
