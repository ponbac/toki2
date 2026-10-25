import { AlertTriangle, Info, Loader2, RefreshCw } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import type { AiAccountDirectory } from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import { formatDateOf, plural } from "../../-lib/format";
import { isStale } from "../../-lib/provider-accounts";

/**
 * Where the seats come from and how fresh they are: GitHub through its app,
 * with a Refresh, or why GitHub can't be read.
 */
export function DirectoryStatus({
  directory,
  today,
  refreshing,
  onRefresh,
}: {
  directory: AiAccountDirectory;
  /** Today's UTC date, `YYYY-MM-DD`. */
  today: string;
  refreshing: boolean;
  onRefresh: () => void;
}) {
  if (directory.status === "disabled") {
    return (
      <Alert>
        <Info className="size-4" />
        <AlertTitle>GitHub isn&apos;t connected</AlertTitle>
        <AlertDescription className="text-muted-foreground">
          Set TOKI_GITHUB__APP_PRIVATE_KEY to read seats from GitHub. Accounts
          seen before are listed and can still be given a disposition.
        </AlertDescription>
      </Alert>
    );
  }
  if (directory.status === "misconfigured") {
    return (
      <Alert variant="destructive">
        <AlertTriangle className="size-4" />
        <AlertTitle>GitHub is misconfigured</AlertTitle>
        <AlertDescription className="space-y-1">
          <p>{directory.message ?? "The GitHub app settings can't be used."}</p>
          <p className="text-foreground">
            Accounts seen before are listed and can still be given a
            disposition.
          </p>
        </AlertDescription>
      </Alert>
    );
  }

  const { lastSuccessAt, lastAttemptAt, lastError } = directory;
  const incomplete =
    directory.seatsListed !== null &&
    directory.seatsReported !== null &&
    directory.seatsListed < directory.seatsReported;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border/70 bg-card/60 px-4 py-3">
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
          <span className="font-medium">Seats from GitHub</span>
          <span className="text-muted-foreground">
            ·{" "}
            {lastSuccessAt ? (
              <time
                dateTime={lastSuccessAt}
                title={new Date(lastSuccessAt).toLocaleString("en-GB")}
              >
                read {formatDateOf(lastSuccessAt)}
              </time>
            ) : (
              "not read yet"
            )}
          </span>
          {directory.seatsListed !== null && (
            <span className="text-muted-foreground">
              · {plural(directory.seatsListed, "seat")}
            </span>
          )}
          {isStale(lastSuccessAt, today) && (
            <StatusBadge tone="warning" label="Not read for over 2 days" />
          )}
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={refreshing}
          onClick={onRefresh}
        >
          {refreshing ? (
            <Loader2
              className="size-4 animate-spin motion-reduce:animate-none"
              aria-hidden
            />
          ) : (
            <RefreshCw className="size-4" aria-hidden />
          )}
          {refreshing ? "Refreshing…" : "Refresh"}
        </Button>
      </div>
      {lastError && (
        <Alert variant="destructive">
          <AlertTriangle className="size-4" />
          <AlertTitle>The last refresh failed</AlertTitle>
          <AlertDescription className="space-y-1">
            <p>
              {lastAttemptAt && `${formatDateOf(lastAttemptAt)}: `}
              {lastError}
            </p>
            {lastSuccessAt && (
              <p className="text-foreground">
                The seats below are from the last read that worked,{" "}
                {formatDateOf(lastSuccessAt)}.
              </p>
            )}
          </AlertDescription>
        </Alert>
      )}
      {incomplete && (
        <Alert variant="warning">
          <AlertTriangle className="size-4" />
          <AlertDescription className="text-foreground">
            GitHub reported {plural(directory.seatsReported ?? 0, "seat")} but
            listed {directory.seatsListed}; ended seats weren&apos;t updated.
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
