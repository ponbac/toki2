import * as React from "react";

type AvatarLoadingStatus = "idle" | "loading" | "loaded" | "error";

export function useAvatarSourceWithFallback(sources: string[]) {
  const [failedSources, setFailedSources] = React.useState<Set<string>>(
    () => new Set(),
  );

  React.useEffect(() => {
    setFailedSources(new Set());
  }, [sources]);

  const avatarSrc = React.useMemo(
    () =>
      sources.find((source) => !failedSources.has(source)) ??
      sources[sources.length - 1],
    [sources, failedSources],
  );

  const onLoadingStatusChange = React.useCallback(
    (status: AvatarLoadingStatus) => {
      if (status !== "error" || !avatarSrc) {
        return;
      }

      setFailedSources((prev) => {
        if (prev.has(avatarSrc)) {
          return prev;
        }

        const next = new Set(prev);
        next.add(avatarSrc);
        return next;
      });
    },
    [avatarSrc],
  );

  return { avatarSrc, onLoadingStatusChange, failedSources };
}
