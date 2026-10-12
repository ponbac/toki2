import {
  keepPreviousData,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Download, Info } from "lucide-react";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import {
  aiAdminQueries,
  type AiBillingCompleteness,
  type AiBillingOverview,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { AttentionPill } from "../-components/attention-pill";
import { MonthPicker } from "../-components/month-picker";
import { QueryError } from "../-components/query-error";
import { needsAttention } from "../-lib/coverage";
import {
  mappingsCheck,
  ratesCheck,
  uploadsCheck,
  type CheckResult,
} from "../-lib/export-checks";
import { MONTH_PATTERN, monthLabel, previousMonth } from "../-lib/format";
import { PROVIDER_COLOR_SCOPE } from "../-lib/providers";
import { BillSummary } from "./-components/bill-summary";
import { BillingRules } from "./-components/billing-rules";
import { BillingTable } from "./-components/billing-table";
import { CompletenessPanel } from "./-components/completeness-panel";
import { ExchangeRatesPanel } from "./-components/exchange-rates";
import { ExportChecks, type ExportCheck } from "./-components/export-checks";
import { PersonalUsage } from "./-components/personal-usage";
import { ProjectCostChart } from "./-components/project-cost-chart";
import { SubscriptionMonths } from "./-components/subscription-months";
import { SubscriptionValueChart } from "./-components/subscription-value";
import { UnassignedLines } from "./-components/unassigned-lines";

/** What sits beneath the month's total and its checks. */
const TABS = ["bill", "subscriptions", "personal", "rules"] as const;
type BillingTab = (typeof TABS)[number];

const searchSchema = z.object({
  month: z.string().regex(MONTH_PATTERN).optional().catch(undefined),
  groupBy: z.enum(["project", "developer"]).optional().catch(undefined),
  tab: z.enum(TABS).optional().catch(undefined),
});

export const Route = createFileRoute("/_layout/admin/ai-billing/")({
  validateSearch: searchSchema,
  component: AiBillingPage,
});

/** An underlined tab, as the admin sections above the page are. */
const TAB_TRIGGER =
  "gap-2 rounded-none border-b-2 border-transparent px-3 py-2 text-muted-foreground transition-colors hover:text-foreground focus-visible:ring-inset focus-visible:ring-offset-0 data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:font-semibold data-[state=active]:shadow-none motion-reduce:transition-none";

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

  // Only a bill that loaded: a failed reload shows its error, not old figures.
  const overview = overviewQuery.isSuccess ? overviewQuery.data : undefined;
  const checks = exportChecks(completenessQuery, overviewQuery);
  const openChecks = checks.filter(
    (check) => check.status === "attention",
  ).length;
  // Flags on the bill are only for the month the bill is of.
  const uploadsMissing =
    overview && completenessQuery.data?.month === overview.month
      ? new Set(
          completenessQuery.data.developers
            .filter(needsAttention)
            .map((developer) => developer.userId),
        )
      : undefined;

  const hasLines = overview !== undefined && overview.lines.length > 0;
  const tabs: BillingTab[] = [
    ...(hasLines ? (["bill", "subscriptions"] as const) : []),
    ...(overview && overview.personalUsage.length > 0
      ? (["personal"] as const)
      : []),
    "rules",
  ];
  const tab = search.tab && tabs.includes(search.tab) ? search.tab : tabs[0];
  const stale = overviewQuery.isPlaceholderData && "opacity-60";

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
            {openChecks > 0 && (
              <AttentionPill>
                {openChecks} open
                <span className="sr-only">
                  {openChecks === 1 ? " check" : " checks"} before export
                </span>
              </AttentionPill>
            )}
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)] items-start gap-4 lg:grid-cols-2">
        {overviewQuery.isPending ? (
          <div role="status" aria-label="Loading billing">
            <Skeleton className="h-56 w-full" />
          </div>
        ) : overviewQuery.isError ? (
          <QueryError
            error={overviewQuery.error}
            what="the billing overview"
            onRetry={() => overviewQuery.refetch()}
          />
        ) : overview === undefined ? null : overview.lines.length === 0 ? (
          <div
            className={cn(
              "rounded-xl border border-dashed border-border/80 p-8 text-center transition-opacity motion-reduce:transition-none",
              stale,
            )}
          >
            <p className="font-medium">
              Nothing to bill for {monthLabel(overview.month)}
            </p>
            <p className="text-sm text-muted-foreground">
              No AI usage that bills was synced for this month and no
              subscription covers it.
            </p>
          </div>
        ) : (
          <div
            className={cn(
              "transition-opacity motion-reduce:transition-none",
              stale,
            )}
          >
            <BillSummary
              month={overview.month}
              totals={overview.totals}
              exchangeRates={overview.exchangeRates}
              missingRates={overview.missingRates}
              pendingRates={overview.pendingRates}
              personalUsage={overview.personalUsage}
            />
          </div>
        )}
        {/* Keyed by the month, so its open rows follow the new month's checks. */}
        <ExportChecks key={month} checks={checks} />
      </div>

      {overviewQuery.isPending ? (
        <Skeleton className="h-64 w-full" />
      ) : (
        overview && (
          <Tabs
            value={tab}
            onValueChange={(value) => {
              const next = TABS.find((option) => option === value);
              navigate({
                search: (prev) => ({
                  ...prev,
                  tab: next === tabs[0] ? undefined : next,
                }),
              });
            }}
            className={cn(
              "transition-opacity motion-reduce:transition-none",
              stale,
            )}
          >
            <TabsList
              aria-label="Parts of the month's bill"
              className="h-auto w-full justify-start gap-1 overflow-x-auto rounded-none bg-transparent p-0 shadow-[inset_0_-1px_0_hsl(var(--border)/0.6)]"
            >
              {tabs.includes("bill") && (
                <TabsTrigger value="bill" className={TAB_TRIGGER}>
                  The bill
                </TabsTrigger>
              )}
              {tabs.includes("subscriptions") && (
                <TabsTrigger value="subscriptions" className={TAB_TRIGGER}>
                  Subscriptions
                  {overview.subscriptions.length > 0 && (
                    <span className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] font-semibold tabular-nums leading-none text-muted-foreground">
                      {overview.subscriptions.length}
                    </span>
                  )}
                </TabsTrigger>
              )}
              {tabs.includes("personal") && (
                <TabsTrigger value="personal" className={TAB_TRIGGER}>
                  Personal usage
                </TabsTrigger>
              )}
              <TabsTrigger value="rules" className={TAB_TRIGGER}>
                <Info className="size-4" aria-hidden />
                How billing works
              </TabsTrigger>
            </TabsList>

            <TabsContent value="bill" className="mt-4 space-y-4">
              <ProjectCostChart lines={overview.lines} />
              <div className="rounded-xl border border-border/60 bg-card/60">
                <BillingTable
                  month={overview.month}
                  billingCurrency={overview.billingCurrency}
                  pendingRates={overview.pendingRates}
                  lines={overview.lines}
                  developers={overview.developers}
                  subscriptions={overview.subscriptions}
                  groupBy={groupBy}
                  uploadsMissing={uploadsMissing}
                />
              </div>
            </TabsContent>
            <TabsContent value="subscriptions" className="mt-4 space-y-4">
              <SubscriptionValueChart overview={overview} />
              <div className="rounded-xl border border-border/60 bg-card/60">
                <SubscriptionMonths
                  billingCurrency={overview.billingCurrency}
                  subscriptions={overview.subscriptions}
                  developers={overview.developers}
                />
              </div>
            </TabsContent>
            <TabsContent value="personal" className="mt-4">
              <PersonalUsage
                usage={overview.personalUsage}
                developers={overview.developers}
              />
            </TabsContent>
            <TabsContent value="rules" className="mt-4">
              <BillingRules timeZone={overview.timeZone} />
            </TabsContent>
          </Tabs>
        )
      )}
    </div>
  );
}

