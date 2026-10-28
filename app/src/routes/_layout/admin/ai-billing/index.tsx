import * as React from "react";
import {
  keepPreviousData,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Download, Info } from "lucide-react";
import { z } from "zod";
import { AiProviderIcon } from "@/components/ai-provider-icon";
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
import type {
  AiProvider,
  AiSubscriptionMismatchList,
} from "@/lib/api/queries/ai-usage";
import { cn } from "@/lib/utils";
import { AttentionPill } from "../-components/attention-pill";
import { MonthPicker } from "../-components/month-picker";
import { QueryError } from "../-components/query-error";
import {
  mappingsCheck,
  plansCheck,
  ratesCheck,
  uploadsCheck,
  type CheckResult,
} from "../-lib/export-checks";
import {
  MONTH_PATTERN,
  formatCount,
  monthLabel,
  previousMonth,
  shortDate,
} from "../-lib/format";
import {
  allocationSegments,
  ledgerOf,
  peopleOf,
  segmentSteps,
  type AllocationSegment,
} from "../-lib/ledger";
import { monthDays } from "../-lib/coverage";
import { PROVIDER_COLOR_SCOPE } from "../-lib/providers";
import { needsAttention } from "../-lib/reconciliation";
import { monthUploads, type DeveloperUploads } from "../-lib/upload-health";
import { BillingRules } from "./-components/billing-rules";
import { CopilotReconciliation } from "./-components/copilot-reconciliation";
import { ExchangeRatesPanel } from "./-components/exchange-rates";
import { ExportChecks, type ExportCheck } from "./-components/export-checks";
import { PeopleTable } from "./-components/people-table";
import { ProjectLedger } from "./-components/project-ledger";
import { Statement } from "./-components/statement";
import { SubscriptionMonths } from "./-components/subscription-months";
import { SubscriptionValueChart } from "./-components/subscription-value";
import { UndeclaredPlans } from "./-components/undeclared-plans";
import { UploadsPanel } from "./-components/uploads-panel";
import {
  EstimateMoneyContext,
  useBillEstimateMoney,
} from "../-lib/use-estimate-money";

/** The parts of the month beneath its statement. */
const TABS = [
  "projects",
  "people",
  "subscriptions",
  "copilot",
  "data",
  "rules",
] as const;
type BillingTab = (typeof TABS)[number];

