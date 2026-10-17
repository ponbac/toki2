import { AlertTriangle } from "lucide-react";
import * as React from "react";
import { Button } from "@/components/ui/button";
import { apiErrorMessage } from "@/lib/api/errors";

/** A failed load, with the server's reason when it gave one, and a retry. */
export function QueryError({
  message,
  error,
  onRetry,
}: {
  message: string;
  error: unknown;
  onRetry: () => void;
}) {
  const reason = useApiErrorMessage(error);
  return (
    <div
      role="alert"
      className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2"
    >
      <p className="flex items-center gap-2 text-sm text-destructive">
        <AlertTriangle aria-hidden className="size-4 shrink-0" />
        <span>
          {message}
          {reason && <span className="text-muted-foreground"> {reason}</span>}
        </span>
      </p>
      <Button type="button" variant="outline" size="sm" onClick={onRetry}>
        Retry
      </Button>
    </div>
  );
}

/**
 * The server's message for a failed request, once it has been read from the
 * response, or null while it is read or when there is none.
 */
function useApiErrorMessage(error: unknown): string | null {
  const [message, setMessage] = React.useState<string | null>(null);
  React.useEffect(() => {
    let current = true;
    void apiErrorMessage(error, "").then((text) => {
      if (current) {
        setMessage(text || null);
      }
    });
    return () => {
      current = false;
    };
  }, [error]);
  return message;
}
