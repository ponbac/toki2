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
import { ChartCard, Segmented } from "./chart-card";
import { hourGrid } from "../-lib/aggregate";
import { SEQUENTIAL, sequentialColor } from "../-lib/colors";
import {
  WEEKDAYS,
  formatCompact,
  formatCost,
  formatHour,
  formatUsd,
  knownUsd,
  totalTokens,
} from "../-lib/format";

type Metric = "cost" | "tokens";

/**
 * Spend (or tokens) per local hour of day and weekday in the server's AI
 * usage time zone, over the whole range. One hue, darker is more; a dot marks
 * hours with unpriced usage, whose cost is unknown.
 */
export function HourHeatmap({
  report,
  className,
}: {
  report: AiUsageReport;
  className?: string;
}) {
  const [metric, setMetric] = React.useState<Metric>("cost");
  const grid = React.useMemo(() => hourGrid(report), [report]);
  const measure = (totals: AiUsageTotals | null) =>
    totals === null
      ? 0
      : metric === "cost"
        ? knownUsd(totals.cost)
        : totalTokens(totals.tokens);
  const max = Math.max(0, ...grid.flat().map((cell) => measure(cell.totals)));
  const format = (value: number) =>
    metric === "cost" ? formatUsd(value) : formatCompact(value);

  return (
    <ChartCard
      className={className}
      title="When you use AI"
      description={`${metric === "cost" ? "Estimated API-equivalent spend" : "Tokens"} per hour of the week, in ${report.timeZone}, summed over the range.`}
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
      <div className="overflow-x-auto">
        <div
          aria-hidden
          className="grid min-w-[560px] gap-[2px]"
          style={{ gridTemplateColumns: "2.5rem repeat(24, minmax(0, 1fr))" }}
        >
          <span />
          {Array.from({ length: 24 }, (_, hour) => (
            <span
              key={hour}
              className="text-center text-[10px] tabular-nums text-muted-foreground"
            >
              {hour % 3 === 0 ? String(hour).padStart(2, "0") : ""}
            </span>
          ))}
          {grid.map((row, day) => (
            <React.Fragment key={day}>
              <span className="self-center text-[11px] text-muted-foreground">
                {WEEKDAYS[day]}
              </span>
              {row.map((cell) => {
                const value = measure(cell.totals);
                const color = sequentialColor(value, max);
                const unpriced = cell.totals?.cost.kind === "partial";
                return (
                  <Tooltip key={cell.hour}>
                    <TooltipTrigger asChild>
                      <div
                        className="relative h-5 rounded-[3px] transition-[outline] hover:outline hover:outline-2 hover:outline-foreground/60"
                        style={{
                          backgroundColor: color ?? "hsl(var(--muted) / 0.5)",
                        }}
                      >
                        {unpriced && metric === "cost" && (
                          <span className="absolute right-0.5 top-0.5 size-1.5 rounded-full border border-card bg-foreground" />
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
                            {formatCost(cell.totals.cost)}
                          </span>{" "}
                          est. ·{" "}
                          {formatCompact(totalTokens(cell.totals.tokens))}{" "}
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
      </div>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground">
        <div className="flex items-center gap-1.5">
          <span>Less</span>
          <span className="flex gap-[2px]">
            {SEQUENTIAL.map((color) => (
              <span
                key={color}
                className="h-2.5 w-4 rounded-[2px]"
                style={{ backgroundColor: color }}
              />
            ))}
          </span>
          <span>More{max > 0 && ` (up to ${format(max)})`}</span>
        </div>
        {metric === "cost" &&
          report.hours.some((hour) => hour.totals.cost.kind === "partial") && (
            <span className="flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-foreground" />
              Includes unpriced usage: cost unknown
            </span>
          )}
      </div>
      <HeatmapTable report={report} />
    </ChartCard>
  );
}

/** The heatmap's hours with usage as a table, for screen readers. */
function HeatmapTable({ report }: { report: AiUsageReport }) {
  return (
    <table className="sr-only">
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
            <td>{formatCost(hour.totals.cost)}</td>
            <td>{formatCompact(totalTokens(hour.totals.tokens))}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