const searchSchema = z.object({
  month: z.string().regex(MONTH_PATTERN).optional().catch(undefined),
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
  const tab = search.tab ?? "projects";
  const navigate = useNavigate({ from: Route.fullPath });

  const overviewQuery = useQuery({
    ...aiAdminQueries.billing(month),
    placeholderData: keepPreviousData,
  });
  const completenessQuery = useQuery({
    ...aiAdminQueries.completeness(month),
    placeholderData: keepPreviousData,
  });
  // The month's own evidence: runs from a wider search can span months
  // without any.
  const days = monthDays(month);
  const mismatchesQuery = useQuery(
    aiAdminQueries.subscriptionMismatchesIn(
      days[0] ?? month,
      days.at(-1) ?? month,
    ),
  );
  // The GitHub Copilot tab's month, shared with the tab, for its mark.
  const meteredQuery = useQuery(aiAdminQueries.meteredMonth(month));
  const download = aiAdminMutations.useDownloadBillingCsv({
    onError: apiErrorToast("Failed to export the billing CSV"),
  });

  const [highlighted, setHighlighted] = React.useState<string | null>(null);
  const [open, setOpen] = React.useState<ReadonlySet<string>>(new Set());
  // The ledger row to bring into view; a new object asks again.
  const [focusRow, setFocusRow] = React.useState<{ key: string } | null>(null);
  // A new month starts with every row closed.
  const [openMonth, setOpenMonth] = React.useState(month);
  if (openMonth !== month) {
    setOpenMonth(month);
    setOpen(new Set());
  }

  // Only a bill that loaded: a failed reload shows its error, not old figures.
  const overview = overviewQuery.isSuccess ? overviewQuery.data : undefined;
  // Estimates show in the bill's currency, at the rate of the bill on screen.
  const estimates = useBillEstimateMoney(overview);
  const completeness =
    completenessQuery.isSuccess && !completenessQuery.isPlaceholderData
      ? completenessQuery.data
      : undefined;
  const ledger = React.useMemo(
    () => (overview ? ledgerOf(overview.lines) : undefined),
    [overview],
  );
  // The strip's segments decide each ledger row's step, so both read them.
  const segments = React.useMemo(
    () => (ledger ? allocationSegments(ledger) : []),
    [ledger],
  );
  const steps = React.useMemo(() => segmentSteps(segments), [segments]);
  const uploads = React.useMemo(
    () => (completeness ? monthUploads(completeness) : undefined),
    [completeness],
  );
  const people = React.useMemo(
    () =>
      overview
        ? peopleOf(
            overview,
            uploads
              ?.filter((upload) => upload.verdict !== "noActivity")
              .map((upload) => upload.developer),
          )
        : [],
    [overview, uploads],
  );
  const verdicts = React.useMemo(
    () =>
      uploads &&
      new Map(uploads.map((u) => [u.developer.userId, u.verdict] as const)),
    [uploads],
  );

  const setTab = (next: BillingTab) =>
    navigate({
      search: (prev) => ({
        ...prev,
        tab: next === "projects" ? undefined : next,
      }),
    });
  const showEntry = (key: string) => {
    setTab("projects");
    setOpen((previous) => new Set(previous).add(key));
    setFocusRow({ key });
  };
  const selectSegment = (segment: AllocationSegment) => {
    if (segment.kind === "others") {
      setTab("projects");
      const first = segment.entries[0];
      setFocusRow(first ? { key: first.key } : null);
    } else {
      showEntry(segment.key);
    }
  };
  const toggle = (key: string) =>
    setOpen((previous) => {
      const next = new Set(previous);
      if (!next.delete(key)) {
        next.add(key);
      }
      return next;
    });

  const checks = exportChecks({
    completenessQuery,
    uploads,
    overviewQuery,
    mismatchesQuery,
    open: {
      uploads: () => setTab("data"),
      rates: () => setTab("data"),
      projects: () =>
        // Repositories to map come first; usage outside any repository
        // next, though no mapping can assign it.
        ledger?.unassigned[0]
          ? showEntry(ledger.unassigned[0].key)
          : setTab("projects"),
      plans: () => setTab("subscriptions"),
    },
  });
  const openChecks = checks.filter((c) => c.status === "attention").length;
  const dataNeedsLook = checks.some(
    (c) => (c.id === "uploads" || c.id === "rates") && c.status === "attention",
  );
  const mismatches = mismatchesQuery.isSuccess
    ? mismatchesQuery.data.mismatches
    : [];
  const undeclared = new Map<number, AiProvider[]>();
  for (const mismatch of mismatches) {
    const tools = undeclared.get(mismatch.userId) ?? [];
    if (!tools.includes(mismatch.provider)) {
      undeclared.set(mismatch.userId, [...tools, mismatch.provider]);
    }
  }
  const stale = overviewQuery.isPlaceholderData && "opacity-60";

  return (
    <EstimateMoneyContext.Provider value={estimates}>
      <div className={cn("flex flex-col gap-8", PROVIDER_COLOR_SCOPE)}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <MonthPicker
              month={month}
              onChange={(next) =>
                navigate({ search: (prev) => ({ ...prev, month: next }) })
              }
            />
            {completeness?.inProgress && (
              <p className="text-sm text-muted-foreground">
                In progress
                {completeness.requiredThrough !== null &&
                  `, usage through ${shortDate(completeness.requiredThrough)}`}
                . Figures change until the month is over.
              </p>
            )}
          </div>
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

        {overviewQuery.isPending ? (
          <div
            role="status"
            aria-label="Loading the bill"
            className="space-y-4"
          >
            <Skeleton className="h-24 w-80" />
            <Skeleton className="h-14 w-full" />
          </div>
        ) : overviewQuery.isError ? (
          <QueryError
            error={overviewQuery.error}
            what="the billing overview"
            onRetry={() => overviewQuery.refetch()}
          />
        ) : (
          overview &&
          ledger && (
            <div
              className={cn(
                "transition-opacity motion-reduce:transition-none",
                stale,
              )}
            >
              {overview.lines.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border/80 p-8 text-center">
                  <p className="font-medium">
                    Nothing to bill for {monthLabel(overview.month)}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    No AI usage that bills was synced for this month and no
                    subscription covers it.
                  </p>
                </div>
              ) : (
                <Statement
                  overview={overview}
                  ledger={ledger}
                  segments={segments}
                  highlighted={highlighted}
                  onHighlight={setHighlighted}
                  onSelect={selectSegment}
                />
              )}
            </div>
          )
        )}

        <ExportChecks checks={checks} />

        {overview && ledger && (
          <Tabs
            value={tab}
            onValueChange={(value) => {
              const next = TABS.find((option) => option === value);
              if (next) {
                setFocusRow(null);
                setTab(next);
              }
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
              <TabsTrigger value="projects" className={TAB_TRIGGER}>
                Projects
                <Count value={ledger.projects.length} />
              </TabsTrigger>
              <TabsTrigger value="people" className={TAB_TRIGGER}>
                People
                <Count value={people.length} />
              </TabsTrigger>
              <TabsTrigger value="subscriptions" className={TAB_TRIGGER}>
                Subscriptions
                <Count value={overview.subscriptions.length} />
                {mismatches.length > 0 && <Dot label="plans nobody declared" />}
              </TabsTrigger>
              <TabsTrigger value="copilot" className={TAB_TRIGGER}>
                <AiProviderIcon provider="copilot" className="size-4" plain />
                GitHub Copilot
                {meteredQuery.isSuccess &&
                  needsAttention(meteredQuery.data) && (
                    <Dot label="needs attention" />
                  )}
              </TabsTrigger>
              <TabsTrigger value="data" className={TAB_TRIGGER}>
                Uploads and rates
                {dataNeedsLook && <Dot label="needs attention" />}
              </TabsTrigger>
              <TabsTrigger value="rules" className={TAB_TRIGGER}>
                <Info className="size-4" aria-hidden />
                How billing works
              </TabsTrigger>
            </TabsList>

            <TabsContent value="projects" className="mt-5">
              {overview.lines.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  No project is billed for {monthLabel(overview.month)}.
                </p>
              ) : (
                <ProjectLedger
                  overview={overview}
                  ledger={ledger}
                  open={open}
                  onToggle={toggle}
                  focusRow={focusRow}
                  steps={steps}
                  undeclared={undeclared}
                  highlighted={highlighted}
                  onHighlight={setHighlighted}
                />
              )}
            </TabsContent>
            <TabsContent value="people" className="mt-5">
              {people.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  Nobody has AI usage or a subscription in{" "}
                  {monthLabel(overview.month)}.
                </p>
              ) : (
                <PeopleTable
                  overview={overview}
                  people={people}
                  verdicts={verdicts}
                  undeclared={undeclared}
                />
              )}
            </TabsContent>
            <TabsContent value="subscriptions" className="mt-5 space-y-5">
              <UndeclaredPlans overview={overview} mismatches={mismatches} />
              <SubscriptionValueChart overview={overview} />
              <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
                <SubscriptionMonths
                  billingCurrency={overview.billingCurrency}
                  subscriptions={overview.subscriptions}
                  developers={overview.developers}
                />
              </div>
            </TabsContent>
            <TabsContent value="copilot" className="mt-5">
              <CopilotReconciliation
                month={month}
                onMonth={(next) =>
                  navigate({ search: (prev) => ({ ...prev, month: next }) })
                }
              />
            </TabsContent>
            <TabsContent value="data" className="mt-5 space-y-10">
              <DataChecks
                query={completenessQuery}
                uploads={uploads}
                completeness={completeness}
              />
              <section
                aria-labelledby="rates-heading"
                className="flex max-w-3xl flex-col gap-2"
              >
                <h3 id="rates-heading" className="text-base font-semibold">
                  Exchange rates
                </h3>
                <ExchangeRatesPanel
                  key={overview.month}
                  month={overview.month}
                  billingCurrency={overview.billingCurrency}
                  rates={overview.exchangeRates}
                  missingRates={overview.missingRates}
                  pendingRates={overview.pendingRates}
                />
              </section>
            </TabsContent>
            <TabsContent value="rules" className="mt-5">
              <BillingRules timeZone={overview.timeZone} />
            </TabsContent>
          </Tabs>
        )}
      </div>
    </EstimateMoneyContext.Provider>
  );
}

