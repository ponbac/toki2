import * as React from "react";
import type {
  AiUsageReport,
  AiUsageTotals,
} from "@/lib/api/queries/ai-usage-report";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Segmented } from "@/components/segmented";
import { ChartCard } from "./chart-card";
import { hourGrid } from "../-lib/aggregate";
import { SEQUENTIAL, sequentialColor } from "../-lib/colors";
import {
  WEEKDAYS,
  formatCompact,
  formatHour,
  knownUsd,
  totalTokens,
} from "../-lib/format";
import { formatCost, formatMoney, useMoney } from "../-lib/money";

type Metric = "cost" | "tokens";

/**
 * Spend (or tokens) per local hour of day and weekday in the server's AI
 * usage time zone, over the whole range, laid out as a calendar week: days
 * across, hours down. One hue, darker is more; a dot marks hours with
 * unpriced usage, whose cost is unknown.
 */
export function HourHeatmap({
  report,
  className,
}: {
  report: AiUsageReport;
  className?: string;
}) {
  const [metric, setMetric] = React.useState<Metric>("cost");
  const money = useMoney();
  const grid = React.useMemo(() => hourGrid(report), [report]);
  const measure = (totals: AiUsageTotals | null) =>
    totals === null
      ? 0
      : metric === "cost"
        ? knownUsd(totals.cost)
        : totalTokens(totals.tokens);
  const max = Math.max(0, ...grid.flat().map((cell) => measure(cell.totals)));
  const format = (value: number) =>
    metric === "cost" ? formatMoney(money, value) : formatCompact(value);

  return (
    <ChartCard
      className={className}
      title="When you use AI"
      description={`Per hour of the week, in ${report.timeZone}.`}
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
      <div
        aria-hidden
        className="grid gap-[2px]"
        style={{ gridTemplateColumns: "1.75rem repeat(7, minmax(0, 1fr))" }}
      >
        <span />
        {WEEKDAYS.map((weekday) => (
          <span
            key={weekday}
            className="pb-0.5 text-center text-[10px] text-muted-foreground"
          >
            {weekday}
          </span>
        ))}
        {Array.from({ length: 24 }, (_, hour) => (
          <React.Fragment key={hour}>
            <span className="-mt-px pr-1.5 text-right text-[10px] leading-3 text-muted-foreground tabular-nums">
              {hour % 3 === 0 ? String(hour).padStart(2, "0") : ""}
            </span>
            {grid.map((row, day) => {
              const cell = row[hour];
              if (!cell) {
                return null;
              }
              const color = sequentialColor(measure(cell.totals), max);
              const unpriced = cell.totals?.cost.kind === "partial";
              return (
                <Tooltip key={day}>
                  <TooltipTrigger asChild>
                    <div
                      className="relative h-3 rounded-[3px] transition-[outline] hover:outline hover:outline-2 hover:outline-foreground/60"
                      style={{
                        backgroundColor: color ?? "hsl(var(--muted) / 0.5)",
                      }}
                    >
                      {unpriced && metric === "cost" && (
                        <span className="absolute top-0.5 right-0.5 size-1.5 rounded-full border border-card bg-foreground" />
                      )}
                    </div>
                  </TooltipTrigger>
                  <TooltipContent className="text-xs">
                    <p className="font-medium">
                      {WEEKDAYS[day]} {formatHour(cell.hour)}–
                      {formatHour((cell.hour + 1) % 24)}
                    </p>
                    {cell.totals ? (
                      <p className="text-muted-foreground">
                        <span className="font-semibold text-foreground">
                          {formatCost(money, cell.totals.cost)}
                        </span>{" "}
                        est., {formatCompact(totalTokens(cell.totals.tokens))}{" "}
                        tokens
                      </p>
                    ) : (
                      <p className="text-muted-foreground">No usage</p>
                    )}
                  </TooltipContent>
                </Tooltip>
              );
            })}
          </React.Fragment>
        ))}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 text-xs text-muted-foreground">
        <div className="flex items-center gap-1.5">
          <span>Less</span>
          <span className="flex gap-[2px]">
            {SEQUENTIAL.map((color) => (
              <span
                key={color}
                className="h-2.5 w-3 rounded-[2px]"
                style={{ backgroundColor: color }}
              />
            ))}
          </span>
          <span>More{max > 0 && `, up to ${format(max)}`}</span>
        </div>
        {metric === "cost" &&
          report.hours.some((hour) => hour.totals.cost.kind === "partial") && (
            <span className="flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-foreground" />
              Includes usage without a known price
            </span>
          )}
      </div>
      <HeatmapTable report={report} />
    </ChartCard>
  );
}

/**
 * The heatmap's hours with usage as a table, for screen readers. A wrapper
 * hides it: a table is never narrower than its content, and its caption sits
 * outside the box `sr-only` clips, so Firefox painted the caption over the
 * card's title while the page was dimmed for a refetch.
 */
function HeatmapTable({ report }: { report: AiUsageReport }) {
  const money = useMoney();
  return (
    <div className="sr-only">
      <table>
        <caption>Usage per local weekday and hour</caption>
        <thead>
          <tr>
            <th scope="col">Weekday</th>
            <th scope="col">Hour</th>
            <th scope="col">Estimated cost</th>
            <th scope="col">Tokens</th>
          </tr>
        </thead>
        <tbody>
          {report.hours.map((hour) => (
            <tr key={`${hour.weekday}-${hour.hour}`}>
              <td>{WEEKDAYS[hour.weekday - 1]}</td>
              <td>{formatHour(hour.hour)}</td>
              <td>{formatCost(money, hour.totals.cost)}</td>
              <td>{formatCompact(totalTokens(hour.totals.tokens))}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
