import type { AiProvider } from "@/lib/api/contracts/ai-usage";

/**
 * Categorical slots in their fixed, validated order (see `--viz-*` in
 * `index.css`). Colours follow entities, never ranks, and are never cycled:
 * a ninth series folds into "Other".
 */
const SLOTS = [
  "var(--viz-1)",
  "var(--viz-2)",
  "var(--viz-3)",
  "var(--viz-4)",
  "var(--viz-5)",
  "var(--viz-6)",
  "var(--viz-7)",
  "var(--viz-8)",
] as const;

/** The de-emphasis grey for "Other" and unassigned usage. */
export const OTHER_COLOR = "var(--viz-other)";

/** Hairline gridlines, one step off the card surface. */
export const GRID_COLOR = "var(--viz-grid)";

/** The card surface, for the gaps and rings that separate marks. */
export const SURFACE_COLOR = "hsl(var(--card))";

/** Each provider keeps its colour in every chart and under every filter. */
export const PROVIDER_COLORS: Readonly<Record<AiProvider, string>> = {
  codex: SLOTS[0],
  claude: SLOTS[1],
  grok: SLOTS[2],
  copilot: SLOTS[3],
};

/** How many models get their own colour; the rest fold into "Other". */
export const MODEL_SLOTS = 7;

/**
 * The colour of the model at `index` in the report's model options, which
 * are ordered by cost over the whole range and ignore filters, so a model
 * keeps its colour when filters change.
 */
export function modelColor(index: number): string {
  return index >= 0 && index < MODEL_SLOTS
    ? (SLOTS[index] ?? OTHER_COLOR)
    : OTHER_COLOR;
}

/** Token categories, in slots 5 to 8 so they never echo a provider's colour. */
export const TOKEN_COLORS = {
  input: SLOTS[4],
  cacheRead: SLOTS[5],
  cacheWrite: SLOTS[6],
  output: SLOTS[7],
} as const;

/** The single-hue sequential ramp, from little to much. */
export const SEQUENTIAL = [
  "var(--viz-seq-1)",
  "var(--viz-seq-2)",
  "var(--viz-seq-3)",
  "var(--viz-seq-4)",
  "var(--viz-seq-5)",
  "var(--viz-seq-6)",
  "var(--viz-seq-7)",
] as const;

/** The ramp step for a value between 0 and `max`, or null for no value. */
export function sequentialColor(value: number, max: number): string | null {
  if (value <= 0 || max <= 0) {
    return null;
  }
  const step = Math.min(
    SEQUENTIAL.length - 1,
    Math.floor((value / max) * SEQUENTIAL.length),
  );
  return SEQUENTIAL[step] ?? null;
}

/** Axis tick text, in muted ink with aligned figures. */
export const AXIS_TICK = {
  fill: "hsl(var(--muted-foreground))",
  fontSize: 11,
  fontVariantNumeric: "tabular-nums",
} as const;