/** The uploads check in full, once this month's answer is in. */
function DataChecks({
  query,
  uploads,
  completeness,
}: {
  query: UseQueryResult<AiBillingCompleteness, Error>;
  uploads: readonly DeveloperUploads[] | undefined;
  completeness: AiBillingCompleteness | undefined;
}) {
  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="the month's uploads"
        onRetry={() => query.refetch()}
      />
    );
  }
  if (!completeness || !uploads) {
    return (
      <div role="status" aria-label="Checking uploads" className="space-y-2">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-48 w-full" />
      </div>
    );
  }
  return <UploadsPanel completeness={completeness} uploads={uploads} />;
}

function Count({ value }: { value: number }) {
  return value > 0 ? (
    <span className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none font-semibold text-muted-foreground tabular-nums">
      {formatCount(value)}
    </span>
  ) : null;
}

/** A small amber mark on a tab whose content needs a look. */
function Dot({ label }: { label: string }) {
  return (
    <span
      className="size-1.5 rounded-full bg-[#c98500] dark:bg-[#fab219]"
      role="img"
      aria-label={label}
    />
  );
}

/**
 * The month's checks before export. A check whose answer is still loading,
 * or is another month's answer held while this one loads, is pending; one
 * whose query failed needs attention, since it could not be made.
 */
