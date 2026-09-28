import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useApiErrorMessage } from "../-lib/use-api-error-message";

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
