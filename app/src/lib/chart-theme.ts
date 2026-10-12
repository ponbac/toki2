import type { ChartTheme } from "@tanstack/charts";
import { tooltip } from "@tanstack/charts/tooltip";
import { portal } from "@tanstack/charts/tooltip/portal";

/** Chart ink follows the application's light and dark theme without rebuilding data. */
export const USAGE_CHART_THEME = {
  foreground: "hsl(var(--foreground))",
  muted: "hsl(var(--muted-foreground))",
  grid: "var(--viz-grid)",
  background: "transparent",
} satisfies Partial<ChartTheme>;

/** Compact chart guides with labels and no tick stubs. */
export const USAGE_CHART_AXIS = {
  line: false,
  ticks: { size: 0, line: false },
  tickLabels: { fontSize: 11, opacity: 1 },
} as const;

/** Native pointer, keyboard, and pinned tooltips with application-owned content. */
export const USAGE_CHART_TOOLTIP = {
  use: tooltip,
  portal,
  className: "usage-chart-tooltip",
  anchor: "pointer",
  placement: ["right", "left"],
  offset: 12,
} as const;