function exportChecks({
  completenessQuery,
  uploads,
  overviewQuery,
  mismatchesQuery,
  open,
}: {
  completenessQuery: UseQueryResult<AiBillingCompleteness, Error>;
  uploads: readonly DeveloperUploads[] | undefined;
  overviewQuery: UseQueryResult<AiBillingOverview, Error>;
  mismatchesQuery: UseQueryResult<AiSubscriptionMismatchList, Error>;
  open: Readonly<
    Record<"uploads" | "rates" | "projects" | "plans", () => void>
  >;
}): ExportCheck[] {
  const loadingBill: CheckResult = {
    status: "pending",
    summary: "Loading the bill",
  };
  const overview =
    overviewQuery.isSuccess && !overviewQuery.isPlaceholderData
      ? overviewQuery.data
      : undefined;
  const billFailed = overviewQuery.isError;

  const uploadsResult: CheckResult = completenessQuery.isError
    ? {
        status: "attention",
        summary: "Could not check what every machine uploaded",
      }
    : completenessQuery.isSuccess &&
        !completenessQuery.isPlaceholderData &&
        uploads
      ? uploadsCheck(completenessQuery.data, uploads)
      : { status: "pending", summary: "Checking what every machine uploaded" };
  const ratesResult: CheckResult = billFailed
    ? { status: "attention", summary: "Could not load the bill" }
    : overview
      ? ratesCheck(overview)
      : loadingBill;
  const projectsResult: CheckResult = billFailed
    ? { status: "attention", summary: "Could not load the bill" }
    : overview
      ? mappingsCheck(overview.lines, overview.billingCurrency)
      : loadingBill;
  const plansResult: CheckResult = mismatchesQuery.isError
    ? { status: "attention", summary: "Could not compare reported plans" }
    : mismatchesQuery.isSuccess
      ? plansCheck(mismatchesQuery.data)
      : { status: "pending", summary: "Comparing reported plans" };

  return [
    { id: "uploads", label: "Uploads", ...uploadsResult, onOpen: open.uploads },
    {
      id: "rates",
      label: "Exchange rates",
      ...ratesResult,
      onOpen: open.rates,
    },
    {
      id: "projects",
      label: "Projects",
      ...projectsResult,
      onOpen: open.projects,
    },
    { id: "plans", label: "Plans", ...plansResult, onOpen: open.plans },
  ];
}
