import { formatShare } from "../../-lib/format";
import type { PersonStats } from "../../-lib/adoption";
import {
  formatEstimateAmount,
  formatEstimateTick,
  type EstimateMoney,
} from "../../-lib/estimates";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { formatCost } from "./format";
import { SectionHead, Segmented } from "./segmented";

export type DepthScale = "linear" | "log";

const SCALES = [
  { value: "linear", label: "Linear" },
  { value: "log", label: "Log" },
] as const;

/** The smallest amount a log scale shows, in display units. */
const LOG_FLOOR = 0.1;

/** Display units per dollar: costs are charted in the currency they show in. */
function unitsPerUsd(money: EstimateMoney): number {
  return money.kind === "converted" ? money.perUsd : 1;
}

/**
 * The vertical scale of cost per day with use, in display units: a linear
 * scale from zero, or a log scale over whole powers of ten.
 */
function depthAxis(
  maxUnits: number,
  scale: DepthScale,
): Readonly<{ ticks: number[]; position: (units: number) => number }> {
  if (scale === "log") {
    const top = Math.max(1, Math.ceil(Math.log10(Math.max(maxUnits, 1))));
    const bottom = Math.log10(LOG_FLOOR);
    const ticks = Array.from({ length: top + 1 }, (_, power) => 10 ** power);
    return {
      ticks,
      position: (units) =>
        (Math.log10(Math.max(units, LOG_FLOOR)) - bottom) / (top - bottom),
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
  scale,
  onScale,
  onSelect,
}: {
  people: readonly PersonStats[];
  weeks: number;
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
        <DepthPlot people={people} scale={scale} onSelect={onSelect} />
      </div>
    </section>
  );
}

/** The plot alone, also drawn on a person's page with them picked out. */
export function DepthPlot({
  people,
  scale,
  highlight,
  onSelect,
  height = 320,
}: {
  people: readonly PersonStats[];
  scale: DepthScale;
  highlight?: number;
  onSelect?: (userId: number) => void;
  height?: number;
}) {
  const money = useEstimateMoney();
  const per = unitsPerUsd(money);
  const users = people
    .filter((stats) => stats.activeDays > 0)
    .sort((a, b) => a.perActiveDayUsd - b.perActiveDayUsd);
  const width = 560;
  const left = 64;
  const right = 16;
  const top = 14;
  const bottom = 36;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const axis = depthAxis(
    Math.max(0, ...users.map((stats) => stats.perActiveDayUsd * per)),
    scale,
  );
  const x = (share: number) => left + share * plotWidth;
  const y = (units: number) =>
    top + plotHeight - axis.position(units) * plotHeight;

  // Dots that would sit on top of each other spread sideways, in order.
  const taken = new Map<string, number>();
  const dots = users.map((stats) => {
    const cy = y(stats.perActiveDayUsd * per);
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
              {formatEstimateTick(tick / per, money)}
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
        const label = `${stats.person.fullName}: AI on ${stats.activeWorkDays} of ${stats.workDays} work days, ${formatEstimateAmount(stats.perActiveDayUsd, money)} on a day with AI`;
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
          {formatCost(labelled.stats.perActiveDayUsd, money)} a day
        </text>
      ) : null}
    </svg>
  );
}
