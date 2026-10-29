/*
 * The Adoption tab's two encodings: amber, the app's primary, is how often
 * someone uses AI; the sequential blue ramp is how much. Tools keep their
 * provider colors.
 */

import type { Habit } from "../../-lib/adoption";

/** The ramp of a day's square, from least (step 0) to most (step 4). */
export const VOLUME_STEP_COLORS = [
  "var(--viz-seq-1)",
  "var(--viz-seq-2)",
  "var(--viz-seq-4)",
  "var(--viz-seq-5)",
  "var(--viz-seq-7)",
] as const;

/** How often, in three amber steps; nobody seen has none. */
export const HABIT_COLORS: Readonly<Record<Habit, string>> = {
  daily: "hsl(var(--primary))",
  weekly: "hsl(var(--primary) / 0.6)",
  sometimes: "hsl(var(--primary) / 0.3)",
  unseen: "transparent",
};

export const HABIT_LABELS: Readonly<Record<Habit, string>> = {
  daily: "Daily",
  weekly: "Weekly",
  sometimes: "Now and then",
  unseen: "Not seen",
};