/**
 * The month's pre-export checks. Uploads stand on the completeness query
 * alone; the other two read the bill, wait while it loads and are left out
 * when it failed, since the bill's own error says so.
 */
function exportChecks(
  completenessQuery: UseQueryResult<AiBillingCompleteness, Error>,
  overviewQuery: UseQueryResult<AiBillingOverview, Error>,
): ExportCheck[] {
  // Another month's answer, held while this month's loads, is no answer for
  // this month: until its own arrives a check is pending, not passed.
  const uploads: CheckResult =
    completenessQuery.isPending || completenessQuery.isPlaceholderData
      ? { status: "pending", summary: "Checking what every machine uploaded" }
      : completenessQuery.isError
        ? {
            status: "attention",
            summary: "Could not check what every machine uploaded",
          }
        : uploadsCheck(completenessQuery.data);
  const checks: ExportCheck[] = [
    {
      id: "uploads",
      label: "Uploads",
      ...uploads,
      detail: completenessQuery.isPlaceholderData ? undefined : (
        <CompletenessPanel query={completenessQuery} />
      ),
    },
  ];
  if (overviewQuery.isPending || overviewQuery.isPlaceholderData) {
    checks.push(
      {
        id: "rates",
        label: "Exchange rates",
        status: "pending",
        summary: "Loading the bill",
      },
      {
        id: "mappings",
        label: "Mappings",
        status: "pending",
        summary: "Loading the bill",
      },
    );
  } else if (overviewQuery.isSuccess) {
    const overview = overviewQuery.data;
    checks.push(
      {
        id: "rates",
        label: "Exchange rates",
        ...ratesCheck(overview),
        detail: (
          <ExchangeRatesPanel
            key={overview.month}
            month={overview.month}
            billingCurrency={overview.billingCurrency}
            rates={overview.exchangeRates}
            missingRates={overview.missingRates}
            pendingRates={overview.pendingRates}
          />
        ),
      },
      {
        id: "mappings",
        label: "Mappings",
        ...mappingsCheck(overview.lines, overview.billingCurrency),
        detail: (
          <UnassignedLines
            month={overview.month}
            billingCurrency={overview.billingCurrency}
            pendingRates={overview.pendingRates}
            lines={overview.lines}
            developers={overview.developers}
          />
        ),
      },
    );
  }
  return checks;
}
