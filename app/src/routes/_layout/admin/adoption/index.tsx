import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { z } from "zod";
import { Skeleton } from "@/components/ui/skeleton";
import { aiAdminQueries, type AiAdoption } from "@/lib/api/queries/ai-admin";
import { aiUsageReportQueries } from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { QueryError } from "../-components/query-error";
import {
  ADOPTION_WINDOWS,
  everyone,
  indexDays,
  windowOf,
  type AdoptionWindow,
} from "../-lib/adoption";
import { estimateMoney } from "../-lib/estimates";
import { PROVIDER_COLOR_SCOPE } from "../-lib/providers";
import { EstimateMoneyContext } from "../-lib/use-estimate-money";
import { HabitDepth } from "./-components/habit-depth";
import { PersonPanel } from "./-components/person-panel";
import { Roster } from "./-components/roster";
import { Segmented } from "./-components/segmented";
import { Summary } from "./-components/summary";
import { ToolReach } from "./-components/tool-reach";
import { VolumeChart } from "./-components/volume-chart";
import { VolumeStrip } from "./-components/volume-strip";

const searchSchema = z.object({
  weeks: z
    .union([z.literal(4), z.literal(8), z.literal(12)])
    .optional()
    .catch(undefined),
  measure: z.enum(["cost", "tokens"]).optional().catch(undefined),
  period: z.enum(["week", "month"]).optional().catch(undefined),
  scale: z.enum(["linear", "log"]).optional().catch(undefined),
  person: z.number().int().optional().catch(undefined),
});

export const Route = createFileRoute("/_layout/admin/adoption/")({
  validateSearch: searchSchema,
  component: AdoptionPage,
});

const WINDOW_OPTIONS = ADOPTION_WINDOWS.map((weeks) => ({
  value: weeks,
  label: `Last ${weeks} weeks`,
}));

function AdoptionPage() {
  const query = useQuery(aiAdminQueries.adoption());

  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="AI adoption"
        onRetry={() => query.refetch()}
      />
    );
  }
  if (!query.data) {
    return (
      <div className="flex flex-col gap-6" aria-busy>
        <Skeleton className="h-8 w-80" />
        <Skeleton className="h-64 w-full" />
        <Skeleton className="h-96 w-full" />
      </div>
    );
  }
  return <Adoption adoption={query.data} />;
}

function Adoption({ adoption }: { adoption: AiAdoption }) {
  const search = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const weeks: AdoptionWindow = search.weeks ?? 4;
  const range = React.useMemo(
    () => windowOf(adoption.today, weeks),
    [adoption.today, weeks],
  );
  const index = React.useMemo(() => indexDays(adoption.days), [adoption.days]);
  const people = React.useMemo(
    () => everyone(adoption, index, range),
    [adoption, index, range],
  );

  // Costs show in the bill's currency at the display rate of today's month,
  // in dollars until it is known.
  const rates = useQuery(aiUsageReportQueries.aiDisplayRates(adoption.today));
  const displayRate =
    rates.data?.rates.find((rate) => rate.currency === adoption.currency)
      ?.perUsd ?? null;
  const money = React.useMemo(
    () => estimateMoney(adoption.currency, null, displayRate),
    [adoption.currency, displayRate],
  );

  const update = (changes: Partial<z.infer<typeof searchSchema>>) =>
    void navigate({
      search: (previous) => ({ ...previous, ...changes }),
      replace: true,
      resetScroll: false,
    });
  // What selected a person, to take focus back once their panel closes.
  const opener = React.useRef<HTMLElement | SVGElement | null>(null);
  const select = (userId: number) => {
    opener.current =
      document.activeElement instanceof HTMLElement ||
      document.activeElement instanceof SVGElement
        ? document.activeElement
        : null;
    update({ person: userId });
  };
  const selected = people.find(
    (stats) => stats.person.userId === search.person,
  );

  return (
    <EstimateMoneyContext.Provider value={money}>
      <div className={cn("flex flex-col gap-10", PROVIDER_COLOR_SCOPE)}>
        <div className="flex flex-wrap items-center gap-4">
          <Segmented
            label="Period"
            options={WINDOW_OPTIONS}
            value={weeks}
            onChange={(value) => update({ weeks: value })}
          />
          <p className="max-w-[70ch] text-[13px] text-muted-foreground">
            All chat and agent use from Token Ledger and GitHub Copilot. Code
            completions are left out. Costs are at API prices, and Copilot at
            the credits GitHub charged.
          </p>
        </div>

        <div className="grid items-start gap-10 lg:grid-cols-[minmax(0,4fr)_minmax(0,8fr)]">
          <Summary adoption={adoption} people={people} range={range} />
          <VolumeChart
            adoption={adoption}
            measure={search.measure ?? "cost"}
            period={search.period ?? "week"}
            onMeasure={(measure) => update({ measure })}
            onPeriod={(period) => update({ period })}
          />
        </div>

        <Roster
          adoption={adoption}
          people={people}
          index={index}
          range={range}
          onSelect={select}
        />

        <div className="grid items-start gap-10 lg:grid-cols-2">
          <HabitDepth
            people={people}
            weeks={weeks}
            scale={search.scale ?? "linear"}
            onScale={(scale) => update({ scale })}
            onSelect={select}
          />
          <div className="flex flex-col gap-10">
            <VolumeStrip adoption={adoption} people={people} range={range} />
            <ToolReach adoption={adoption} range={range} weeks={weeks} />
          </div>
        </div>
      </div>

      <PersonPanel
        adoption={adoption}
        stats={selected}
        people={people}
        index={index}
        range={range}
        weeks={weeks}
        scale={search.scale ?? "linear"}
        onClose={() => update({ person: undefined })}
        onClosed={() => opener.current?.focus()}
      />
    </EstimateMoneyContext.Provider>
  );
}
