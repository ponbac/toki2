import * as React from "react";
import {
  keepPreviousData,
  useInfiniteQuery,
  useQuery,
} from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { MotionConfig } from "framer-motion";
import { useSetAtom } from "jotai/react";
import { Globe, Info } from "lucide-react";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { AI_PROVIDERS } from "@/lib/api/contracts/ai-usage";
import {
  aiUsageReportQueries,
  type AiUsageReport,
  type AiUsageReportParams,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { Breakdown } from "./-components/breakdown";
import { ConnectMachineDialog } from "./-components/connect-machine-dialog";
import { ConnectMachinePrompt } from "./-components/connect-machine-prompt";
import { CurrencySwitch } from "./-components/currency-switch";
import { DailySpendChart } from "./-components/daily-spend-chart";
import { FilterBar, type FilterPatch } from "./-components/filter-bar";
import { HourHeatmap } from "./-components/hour-heatmap";
import { MachinesPanel } from "./-components/machines-panel";
import { QueryError } from "./-components/query-state";
import { RepositoriesPanel } from "./-components/repositories-panel";
import {
  DEFAULT_SESSION_SORT,
  SESSIONS_PAGE_SIZE,
  SessionsTable,
} from "./-components/sessions-table";
import { SummaryTiles } from "./-components/summary-tiles";
import { UsageTabs } from "./-components/usage-tabs";
import { formatLongDate } from "./-lib/format";
import { needsAttention } from "./-lib/machine-findings";
import { MoneyContext, displayCurrencyAtom, formatZero } from "./-lib/money";
import { DEFAULT_TAB, USAGE_TABS } from "./-lib/tabs";
import { useDisplayMoney } from "./-lib/use-display-money";

const localDate = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);

/**
 * The page's URL state. Each field falls back to its default on its own, so a
 * malformed value never breaks the page. Without `from` and `to`, the server
 * reports on the current month in its AI usage time zone.
 */
const searchSchema = z.object({
  tab: z.enum(USAGE_TABS).optional().catch(undefined),
  from: localDate.optional().catch(undefined),
  to: localDate.optional().catch(undefined),
  provider: z.enum(AI_PROVIDERS).optional().catch(undefined),
  model: z.string().optional().catch(undefined),
  projectId: z.string().min(1).optional().catch(undefined),
  unassigned: z.literal(true).optional().catch(undefined),
  personal: z.literal(true).optional().catch(undefined),
  machineId: z.string().uuid().optional().catch(undefined),
  stackBy: z.enum(["provider", "model"]).optional().catch(undefined),
});

type Search = z.infer<typeof searchSchema>;

export const Route = createFileRoute("/_layout/ai-usage/")({
  validateSearch: searchSchema,
  component: AiUsagePage,
});

/**
 * The report parameters of the URL state. A project, Unassigned and Personal
 * exclude each other; Unassigned wins, then Personal.
 */
function reportParams(search: Search): AiUsageReportParams {
  return {
    from: search.from,
    to: search.to,
    provider: search.provider,
    model: search.model,
    projectId:
      search.unassigned || search.personal ? undefined : search.projectId,
    unassigned: search.unassigned,
    personal: search.unassigned ? undefined : search.personal,
    machineId: search.machineId,
  };
}

