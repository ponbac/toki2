import { formatShare, formatTokens } from "../../-lib/format";
import {
  perActiveDayOf,
  type PersonStats,
  type VolumeMeasure,
} from "../../-lib/adoption";
import { formatEstimateTick, type EstimateMoney } from "../../-lib/estimates";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { formatAmount } from "./format";
import { SectionHead, Segmented } from "./segmented";

export type DepthScale = "linear" | "log";

const SCALES = [
  { value: "linear", label: "Linear" },
  { value: "log", label: "Log" },
] as const;

/** Display units per dollar: costs are charted in the currency they show
 * in; tokens are their own unit. */
function unitsPer(measure: VolumeMeasure, money: EstimateMoney): number {
  return measure === "cost" && money.kind === "converted" ? money.perUsd : 1;
}

/**
 * The vertical scale of `measure` per day with use, in display units: a
 * linear scale from zero, or a log scale over the whole powers of ten from
 * the smallest value to the largest.
 */
function depthAxis(
  units: readonly number[],
  scale: DepthScale,
): Readonly<{ ticks: number[]; position: (units: number) => number }> {
  const maxUnits = Math.max(0, ...units);
  if (scale === "log") {
    const smallest = Math.min(...units.filter((value) => value > 0), maxUnits);
    const bottom = smallest > 0 ? Math.floor(Math.log10(smallest)) : 0;
    const top = Math.max(
      bottom + 1,
      Math.ceil(Math.log10(Math.max(maxUnits, 1))),
    );
    const ticks = Array.from(
      { length: top - bottom + 1 },
      (_, power) => 10 ** (bottom + power),
    );
    const floor = 10 ** bottom;
    return {
      ticks,
      position: (value) =>
        (Math.log10(Math.max(value, floor)) - bottom) / (top - bottom),
    };
  }
  const raw = Math.max(maxUnits, 1);
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step =
    raw / magnitude > 5
      ? 2 * magnitude
      : raw / magnitude > 2
        ? magnitude
        : magnitude / 2;
  const top = Math.ceil(raw / step) * step;
  const ticks: number[] = [];
  for (let tick = 0; tick <= top + step / 2; tick += step) ticks.push(tick);
  return { ticks, position: (units) => units / top };
}

/**
 * Everyone who used AI in the window: right means more of its work days with
 * AI, up means more AI on the days it is used.
 */
export function HabitDepth({
  people,
  weeks,
  measure,
  scale,
  onScale,
  onSelect,
}: {
  people: readonly PersonStats[];
  weeks: number;
  measure: VolumeMeasure;
  scale: DepthScale;
  onScale: (scale: DepthScale) => void;
  onSelect: (userId: number) => void;
}) {
  return (
    <section aria-labelledby="adoption-depth" className="flex flex-col gap-3">
      <SectionHead
        id="adoption-depth"
        title="Habit and depth"
        description={`Right means AI on more of the last ${weeks} weeks' work days. Up means more AI on the days it is used.`}
      >
        <Segmented
          label="Vertical scale"
          options={SCALES}
          value={scale}
          onChange={onScale}
        />
      </SectionHead>
      <div className="rounded-xl border border-border/60 bg-card p-4">
        <DepthPlot
          people={people}
          measure={measure}
          scale={scale}
          onSelect={onSelect}
        />
      </div>
    </section>
  );
}

