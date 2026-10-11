import * as React from "react";
import { HTTPError } from "ky";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { apiErrorMessage } from "@/lib/api/errors";

/** A failed admin query: why it failed, and a way to retry. */
export function QueryError({
  error,
  what,
  onRetry,
}: {
  error: Error;
  what: string;
  onRetry: () => unknown;
}) {
  const [message, setMessage] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    const fallback =
      error instanceof HTTPError && error.response.status === 403
        ? "Only an admin signed in to Toki with a browser session can see this."
        : `Could not load ${what}.`;
    void apiErrorMessage(error, fallback).then((text) => {
      if (!cancelled) {
        setMessage(text);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [error, what]);

  return (
    <div
      className="flex items-center justify-between gap-3 rounded-lg border border-destructive/40 bg-destructive/5 p-3"
      role="alert"
    >
      <p className="flex items-center gap-2 text-sm text-destructive">
        <AlertTriangle className="size-4 shrink-0" />
        {message ?? `Could not load ${what}.`}
      </p>
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => void onRetry()}
      >
        Retry
      </Button>
    </div>
  );
}
