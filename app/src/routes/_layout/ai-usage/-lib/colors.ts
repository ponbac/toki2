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
  pi: SLOTS[4],
  opencode: SLOTS[5],
};

/** How many models of one tool get their own tint; the rest fold into "Other". */
export const MODEL_TINTS = 3;

/** How much of the tool's colour each tint keeps; the rest is the surface. */
const TINT_STRENGTH = [100, 66, 42] as const;

/** `color` at `strength` percent, receding into the card surface. */
function tint(color: string, strength: number): string {
  return strength >= 100
    ? color
    : `color-mix(in srgb, ${color} ${strength}%, ${SURFACE_COLOR})`;
}

/**
 * The colour of a tool's model at `rank` among that tool's models: a tint of
 * the tool's own colour, so a model reads as its tool in every chart, and the
 * costliest model is the strongest.
 */
export function modelColor(provider: AiProvider, rank: number): string {
  const strength = TINT_STRENGTH[rank];
  return strength === undefined
    ? OTHER_COLOR
    : tint(PROVIDER_COLORS[provider], strength);
}

/**
 * Token categories that are paid for in full, on one hue, strongest for the
 * dearest per token. Cache reads are most of the tokens and little of the
 * cost, so they are a figure beside the strip, not a segment of it.
 */
export const TOKEN_COLORS = {
  output: SLOTS[6],
  cacheWrite: tint(SLOTS[6], TINT_STRENGTH[1]),
  input: tint(SLOTS[6], TINT_STRENGTH[2]),
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