/** The plot alone, also drawn on a person's page with them picked out. */
export function DepthPlot({
  people,
  measure,
  scale,
  highlight,
  onSelect,
  height = 320,
}: {
  people: readonly PersonStats[];
  measure: VolumeMeasure;
  scale: DepthScale;
  highlight?: number;
  onSelect?: (userId: number) => void;
  height?: number;
}) {
  const money = useEstimateMoney();
  const per = unitsPer(measure, money);
  const perDay = (stats: PersonStats) => perActiveDayOf(stats, measure);
  // People on GitHub Copilot alone have no tokens to place.
  const users = people
    .filter((stats) =>
      measure === "cost" ? stats.activeDays > 0 : stats.perActiveDayTokens > 0,
    )
    .sort((a, b) => perDay(a) - perDay(b));
  const width = 560;
  const left = 64;
  const right = 16;
  const top = 14;
  const bottom = 36;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const axis = depthAxis(
    users.map((stats) => perDay(stats) * per),
    scale,
  );
  const tickLabel = (tick: number) =>
    measure === "cost"
      ? formatEstimateTick(tick / per, money)
      : formatTokens(tick);
  const x = (share: number) => left + share * plotWidth;
  const y = (units: number) =>
    top + plotHeight - axis.position(units) * plotHeight;

  // Dots that would sit on top of each other spread sideways, in order.
  const taken = new Map<string, number>();
  const dots = users.map((stats) => {
    const cy = y(perDay(stats) * per);
    const key = `${Math.round(stats.share * 40)}:${Math.round(cy / 9)}`;
    const n = taken.get(key) ?? 0;
    taken.set(key, n + 1);
    const offset = (n % 2 === 1 ? -1 : 1) * Math.ceil(n / 2) * 9;
    return {
      stats,
      cx: Math.min(
        Math.max(x(stats.share) + offset, left + 6),
        width - right - 6,
      ),
      cy,
    };
  });
  const top1 = users.at(-1);
  const picked = dots.find((dot) => dot.stats.person.userId === highlight);
  const labelled =
    highlight !== undefined ? picked : dots.find((dot) => dot.stats === top1);

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="h-auto w-full"
      role="group"
      aria-label="Each person by the share of work days with AI and the AI on a day it is used"
    >
      <g className="text-[11px]" fill="hsl(var(--muted-foreground))">
        {axis.ticks.map((tick) => (
          <g key={tick}>
            <line
              x1={left}
              x2={width - right}
              y1={y(tick)}
              y2={y(tick)}
              stroke="var(--viz-grid)"
            />
            <text x={left - 8} y={y(tick) + 4} textAnchor="end">
              {tickLabel(tick)}
            </text>
          </g>
        ))}
        {[0, 0.25, 0.5, 0.75, 1].map((share) => (
          <g key={share}>
            <line
              x1={x(share)}
              x2={x(share)}
              y1={top}
              y2={top + plotHeight}
              stroke={share === 0.75 ? "hsl(var(--border))" : "var(--viz-grid)"}
              strokeDasharray={share === 0.75 ? "3 3" : undefined}
            />
            <text x={x(share)} y={height - 18} textAnchor="middle">
              {formatShare(share)}
            </text>
          </g>
        ))}
        <text x={left + plotWidth / 2} y={height - 2} textAnchor="middle">
          Work days with AI
        </text>
        {highlight === undefined ? (
          <text
            x={x(0.75) - 6}
            y={top + plotHeight - 8}
            textAnchor="end"
            className="font-semibold"
          >
            Daily from here
          </text>
        ) : null}
      </g>
      {dots.map(({ stats, cx, cy }) => {
        const label = `${stats.person.fullName}: AI on ${stats.activeWorkDays} of ${stats.workDays} work days, ${formatAmount(perDay(stats), measure, money)} on a day with AI`;
        if (highlight !== undefined) {
          return stats.person.userId === highlight ? null : (
            <circle
              key={stats.person.userId}
              cx={cx}
              cy={cy}
              r={4.5}
              fill="hsl(var(--card))"
              stroke="hsl(var(--muted-foreground))"
              strokeWidth={1.5}
              opacity={0.55}
            >
              <title>{label}</title>
            </circle>
          );
        }
        return (
          <circle
            key={stats.person.userId}
            cx={cx}
            cy={cy}
            r={5.5}
            fill="var(--viz-seq-5)"
            stroke="hsl(var(--card))"
            strokeWidth={1.5}
            role="button"
            tabIndex={0}
            aria-label={label}
            className="cursor-pointer outline-none focus-visible:stroke-[hsl(var(--ring))] focus-visible:[stroke-width:3]"
            onClick={() => onSelect?.(stats.person.userId)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelect?.(stats.person.userId);
              }
            }}
          >
            <title>{label}</title>
          </circle>
        );
      })}
      {picked ? (
        <circle
          cx={picked.cx}
          cy={picked.cy}
          r={7.5}
          fill="hsl(var(--primary))"
          stroke="hsl(var(--background))"
          strokeWidth={2.5}
        />
      ) : null}
      {labelled ? (
        <text
          x={labelled.cx - 12}
          y={labelled.cy + 4}
          textAnchor="end"
          className="text-xs font-semibold"
          fill="hsl(var(--foreground))"
        >
          {labelled.stats.person.fullName.split(" ")[0]},{" "}
          {formatAmount(perDay(labelled.stats), measure, money)} a day
        </text>
      ) : null}
    </svg>
  );
}
