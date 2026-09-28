import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Download } from "lucide-react";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import { aiAdminQueries } from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { MonthPicker } from "../-components/month-picker";
import { QueryError } from "../-components/query-error";
import { MONTH_PATTERN, monthLabel, previousMonth } from "../-lib/format";
import { PROVIDER_COLOR_SCOPE } from "../-lib/providers";
import { BillingRules } from "./-components/billing-rules";
import { BillingTable } from "./-components/billing-table";
import { CompletenessPanel } from "./-components/completeness-panel";
import { ProjectCostChart } from "./-components/project-cost-chart";
import { SubscriptionMonths } from "./-components/subscription-months";
import { TotalsTiles } from "./-components/totals-tiles";

const searchSchema = z.object({
  month: z.string().regex(MONTH_PATTERN).optional().catch(undefined),
  groupBy: z.enum(["project", "developer"]).optional().catch(undefined),
});

export const Route = createFileRoute("/_layout/admin/ai-billing/")({
  validateSearch: searchSchema,
  component: AiBillingPage,
});

function AiBillingPage() {
  const search = Route.useSearch();
  const month = search.month ?? previousMonth();
  const groupBy = search.groupBy ?? "project";
  const navigate = useNavigate({ from: Route.fullPath });

  const overviewQuery = useQuery({
    ...aiAdminQueries.billing(month),
    placeholderData: keepPreviousData,
  });
  const completenessQuery = useQuery({
    ...aiAdminQueries.completeness(month),
    placeholderData: keepPreviousData,
  });
  const download = aiAdminMutations.useDownloadBillingCsv({
    onError: apiErrorToast("Failed to export the billing CSV"),
  });

  return (
    <div className={cn("flex flex-col gap-6", PROVIDER_COLOR_SCOPE)}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <MonthPicker
          month={month}
          onChange={(next) =>
            navigate({ search: (prev) => ({ ...prev, month: next }) })
          }
        />
        <div className="flex flex-wrap items-center gap-2">
          <Tabs
            value={groupBy}
            onValueChange={(value) =>
              navigate({
                search: (prev) => ({
                  ...prev,
                  groupBy: value === "developer" ? "developer" : undefined,
                }),
              })
            }
          >
            <TabsList>
              <TabsTrigger value="project">By project</TabsTrigger>
              <TabsTrigger value="developer">By developer</TabsTrigger>
            </TabsList>
          </Tabs>
          <Button
            type="button"
            variant="outline"
            disabled={download.isPending}
            onClick={() => download.mutate({ month })}
          >
            <Download className="size-4" />
            {download.isPending ? "Exporting..." : "Export CSV"}
          </Button>
        </div>
      </div>

      <CompletenessPanel query={completenessQuery} />

      {overviewQuery.isPending ? (
        <div
          className="flex flex-col gap-3"
          role="status"
          aria-label="Loading billing"
        >
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      ) : overviewQuery.isError ? (
        <QueryError
          error={overviewQuery.error}
          what="the billing overview"
          onRetry={() => overviewQuery.refetch()}
        />
      ) : (
        <div
          className={cn(
            "flex flex-col gap-6 transition-opacity",
            overviewQuery.isPlaceholderData && "opacity-60",
          )}
        >
          {overviewQuery.data.lines.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/80 p-8 text-center">
              <p className="font-medium">
                Nothing to bill for {monthLabel(month)}
              </p>
              <p className="text-sm text-muted-foreground">
                No AI usage was synced for this month and no subscription covers
                it.
              </p>
            </div>
          ) : (
            <>
              <TotalsTiles totals={overviewQuery.data.totals} />
              <ProjectCostChart lines={overviewQuery.data.lines} />
              <section
                aria-labelledby="billing-lines-heading"
                className="flex flex-col gap-2"
              >
                <h2
                  id="billing-lines-heading"
                  className="text-lg font-semibold"
                >
                  {monthLabel(month)} by {groupBy}
                </h2>
                <div className="rounded-xl border border-border/60 bg-card/60">
                  <BillingTable
                    month={month}
                    lines={overviewQuery.data.lines}
                    developers={overviewQuery.data.developers}
                    subscriptions={overviewQuery.data.subscriptions}
                    groupBy={groupBy}
                  />
                </div>
              </section>
              <section
                aria-labelledby="subscriptions-heading"
                className="flex flex-col gap-2"
              >
                <h2
                  id="subscriptions-heading"
                  className="text-lg font-semibold"
                >
                  Subscriptions this month
                </h2>
                <div className="rounded-xl border border-border/60 bg-card/60">
                  <SubscriptionMonths
                    subscriptions={overviewQuery.data.subscriptions}
                    developers={overviewQuery.data.developers}
                  />
                </div>
              </section>
            </>
          )}
          <BillingRules timeZone={overviewQuery.data.timeZone} />
        </div>
      )}
    </div>
  );
}
