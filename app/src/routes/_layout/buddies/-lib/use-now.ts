import * as React from "react";

/**
 * The current time in milliseconds, rounded down to `stepMs` and updated
 * every step, for countdowns that tick while the page is open.
 */
export function useNow(stepMs: number): number {
  const subscribe = React.useCallback(
    (onChange: () => void) => {
      const interval = window.setInterval(onChange, stepMs);
      return () => window.clearInterval(interval);
    },
    [stepMs],
  );
  const getSnapshot = React.useCallback(
    () => Math.floor(Date.now() / stepMs) * stepMs,
    [stepMs],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot);
}
