import { cell, defineChart } from "@tanstack/charts";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { useMemo } from "react";
import { USAGE_CHART_THEME, USAGE_CHART_TOOLTIP } from "@/lib/chart-theme";
import type { CoverageCell } from "../../-lib/coverage";
import { weekdayDate } from "../../-lib/format";

/** A day a sync covered: the quiet step of the sequential ramp. */
export const UPLOADED_COLOR = "var(--viz-seq-2)";
/** A required day nothing covered, in the critical status colour. */
export const MISSING_COLOR = "var(--coverage-missing)";
/** The outline of a day that is not required. */
export const NOT_REQUIRED_COLOR = "var(--viz-grid)";

/** Defines `--coverage-missing` for light and dark; set it on an ancestor. */
export const COVERAGE_COLOR_SCOPE =
  "[--coverage-missing:#d03b3b] dark:[--coverage-missing:#e66767]";

const ROW = "row";
const FOCUSED = [
  {
    when: { focus: "primary" },
    style: { stroke: "hsl(var(--foreground))", strokeWidth: 2 },
  },
] as const;

/**
 * One row of local days: uploaded, missing, or not required. The arrow keys
 * walk the days, and focus shows the same summary as hover. The row's label
 * beside it says what is missing in words.
 */
export function CoverageStrip({
  cells,
  subject,
  thin = false,
}: {
  cells: readonly CoverageCell[];
  /** Whose uploads the row shows, such as a machine's label. */
  subject: string;
  thin?: boolean;
}) {
  const definition = useMemo(() => {
    const radius = thin ? 2 : 3;
    return defineChart({
      marks: [
        cell(
          cells.filter((day) => day.state === "notRequired"),
          {
            x: "day",
            y: () => ROW,
            key: "day",
            fill: "transparent",
            stroke: NOT_REQUIRED_COLOR,
            strokeWidth: 1,
            inset: 1.5,
            radius,
            states: FOCUSED,
          },
        ),
        cell(
          cells.filter((day) => day.state !== "notRequired"),
          {
            x: "day",
            y: () => ROW,
            key: "day",
            color: "state",
            inset: 1,
            radius,
            states: FOCUSED,
          },
        ),
      ],
      scales: {
        // Configured scales: a factory would take its domain from the marks,
        // and so put the days that are not required first.
        x: {
          scale: scaleBand<string>().domain(cells.map((day) => day.day)),
          axis: false,
        },
        y: { scale: scaleBand<string>().domain([ROW]), axis: false },
      },
      guides: false,
      margin: 0,
      color: {
        domain: ["uploaded", "missing"],
        range: [UPLOADED_COLOR, MISSING_COLOR],
      },
      theme: USAGE_CHART_THEME,
      focus: "nearest-x",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        format: ({ datum }) => `${weekdayDate(datum.day)}\n${datum.text}`,
      },
    });
  }, [cells, thin]);

  return (
    <Chart
      definition={definition}
      height={thin ? 10 : 18}
      className="min-w-0"
      ariaLabel={`${subject}: uploads by day`}
      ariaDescription="Each cell is one local day: uploaded, missing, or not required. The text beside the row lists the missing days."
      renderTooltipBody={({ primaryPoint }) =>
        primaryPoint ? (
          <div className="rounded-lg border border-border/60 bg-card px-3 py-2 text-sm shadow-elevated">
            <div className="font-semibold">
              {weekdayDate(primaryPoint.datum.day)}
              <span className="font-normal text-muted-foreground">
                {" "}
                · {subject}
              </span>
            </div>
            <div className="text-xs text-muted-foreground">
              {primaryPoint.datum.text}
            </div>
          </div>
        ) : null
      }
    />
  );
}
