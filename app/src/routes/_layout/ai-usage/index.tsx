import * as React from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { AlertTriangle } from "lucide-react";
import { z } from "zod";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { AI_PROVIDERS } from "@/lib/api/contracts/ai-usage";
import {
  aiUsageReportQueries,
  type AiUsageReport,
  type AiUsageReportParams,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { ModelTable, ProjectTable } from "./-components/breakdown-tables";
import { DailySpendChart } from "./-components/daily-spend-chart";
import { FilterBar, type FilterPatch } from "./-components/filter-bar";
import { HourHeatmap } from "./-components/hour-heatmap";
import { MachinesPanel } from "./-components/machines-panel";
import { ProjectChart } from "./-components/project-chart";
import { QueryError } from "./-components/query-state";
import { SessionsTable } from "./-components/sessions-table";
import { SummaryTiles } from "./-components/summary-tiles";
import { TokenMixChart } from "./-components/token-mix-chart";
import { UnassignedKeys } from "./-components/unassigned-keys";
import { formatLongDate } from "./-lib/format";

const localDate = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);

/**
 * The page's URL state. Each field falls back to its default on its own, so a
 * malformed value never breaks the page. Without `from` and `to`, the server
 * reports on the current month in its AI usage time zone.
 */
const searchSchema = z.object({
  from: localDate.optional().catch(undefined),
  to: localDate.optional().catch(undefined),
  provider: z.enum(AI_PROVIDERS).optional().catch(undefined),
  model: z.string().optional().catch(undefined),
  projectId: z.string().min(1).optional().catch(undefined),
  unassigned: z.literal(true).optional().catch(undefined),
  machineId: z.string().uuid().optional().catch(undefined),
  stackBy: z.enum(["provider", "model"]).optional().catch(undefined),
});

type Search = z.infer<typeof searchSchema>;

export const Route = createFileRoute("/_layout/ai-usage/")({
  validateSearch: searchSchema,
  component: AiUsagePage,
});

/** The report parameters of the URL state. A project and Unassigned exclude each other. */
function reportParams(search: Search): AiUsageReportParams {
  return {
    from: search.from,
    to: search.to,
    provider: search.provider,
    model: search.model,
    projectId: search.unassigned ? undefined : search.projectId,
    unassigned: search.unassigned,
    machineId: search.machineId,
  };
}

function AiUsagePage() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const params = reportParams(search);
  const reportQuery = useQuery({
    ...aiUsageReportQueries.aiUsageReport(params),
    placeholderData: keepPreviousData,
  });
  // Machines follow the date range only: their usage ignores the filters.
  const machinesQuery = useQuery({
    ...aiUsageReportQueries.aiUsageMachines({
      from: search.from,
      to: search.to,
    }),
    placeholderData: keepPreviousData,
  });
  const keysQuery = useQuery(aiUsageReportQueries.aiUsageProjectKeys());
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

  const setFilters = (patch: FilterPatch) =>
    void navigate({ search: (previous) => ({ ...previous, ...patch }) });

  return (
    <main className="mx-auto w-full max-w-7xl space-y-5 p-4 md:p-8">
      <header className="space-y-1">
        <h1 className="font-display text-2xl font-semibold tracking-tight">
          AI usage
        </h1>
        <p className="max-w-3xl text-sm text-muted-foreground">
          Your own AI usage, as <code>token-ledger sync</code> uploads it from
          your machines. Only you see these hours and sessions. Every cost is an
          API-equivalent estimate in USD, not a subscription bill, and usage
          without a known price shows as unknown, never as $0.
        </p>
      </header>

      {mappable > 0 && (
        <Alert variant="warning" className="p-3 [&>svg]:left-3 [&>svg]:top-3">
          <AlertTriangle className="size-4" />
          <AlertTitle className="text-sm">
            {mappable === 1
              ? "1 repository is not mapped to a project"
              : `${mappable} repositories are not mapped to a project`}
          </AlertTitle>
          <AlertDescription className="text-xs text-foreground">
            Their usage counts as unassigned.{" "}
            <a href="#unassigned" className="underline underline-offset-2">
              Map them to time-tracking projects
            </a>
            .
          </AlertDescription>
        </Alert>
      )}

      {reportQuery.isPending ? (
        <ReportSkeleton />
      ) : reportQuery.isError && !reportQuery.data ? (
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
      ) : (
        <Report
          report={reportQuery.data}
          params={params}
          stale={reportQuery.isPlaceholderData}
          refreshError={reportQuery.isError ? reportQuery.error : null}
          onRetry={() => void reportQuery.refetch()}
          machineLabels={machineLabels}
          stackBy={search.stackBy ?? "provider"}
          onStackByChange={(stackBy) =>
            void navigate({
              search: (previous) => ({ ...previous, stackBy }),
              replace: true,
            })
          }
          onFiltersChange={setFilters}
        />
      )}

      <SessionsTable params={params} machineLabels={machineLabels} />

      <div className="grid gap-5 lg:grid-cols-2">
        <MachinesPanel query={machinesQuery} />
        <UnassignedKeys id="unassigned" query={keysQuery} />
      </div>
    </main>
  );
}

function Report({
  report,
  params,
  stale,
  refreshError,
  onRetry,
  machineLabels,
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
  stackBy: "provider" | "model";
  onStackByChange: (stackBy: "provider" | "model") => void;
  onFiltersChange: (patch: FilterPatch) => void;
}) {
  const empty = report.totals.records === 0;

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FilterBar
          params={params}
          report={report}
          machineLabels={machineLabels}
          onChange={onFiltersChange}
        />
        <p className="text-xs text-muted-foreground">
          Days and hours in {report.timeZone}
          {!report.timeTrackingConfigured &&
            ". Time tracking is not configured, so all usage is unassigned"}
        </p>
      </div>
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
        {empty ? (
          <div className="rounded-2xl border border-dashed border-border/80 bg-card/60 p-8 text-center">
            <p className="font-medium">
              No usage from {formatLongDate(report.from)} to{" "}
              {formatLongDate(report.to)}
              {report.options.providers.length > 0 && " matches the filters"}.
            </p>
            <p className="mt-1 text-sm text-muted-foreground">
              Usage appears here after <code>token-ledger sync</code> uploads
              it. A machine that has not synced lately may be missing usage; see
              Machines below.
            </p>
          </div>
        ) : (
          <>
            <SummaryTiles report={report} />
            <div className="grid gap-5 lg:grid-cols-2">
              <DailySpendChart
                className="lg:col-span-2"
                report={report}
                stackBy={stackBy}
                onStackByChange={onStackByChange}
              />
              <ProjectChart report={report} />
              <TokenMixChart report={report} />
              <HourHeatmap className="lg:col-span-2" report={report} />
              <ProjectTable report={report} />
              <ModelTable report={report} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function ReportSkeleton() {
  return (
    <div className="space-y-5" role="status" aria-label="Loading your AI usage">
      <Skeleton className="h-9 w-full max-w-2xl" />
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <Skeleton key={index} className="h-24 rounded-2xl" />
        ))}
      </div>
      <Skeleton className="h-80 rounded-2xl" />
    </div>
  );
}
