import * as React from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowLeft } from "lucide-react";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import {
  aiAdminQueries,
  type AiBillingUsage,
  type AiDayUsage,
  type AiDeveloperMonth,
} from "@/lib/api/queries/ai-admin";
import { AI_PROVIDERS } from "@/lib/api/queries/ai-usage";
import { cn } from "@/lib/utils";
import { MonthPicker } from "../-components/month-picker";
import { QueryError } from "../-components/query-error";
import {
  NO_FILTERS,
  addUsage,
  breakdown,
  dailySeries,
  emptyUsage,
  filterDays,
  projectFilterValue,
  type Breakdown,
  type UsageFilters,
} from "../-lib/daily";
import {
  MONTH_PATTERN,
  formatCount,
  formatEstimate,
  formatTokens,
  monthLabel,
  previousMonth,
  shortDate,
} from "../-lib/format";
import { PROVIDER_COLOR_SCOPE } from "../-lib/providers";
import { UNASSIGNED_LABEL } from "../-lib/billing";
import { BillingTable } from "./-components/billing-table";
import { DailyCostChart } from "./-components/daily-cost-chart";
import { SubscriptionMonths } from "./-components/subscription-months";

const searchSchema = z.object({
  month: z.string().regex(MONTH_PATTERN).optional().catch(undefined),
});

export const Route = createFileRoute("/_layout/admin/ai-billing/$userId")({
  validateSearch: searchSchema,
  component: DeveloperMonthPage,
});

function DeveloperMonthPage() {
  const { userId: rawUserId } = Route.useParams();
  const search = Route.useSearch();
  const month = search.month ?? previousMonth();
  const navigate = useNavigate({ from: Route.fullPath });
  const userId = Number(rawUserId);
  const validUser = Number.isInteger(userId) && userId > 0;

  const query = useQuery({
    ...aiAdminQueries.developerMonth(month, userId),
    enabled: validUser,
    placeholderData: keepPreviousData,
  });

  return (
    <div className={cn("flex flex-col gap-6", PROVIDER_COLOR_SCOPE)}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <Button asChild variant="ghost" size="sm" className="-ml-2 w-fit">
            <Link to="/admin/ai-billing" search={{ month }}>
              <ArrowLeft className="size-4" />
              All developers
            </Link>
          </Button>
          <h2 className="text-xl font-semibold">
            {query.data?.developer.fullName ?? "Developer"}
            <span className="ml-2 text-base font-normal text-muted-foreground">
              {monthLabel(month)}
            </span>
          </h2>
          {query.data && (
            <p className="text-sm text-muted-foreground">
              {query.data.developer.email} · days in {query.data.timeZone}
            </p>
          )}
        </div>
        <MonthPicker
          month={month}
          onChange={(next) =>
            navigate({ search: (prev) => ({ ...prev, month: next }) })
          }
        />
      </div>

      {!validUser ? (
        <p className="text-sm text-destructive" role="alert">
          That is not a developer.
        </p>
      ) : query.isPending ? (
        <div className="flex flex-col gap-3" role="status" aria-label="Loading">
          <Skeleton className="h-32 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      ) : query.isError ? (
        <QueryError
          error={query.error}
          what="this developer's month"
          onRetry={() => query.refetch()}
        />
      ) : (
        <div
          className={cn(
            "flex flex-col gap-6 transition-opacity",
            query.isPlaceholderData && "opacity-60",
          )}
        >
          {/* Keyed by the loaded month, so its filters start over. */}
          <DeveloperMonthView
            key={query.data.month}
            month={query.data.month}
            data={query.data}
          />
        </div>
      )}
    </div>
  );
}