function AiUsagePage() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const params = reportParams(search);
  const tab = search.tab ?? DEFAULT_TAB;
  const [connecting, setConnecting] = React.useState(false);
  const headingRef = React.useRef<HTMLHeadingElement>(null);
  const reportQuery = useQuery({
    ...aiUsageReportQueries.aiUsageReport(params),
    placeholderData: keepPreviousData,
  });
  // Machines follow the date range only: their usage ignores the filters.
  const machineRange = { from: search.from, to: search.to };
  const machinesQuery = useQuery({
    ...aiUsageReportQueries.aiUsageMachines(machineRange),
    placeholderData: keepPreviousData,
  });
  // The listing holds every machine whatever the range, so none means none ever.
  const neverSynced = machinesQuery.data?.machines.length === 0;
  const keysQuery = useQuery(aiUsageReportQueries.aiUsageProjectKeys());
  // The first page of the Sessions tab, which also says how many there are.
  const sessionsQuery = useInfiniteQuery({
    ...aiUsageReportQueries.aiUsageSessions(
      params,
      DEFAULT_SESSION_SORT,
      SESSIONS_PAGE_SIZE,
    ),
    placeholderData: keepPreviousData,
    enabled: !neverSynced,
  });
  const sessions = sessionsQuery.data?.pages[0]?.total;
  const setCurrency = useSetAtom(displayCurrencyAtom);
  // The header follows the report on screen, like the report's own figures.
  const { currency, money, rates, fetching, failed } = useDisplayMoney(
    reportQuery.data?.to ?? search.to,
  );
  const shown = money?.currency ?? currency;
  // Warms the rates of the dates asked for while their report loads; the
  // figures on screen keep the rates of their own dates until it arrives.
  useQuery({
    ...aiUsageReportQueries.aiDisplayRates(search.to),
    enabled: currency !== "USD",
  });
  const machineLabels = React.useMemo(
    () =>
      new Map(
        (machinesQuery.data?.machines ?? []).map(
          (machine) => [machine.machineId, machine.label] as const,
        ),
      ),
    [machinesQuery.data],
  );
  const mappable =
    keysQuery.data?.projectKeys.filter((key) => key.mappable).length ?? 0;
  // The same findings the Machines tab lists, so none waits there unseen;
  // a failed check of the machines is one too.
  const staleAfterDays = machinesQuery.data?.staleAfterDays;
  const machineProblem =
    machinesQuery.isError ||
    (staleAfterDays !== undefined &&
      (machinesQuery.data?.machines.some((machine) =>
        needsAttention(machine, staleAfterDays),
      ) ??
        false));

  const setFilters = (patch: FilterPatch) =>
    void navigate({ search: (previous) => ({ ...previous, ...patch }) });

  const report = reportQuery.data;
  // The dates and filters, above the report and above the sessions alike.
  const filters = report && (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <FilterBar
        params={params}
        report={report}
        machineLabels={machineLabels}
        onChange={setFilters}
      />
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Globe aria-hidden className="size-3.5 shrink-0" />
        <span>
          <span className="sr-only">Days and hours in </span>
          {report.timeZone}
          {!report.timeTrackingConfigured &&
            ". Time tracking is not configured, so no usage counts for a project"}
        </span>
      </p>
    </div>
  );
  const reportError = reportQuery.isError && !report && (
    <div className="space-y-3">
      <QueryError
        message="Could not load your AI usage."
        error={reportQuery.error}
        onRetry={() => void reportQuery.refetch()}
      />
      {(search.from !== undefined || search.to !== undefined) && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setFilters({ from: undefined, to: undefined })}
        >
          Show this month instead
        </Button>
      )}
    </div>
  );

  return (
    <MotionConfig reducedMotion="user">
      <main className="mx-auto w-full max-w-7xl space-y-5 p-4 md:p-8">
        <header className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:gap-x-8">
          <div className="space-y-1">
            <h1
              ref={headingRef}
              tabIndex={-1}
              className="font-display text-2xl font-semibold tracking-tight outline-none"
            >
              AI usage
            </h1>
            <p className="max-w-3xl text-sm text-muted-foreground">
              Your own AI usage, as <code>token-ledger sync</code> uploads it
              from your machines. Only you see these hours and sessions. Costs
              are API-equivalent estimates, not a subscription bill.{" "}
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    aria-label="How costs are estimated"
                    className="inline-flex rounded-full align-[-2px] outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <Info aria-hidden className="size-3.5" />
                  </button>
                </TooltipTrigger>
                <TooltipContent className="max-w-72 text-xs font-medium">
                  Every cost is what the usage would have cost at API prices,
                  priced in USD
                  {shown === "USD" ? "" : ` and shown in ${shown}`}. Usage
                  without a known price shows as unknown, never as{" "}
                  {formatZero(shown)}.
                </TooltipContent>
              </Tooltip>
            </p>
          </div>
          <CurrencySwitch
            className="justify-items-start sm:justify-items-end sm:pt-0.5"
            currency={currency}
            onChange={setCurrency}
            money={money}
            rates={rates}
            loading={fetching}
            failed={failed}
          />
        </header>

        {neverSynced ? (
          <>
            {machinesQuery.isError && (
              <QueryError
                message="Could not check your machines; showing none, as last known."
                error={machinesQuery.error}
                onRetry={() => void machinesQuery.refetch()}
              />
            )}
            <ConnectMachinePrompt onConnect={() => setConnecting(true)} />
          </>
        ) : (
          <>
            <UsageTabs
              tab={tab}
              sessions={sessions}
              mappable={mappable}
              machines={machinesQuery.data?.machines.length}
              machineProblem={machineProblem}
            />
            {tab === "overview" &&
              (reportQuery.isPending || machinesQuery.isPending ? (
                <ReportSkeleton />
              ) : (
                reportError ||
                (report && (
                  <>
                    {filters}
                    <Report
                      report={report}
                      params={params}
                      stale={reportQuery.isPlaceholderData}
                      refreshError={
                        reportQuery.isError ? reportQuery.error : null
                      }
                      onRetry={() => void reportQuery.refetch()}
                      machineLabels={machineLabels}
                      sessions={sessions}
                      mappable={mappable}
                      stackBy={search.stackBy ?? "provider"}
                      onStackByChange={(stackBy) =>
                        void navigate({
                          search: (previous) => ({ ...previous, stackBy }),
                          replace: true,
                        })
                      }
                      onFiltersChange={setFilters}
                    />
                  </>
                ))
              ))}
            {tab === "sessions" && (
              <>
                {reportQuery.isPending ? (
                  <Skeleton className="h-9 w-full max-w-2xl" />
                ) : (
                  reportError || filters
                )}
                <SessionsTable params={params} machineLabels={machineLabels} />
              </>
            )}
            {tab === "repositories" && <RepositoriesPanel query={keysQuery} />}
            {tab === "machines" && (
              <MachinesPanel
                query={machinesQuery}
                onAddMachine={() => setConnecting(true)}
              />
            )}
          </>
        )}

        <ConnectMachineDialog
          open={connecting}
          onOpenChange={setConnecting}
          range={machineRange}
          fallbackFocusRef={headingRef}
        />
      </main>
    </MotionConfig>
  );
}

