import * as React from "react";

type AvatarLoadingStatus = "idle" | "loading" | "loaded" | "error";

/** Tries avatar URLs in order and resets failures when the source list changes. */
export function useAvatarSourceWithFallback(sources: string[]) {
  const [failures, setFailures] = React.useState(() => ({
    sources,
    failedSources: new Set<string>(),
  }));
  if (failures.sources !== sources) {
    setFailures({ sources, failedSources: new Set() });
  }
  const failedSources =
    failures.sources === sources ? failures.failedSources : new Set<string>();

  const avatarSrc =
    sources.find((source) => !failedSources.has(source)) ??
    sources[sources.length - 1];

  const onLoadingStatusChange = React.useCallback(
    (status: AvatarLoadingStatus) => {
      if (status !== "error" || !avatarSrc) {
        return;
      }

      setFailures((prev) => {
        if (prev.sources !== sources || prev.failedSources.has(avatarSrc)) {
          return prev;
        }

        const next = new Set(prev.failedSources);
        next.add(avatarSrc);
        return { sources, failedSources: next };
      });
    },
    [avatarSrc, sources],
  );

  return { avatarSrc, onLoadingStatusChange, failedSources };
}