function DeveloperMonthView({
  month,
  data,
}: {
  month: string;
  data: AiDeveloperMonth;
}) {
  const [filters, setFilters] = React.useState<UsageFilters>(NO_FILTERS);
  const rows = filterDays(data.dailyUsage, filters);
  const plans = new Map(
    data.subscriptions.map((subscription) => [
      subscription.subscriptionId,
      subscription.plan,
    ]),
  );
  const billing = new Map<string, string>();
  for (const row of data.dailyUsage) {
    billing.set(
      `${row.day}:${row.provider}`,
      row.subscriptionId === null
        ? "API"
        : `Subscription · ${plans.get(row.subscriptionId) ?? "declared plan"}`,
    );
  }

  if (data.lines.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-border/80 p-8 text-center">
        <p className="font-medium">Nothing to bill for {monthLabel(month)}</p>
        <p className="text-sm text-muted-foreground">
          {data.developer.fullName} has no synced AI usage and no subscription
          in this month.
        </p>
      </div>
    );
  }

  return (
    <>
      <section
        aria-labelledby="developer-billing-heading"
        className="flex flex-col gap-2"
      >
        <h3 id="developer-billing-heading" className="text-lg font-semibold">
          Billing
        </h3>
        <div className="rounded-xl border border-border/60 bg-card/60">
          <BillingTable
            month={month}
            lines={data.lines}
            developers={[data.developer]}
            subscriptions={data.subscriptions}
            groupBy="project"
            linkDevelopers={false}
          />
        </div>
        {data.subscriptions.length > 0 && (
          <div className="rounded-xl border border-border/60 bg-card/60">
            <SubscriptionMonths
              subscriptions={data.subscriptions}
              developers={[data.developer]}
              showDeveloper={false}
            />
          </div>
        )}
      </section>

      <section
        aria-labelledby="developer-usage-heading"
        className="flex flex-col gap-3"
      >
        <div>
          <h3 id="developer-usage-heading" className="text-lg font-semibold">
            Usage by day
          </h3>
          <p className="text-sm text-muted-foreground">
            API-equivalent cost per local day. Admins see usage per day by
            provider, model, machine and project, never by hour or session.
          </p>
        </div>
        <Filters data={data} filters={filters} onChange={setFilters} />
        <div className="rounded-xl border border-border/60 bg-card/60 p-4">
          <DailyCostChart
            points={dailySeries(rows, data.firstDay, data.lastDay)}
            billing={billing}
          />
        </div>
        <Breakdowns data={data} rows={rows} billing={billing} />
      </section>
    </>
  );
}

const ALL = "all";
/** token-ledger's key for usage without project metadata; never mappable. */
const UNATTRIBUTED_KEY = "unattributed";

function Filters({
  data,
  filters,
  onChange,
}: {
  data: AiDeveloperMonth;
  filters: UsageFilters;
  onChange: (filters: UsageFilters) => void;
}) {
  const providers = AI_PROVIDERS.filter((provider) =>
    data.dailyUsage.some((row) => row.provider === provider),
  );
  const models = [...new Set(data.dailyUsage.map((row) => row.model))].sort();
  const projects = breakdown(data.dailyUsage, projectFilterValue, (row) => ({
    label: row.project?.projectName ?? UNASSIGNED_LABEL,
  }));
  const active = Object.values(filters).some((value) => value !== null);

  return (
    <div className="flex flex-wrap items-center gap-2" aria-label="Filters">
      <FilterSelect
        label="Provider"
        value={filters.provider}
        options={providers.map((provider) => ({
          value: provider,
          label: AI_PROVIDER_LABELS[provider],
        }))}
        onChange={(value) =>
          onChange({
            ...filters,
            provider:
              AI_PROVIDERS.find((provider) => provider === value) ?? null,
          })
        }
      />
      <FilterSelect
        label="Model"
        value={filters.model}
        options={models.map((model) => ({
          value: model,
          label: model || "Unknown model",
        }))}
        onChange={(model) => onChange({ ...filters, model })}
      />
      <FilterSelect
        label="Machine"
        value={filters.machineId}
        options={data.machines.map((machine) => ({
          value: machine.machineId,
          label: machine.label,
        }))}
        onChange={(machineId) => onChange({ ...filters, machineId })}
      />
      <FilterSelect
        label="Project"
        value={filters.project}
        options={projects.map((project) => ({
          value: project.key,
          label: project.label,
        }))}
        onChange={(project) => onChange({ ...filters, project })}
      />
      {active && (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => onChange(NO_FILTERS)}
        >
          Clear filters
        </Button>
      )}
    </div>
  );
}

function FilterSelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string | null;
  options: readonly { value: string; label: string }[];
  onChange: (value: string | null) => void;
}) {
  return (
    <Select
      value={value ?? ALL}
      onValueChange={(next) => onChange(next === ALL ? null : next)}
    >
      <SelectTrigger className="w-44" aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={ALL}>All {label.toLowerCase()}s</SelectItem>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function Breakdowns({
  data,
  rows,
  billing,
}: {
  data: AiDeveloperMonth;
  rows: readonly AiDayUsage[];
  billing: ReadonlyMap<string, string>;
}) {
  const machines = new Map(data.machines.map((m) => [m.machineId, m.label]));
  const total = rows.reduce(
    (sum, row) => addUsage(sum, row.usage),
    emptyUsage(),
  );
  const tables: Record<string, { heading: string; items: Breakdown[] }> = {
    days: {
      heading: "Day",
      items: breakdown(
        rows,
        (row) => `${row.day}:${row.provider}`,
        (row) => ({
          label: shortDate(row.day),
          detail: `${AI_PROVIDER_LABELS[row.provider]} · ${billing.get(`${row.day}:${row.provider}`) ?? "API"}`,
        }),
      ).sort((a, b) => a.key.localeCompare(b.key)),
    },
    providers: {
      heading: "Provider",
      items: breakdown(
        rows,
        (row) => row.provider,
        (row) => ({
          label: AI_PROVIDER_LABELS[row.provider],
        }),
      ),
    },
    models: {
      heading: "Model",
      items: breakdown(
        rows,
        (row) => `${row.provider}:${row.model}`,
        (row) => ({
          label: row.model || "Unknown model",
          detail: AI_PROVIDER_LABELS[row.provider],
        }),
      ),
    },
    machines: {
      heading: "Machine",
      items: breakdown(
        rows,
        (row) => row.machineId,
        (row) => ({
          label: machines.get(row.machineId) ?? "Unknown machine",
        }),
      ),
    },
    projects: {
      heading: "Project key",
      items: breakdown(
        rows,
        (row) => row.projectKey,
        (row) => ({
          label: row.projectKey,
          detail: row.project
            ? row.project.projectName
            : row.projectKey === UNATTRIBUTED_KEY
              ? `${UNASSIGNED_LABEL}: no project metadata`
              : `${UNASSIGNED_LABEL}: unmapped or stale key`,
        }),
      ),
    },
  };

  return (
    <Tabs defaultValue="days">
      <TabsList>
        <TabsTrigger value="days">Days</TabsTrigger>
        <TabsTrigger value="providers">Providers</TabsTrigger>
        <TabsTrigger value="models">Models</TabsTrigger>
        <TabsTrigger value="machines">Machines</TabsTrigger>
        <TabsTrigger value="projects">Projects</TabsTrigger>
      </TabsList>
      {Object.entries(tables).map(([key, table]) => (
        <TabsContent key={key} value={key}>
          <div className="rounded-xl border border-border/60 bg-card/60">
            <BreakdownTable
              heading={table.heading}
              items={table.items}
              total={total}
            />
          </div>
        </TabsContent>
      ))}
    </Tabs>
  );
}

function BreakdownTable({
  heading,
  items,
  total,
}: {
  heading: string;
  items: readonly Breakdown[];
  total: AiBillingUsage;
}) {
  if (items.length === 0) {
    return (
      <p className="p-4 text-sm text-muted-foreground">
        No usage matches the filters.
      </p>
    );
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{heading}</TableHead>
          <TableHead className="text-right">Records</TableHead>
          <TableHead className="text-right">Tokens</TableHead>
          <TableHead className="text-right">API-equivalent</TableHead>
          <TableHead className="text-right">Share of cost</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {items.map((item) => (
          <TableRow key={item.key}>
            <TableCell>
              <div className="font-medium">{item.label}</div>
              {item.detail && (
                <div className="text-xs text-muted-foreground">
                  {item.detail}
                </div>
              )}
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {formatCount(item.usage.records)}
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {formatTokens(item.usage.tokens.total)}
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {formatEstimate(item.usage)}
            </TableCell>
            <TableCell className="text-right tabular-nums text-muted-foreground">
              {total.apiEquivalentUsd > 0
                ? `${Math.round((item.usage.apiEquivalentUsd / total.apiEquivalentUsd) * 100)}%`
                : "—"}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