function Report({
  report,
  params,
  stale,
  refreshError,
  onRetry,
  machineLabels,
  sessions,
  mappable,
  stackBy,
  onStackByChange,
  onFiltersChange,
}: {
  report: AiUsageReport;
  params: AiUsageReportParams;
  stale: boolean;
  refreshError: Error | null;
  onRetry: () => void;
  machineLabels: ReadonlyMap<string, string>;
  sessions: number | undefined;
  mappable: number;
  stackBy: "provider" | "model";
  onStackByChange: (stackBy: "provider" | "model") => void;
  onFiltersChange: (patch: FilterPatch) => void;
}) {
  const empty = report.totals.records === 0;
  const { money } = useDisplayMoney(report.to);

  return (
    <div className="space-y-5">
      {refreshError && (
        <QueryError
          message="Could not update your AI usage; showing the previous figures."
          error={refreshError}
          onRetry={onRetry}
        />
      )}
      <div
        className={cn(
          "space-y-5 transition-opacity",
          stale && "pointer-events-none opacity-60",
        )}
        aria-busy={stale}
      >
        {money === null ? (
          <FiguresSkeleton />
        ) : empty ? (
          <div className="rounded-2xl border border-dashed border-border/80 bg-card/60 p-8 text-center">
            <p className="font-medium">
              No usage from {formatLongDate(report.from)} to{" "}
              {formatLongDate(report.to)}
              {report.options.providers.length > 0 && " matches the filters"}.
            </p>
            <p className="mt-1 text-sm text-muted-foreground">
              Usage appears here after <code>token-ledger sync</code> uploads
              it. A machine that has not synced lately may be missing usage; see
              the Machines tab.
            </p>
          </div>
        ) : (
          <MoneyContext.Provider value={money}>
            <SummaryTiles
              report={report}
              sessions={sessions}
              mappable={mappable}
            />
            <DailySpendChart
              report={report}
              stackBy={stackBy}
              onStackByChange={onStackByChange}
            />
            <div className="grid gap-5 lg:grid-cols-[minmax(0,2.3fr)_minmax(0,1fr)]">
              <Breakdown
                report={report}
                params={params}
                machineLabels={machineLabels}
                onFiltersChange={onFiltersChange}
              />
              <HourHeatmap report={report} />
            </div>
          </MoneyContext.Provider>
        )}
      </div>
    </div>
  );
}

function ReportSkeleton() {
  return (
    <div className="space-y-5" role="status" aria-label="Loading your AI usage">
      <Skeleton className="h-9 w-full max-w-2xl" />
      <FiguresSkeleton />
    </div>
  );
}

/** Stands in for the report's figures, as while their exchange rate loads. */
function FiguresSkeleton() {
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <Skeleton key={index} className="h-24 rounded-2xl" />
        ))}
      </div>
      <Skeleton className="h-80 rounded-2xl" />
    </div>
  );
}
