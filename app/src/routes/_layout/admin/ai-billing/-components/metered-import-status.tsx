import * as React from "react";
import {
  AlertTriangle,
  ChevronRight,
  DownloadCloud,
  Info,
  Loader2,
  RefreshCw,
} from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import type {
  AiMeteredImportMode,
  AiMeteredMonth,
  AiMeteredRun,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { StatusBadge } from "../../-components/status-badge";
import {
  formatInstant,
  listOf,
  monthLabel,
  plural,
  shortDate,
} from "../../-lib/format";
import {
  accountsWithErrors,
  blocksImport,
  canImport,
  freshnessBadge,
  freshnessText,
  sourceLabel,
  staleReasonText,
} from "../../-lib/reconciliation";

function Instant({ value }: { value: string }) {
  return (
    <time dateTime={value} title={new Date(value).toLocaleString("en-GB")}>
      {formatInstant(value)}
    </time>
  );
}

const RUN_KINDS: Readonly<Record<AiMeteredRun["kind"], string>> = {
  hourly: "Hourly",
  nightly: "Nightly",
  admin: "Admin",
};

/**
 * How fresh the GitHub usage import is, why it is stale, and the import
 * buttons. `compact` shows only the state and why, for a developer's card.
 */
export function MeteredImportStatus({
  data,
  month,
  compact = false,
}: {
  data: AiMeteredMonth;
  /** The UTC month on screen, `YYYY-MM`. */
  month: string;
  compact?: boolean;
}) {
  const imported = data.import;
  const badge = freshnessBadge(imported);
  const running = imported.running;
  const lastSuccess = imported.sources
    .map((source) => source.lastSuccessAt)
    .filter((at): at is string => at !== null)
    .sort()
    .at(-1);

  const start = aiAdminMutations.useStartMeteredImport({
    onSuccess: (started) => {
      if (started === null) {
        toast.info("An import is already running.");
        return;
      }
      toast.success(
        `Importing ${listOf(started.months.map(monthLabel), "conjunction")} from GitHub…`,
      );
    },
    onError: apiErrorToast("Failed to start the GitHub import"),
  });
  const importable = canImport(imported, month, data.today);
  const busy = running !== null || start.isPending;
  const startImport = (mode: AiMeteredImportMode) =>
    start.mutate({ month, mode });

  const summary = (
    <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
      <span className="font-medium">Usage from GitHub</span>
      <StatusBadge tone={badge.tone} label={badge.label} />
      <span className="text-muted-foreground">{freshnessText(imported)}</span>
      {lastSuccess && (
        <span className="text-muted-foreground">
          · last import <Instant value={lastSuccess} />
        </span>
      )}
      {running && (
        <span className="inline-flex items-center gap-1 text-muted-foreground">
          <Loader2
            className="size-3.5 animate-spin motion-reduce:animate-none"
            aria-hidden
          />
          Importing {listOf(running.months.map(monthLabel), "conjunction")}…
        </span>
      )}
    </p>
  );

  const staleAlert = imported.freshness === "stale" && imported.staleReason && (
    <Alert variant="warning">
      <AlertTriangle className="size-4" />
      <AlertTitle>The import is stale</AlertTitle>
      <AlertDescription className="text-foreground">
        {staleReasonText(imported.staleReason)}. The figures below may be behind
        GitHub.
      </AlertDescription>
    </Alert>
  );

  if (compact) {
    return (
      <div className="flex flex-col gap-2">
        {summary}
        {staleAlert}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border/70 bg-card/60 px-4 py-3">
        {summary}
        {imported.status === "ready" && (
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              disabled={busy || !importable}
              title={
                importable
                  ? `Fetch the days of ${monthLabel(month)} that changed on GitHub`
                  : `GitHub usage is imported from ${shortDate(imported.backfillFrom)} through today`
              }
              onClick={() => startImport("changes")}
            >
              {busy ? (
                <Loader2
                  className="size-4 animate-spin motion-reduce:animate-none"
                  aria-hidden
                />
              ) : (
                <DownloadCloud className="size-4" aria-hidden />
              )}
              {busy ? "Importing…" : "Import now"}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={busy || !importable}
              title={`Fetch every day of ${monthLabel(month)} again`}
              onClick={() => startImport("full")}
            >
              <RefreshCw className="size-4" aria-hidden />
              Full refresh
            </Button>
          </div>
        )}
      </div>
      {imported.status === "misconfigured" && (
        <Alert variant="destructive">
          <AlertTriangle className="size-4" />
          <AlertTitle>GitHub is misconfigured</AlertTitle>
          <AlertDescription>
            {imported.message ?? "The GitHub app settings can't be used."}
          </AlertDescription>
        </Alert>
      )}
      {staleAlert}
      {imported.status === "ready" && !imported.unattended && (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Info className="size-3.5 shrink-0" aria-hidden />
          Scheduled imports are off in this build; only Import now fetches from
          GitHub.
        </p>
      )}
      <ImportDetails data={data} />
    </div>
  );
}

/** The sources, account errors and last run, behind a disclosure. */
function ImportDetails({ data }: { data: AiMeteredMonth }) {
  const [open, setOpen] = React.useState(false);
  const id = React.useId();
  const imported = data.import;
  const blockingErrors = imported.accountErrors.filter(blocksImport);
  const last = imported.lastRun;
  if (imported.sources.length === 0 && last === null) {
    return null;
  }

  return (
    <div className="text-sm">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((current) => !current)}
        className="flex items-center gap-1 rounded-md text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
      >
        <ChevronRight
          className={cn(
            "size-3.5 transition-transform duration-150 motion-reduce:transition-none",
            open && "rotate-90",
          )}
          aria-hidden
        />
        Import details
        {blockingErrors.length > 0 &&
          ` · ${plural(accountsWithErrors(blockingErrors), "account")} not imported`}
      </button>
      {open && (
        <div
          id={id}
          className="mt-2 grid gap-4 rounded-md bg-muted/30 p-3 md:grid-cols-2"
        >
          <section className="flex flex-col gap-1.5">
            <h3 className="text-xs font-semibold text-muted-foreground">
              Sources
            </h3>
            <ul className="flex flex-col gap-1">
              {imported.sources.map((source) => (
                <li key={source.source} className="flex flex-col">
                  <span className="font-medium">
                    {sourceLabel(source.source)}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    {source.source === "summary"
                      ? "Read at every import"
                      : source.importedThrough
                        ? `Imported through ${shortDate(source.importedThrough)} (UTC)`
                        : "No day imported completely yet"}
                    {source.settleLagDays !== null &&
                      ` · late changes up to ${plural(source.settleLagDays, "day")} after a day ended`}
                    {source.lastSuccessAt && (
                      <>
                        {" "}
                        · last success <Instant value={source.lastSuccessAt} />
                      </>
                    )}
                  </span>
                  {source.lastError && (
                    <span className="text-xs text-destructive">
                      {source.lastError}
                    </span>
                  )}
                </li>
              ))}
            </ul>
            <p className="text-xs text-muted-foreground">
              Imported from {imported.backfillFrom} (UTC) on.
            </p>
          </section>
          <section className="flex flex-col gap-1.5">
            <h3 className="text-xs font-semibold text-muted-foreground">
              Last run
            </h3>
            {last ? (
              <div className="flex flex-col gap-0.5">
                <span>
                  {RUN_KINDS[last.kind]} ·{" "}
                  {listOf(last.months.map(monthLabel), "conjunction")} ·{" "}
                  {last.status}
                </span>
                <span className="text-xs text-muted-foreground">
                  <Instant value={last.startedAt} />
                  {last.finishedAt && (
                    <>
                      {" "}
                      – <Instant value={last.finishedAt} />
                    </>
                  )}{" "}
                  · {plural(last.requests, "request")} ·{" "}
                  {plural(last.slicesFetched, "slice")} fetched,{" "}
                  {last.slicesChanged} changed
                </span>
                {last.error && (
                  <span className="text-xs text-destructive">{last.error}</span>
                )}
              </div>
            ) : (
              <p className="text-muted-foreground">None finished yet.</p>
            )}
            {imported.accountErrors.length > 0 && (
              <>
                <h3 className="mt-2 text-xs font-semibold text-muted-foreground">
                  Accounts not imported
                </h3>
                <ul className="flex flex-col gap-1">
                  {imported.accountErrors.map((error) => (
                    <li
                      key={`${error.login}:${error.month}`}
                      className="flex flex-col"
                    >
                      <span className="flex flex-wrap items-center gap-1.5">
                        <span className="font-medium">{error.login}</span>
                        <span className="text-xs text-muted-foreground">
                          {monthLabel(error.month)}
                        </span>
                        {error.interrupted ? (
                          <StatusBadge
                            tone="warning"
                            label="Run stopped: imported again next run"
                          />
                        ) : error.seated ? (
                          <StatusBadge tone="warning" label="Held a seat" />
                        ) : (
                          <StatusBadge
                            tone="neutral"
                            label="No seat that month: informational"
                          />
                        )}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {error.error}
                      </span>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
