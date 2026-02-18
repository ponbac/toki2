import type { Iteration } from "@/lib/api/queries/workItems";

function getEffectiveFinishTimeMs(finishDate: string): number {
  const finish = new Date(finishDate);
  const isMidnightUtc =
    finish.getUTCHours() === 0 &&
    finish.getUTCMinutes() === 0 &&
    finish.getUTCSeconds() === 0 &&
    finish.getUTCMilliseconds() === 0;

  // ADO often stores finish dates at midnight; treat those as end-of-day inclusive.
  return isMidnightUtc ? finish.getTime() + 24 * 60 * 60 * 1000 - 1 : finish.getTime();
}

export function isCurrentIteration(iteration: Iteration, nowMs: number): boolean {
  if (iteration.isCurrent) return true;

  // Match backend semantics: date-range fallback is only valid when both dates exist.
  if (!iteration.startDate || !iteration.finishDate) return false;

  const startMs = new Date(iteration.startDate).getTime();
  const finishMs = getEffectiveFinishTimeMs(iteration.finishDate);

  return nowMs >= startMs && nowMs <= finishMs;
}

export function resolveEffectiveIterationPath(
  iterations: Iteration[],
  selectedIterationPath: string | undefined,
): string | undefined {
  if (iterations.length === 0) {
    return selectedIterationPath;
  }

  if (
    selectedIterationPath &&
    iterations.some((iteration) => iteration.path === selectedIterationPath)
  ) {
    return selectedIterationPath;
  }

  const nowMs = Date.now();
  const currentIteration = iterations.find((iteration) =>
    isCurrentIteration(iteration, nowMs),
  );

  return currentIteration?.path ?? iterations[iterations.length - 1]?.path;
}
