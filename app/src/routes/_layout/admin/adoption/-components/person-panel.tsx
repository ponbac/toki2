import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { cn } from "@/lib/utils";
import { aiAdminQueries, type AiAdoption } from "@/lib/api/queries/ai-admin";
import {
  addDays,
  mondayOf,
  personStats,
  previousWindow,
  covers,
  VOLUME_WEEKS,
  volumeBars,
  volumeStep,
  type DayIndex,
  type DayRange,
  type PersonStats,
} from "../../-lib/adoption";
import { formatEstimateAmount } from "../../-lib/estimates";
import { formatShare, shortDate, weekdayDate } from "../../-lib/format";
import { PROVIDER_COLOR_SCOPE } from "../../-lib/providers";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { formatKnownCost } from "./format";
import { HABIT_COLORS, HABIT_LABELS, VOLUME_STEP_COLORS } from "./colors";
import { DepthPlot, type DepthScale } from "./habit-depth";
import { VolumeBars } from "./volume-chart";

/** One person's page, beside the tab: their days, how they compare, their
 * tools per week and their models. */
export function PersonPanel({
  adoption,
  stats,
  people,
  index,
  range,
  weeks,
  scale,
  onClose,
  onClosed,
}: {
  adoption: AiAdoption;
  stats: PersonStats | undefined;
  people: readonly PersonStats[];
  index: DayIndex;
  range: DayRange;
  weeks: number;
  scale: DepthScale;
  onClose: () => void;
  /** Where focus goes once the panel has closed. */
  onClosed: () => void;
}) {
  return (
    <DialogPrimitive.Root
      open={stats !== undefined}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-foreground/25 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0 motion-reduce:animate-none" />
        <DialogPrimitive.Content
          // No trigger opens the panel: focus returns to what selected the
          // person instead.
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            onClosed();
          }}
          className={cn(
            "fixed inset-y-0 right-0 z-50 w-full max-w-xl overflow-y-auto border-l border-border bg-background shadow-lg duration-200 data-[state=closed]:animate-out data-[state=closed]:slide-out-to-right data-[state=open]:animate-in data-[state=open]:slide-in-from-right motion-reduce:animate-none",
            // The panel renders outside the tab: it brings the tool colors.
            PROVIDER_COLOR_SCOPE,
          )}
        >
          <DialogPrimitive.Close className="absolute top-5 right-5 rounded-md border border-border bg-card p-1.5 opacity-80 transition-opacity hover:opacity-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring motion-reduce:transition-none">
            <X className="size-4" />
            <span className="sr-only">Close</span>
          </DialogPrimitive.Close>
          {stats ? (
            <PersonPage
              adoption={adoption}
              stats={stats}
              people={people}
              index={index}
              range={range}
              weeks={weeks}
              scale={scale}
            />
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function PersonPage({
  adoption,
  stats,
  people,
  index,
  range,
  weeks,
  scale,
}: {
  adoption: AiAdoption;
  stats: PersonStats;
  people: readonly PersonStats[];
  index: DayIndex;
  range: DayRange;
  weeks: number;
  scale: DepthScale;
}) {
  const money = useEstimateMoney();
  const { person } = stats;
  const before = previousWindow(range);
  const previous = covers(adoption, before)
    ? personStats(person, adoption.days, index, before)
    : null;
  const bars = React.useMemo(
    () => volumeBars(adoption, "cost", "week", person.userId),
    [adoption, person.userId],
  );
  const users = people.filter((entry) => entry.activeDays > 0);
  const rank = users.filter((entry) => entry.share > stats.share).length + 1;
  const sources = [
    ...person.seats.map(
      (seat) =>
        `${AI_PROVIDER_LABELS[seat.provider]} seat since ${longDate(seat.since)}`,
    ),
    person.firstSyncedOn
      ? `first Token Ledger sync ${shortDate(person.firstSyncedOn)}`
      : null,
  ].filter((source): source is string => source !== null);

  return (
    <div className="flex flex-col gap-8 p-8">
      <header className="flex flex-col gap-2 pr-10">
        <DialogPrimitive.Title className="text-2xl font-bold">
          {person.fullName}
        </DialogPrimitive.Title>
        {stats.habit !== "unseen" ? (
          <span
            className="w-fit rounded-full px-2.5 py-0.5 text-xs font-semibold"
            style={{
              background: HABIT_COLORS[stats.habit],
              color:
                stats.habit === "daily"
                  ? "hsl(var(--primary-foreground))"
                  : "hsl(var(--foreground))",
            }}
          >
            {HABIT_LABELS[stats.habit]}
          </span>
        ) : null}
      </header>

      <div className="flex flex-col gap-2">
        <DialogPrimitive.Description asChild>
          <p className="max-w-[46ch] text-lg leading-normal">
            {stats.activeDays > 0 ? (
              <>
                Used AI on{" "}
                <b className="font-extrabold tabular-nums">
                  {stats.activeWorkDays} of {stats.workDays}
                </b>{" "}
                work days in the last {weeks} weeks
                {previous &&
                previous.activeWorkDays !== stats.activeWorkDays ? (
                  <span className="text-sm font-semibold text-muted-foreground">
                    {" "}
                    ({previous.activeWorkDays} the {weeks} weeks before)
                  </span>
                ) : null}
                .{" "}
                <b className="font-extrabold tabular-nums">
                  {formatKnownCost(stats.costUsd, stats.unpricedRecords, money)}
                </b>{" "}
                at API prices
                {stats.tools[0]
                  ? `, mostly ${AI_PROVIDER_LABELS[stats.tools[0]]}`
                  : ""}
                .
              </>
            ) : (
              `No AI use reported in the last ${weeks} weeks.`
            )}
          </p>
        </DialogPrimitive.Description>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {sources.length > 0
            ? `${capitalize(sources.join(", "))}.`
            : "No Copilot seat and no Token Ledger sync, so Toki cannot see this person's AI use."}
        </p>
      </div>

      <PanelSection title={`The last ${VOLUME_WEEKS} weeks`}>
        <Calendar adoption={adoption} index={index} userId={person.userId} />
      </PanelSection>

      {stats.activeDays > 0 ? (
        <PanelSection
          title="Compared with everyone using AI"
          description={`${rank === 1 ? "The most" : `${ordinal(rank)} most`} work days with AI of ${users.length} people. Each grey dot is a colleague.`}
        >
          <DepthPlot
            people={people}
            scale={scale}
            highlight={person.userId}
            height={240}
          />
        </PanelSection>
      ) : null}

      {bars.some((bar) => bar.total > 0 || bar.unpricedRecords > 0) ? (
        <PanelSection
          title="AI per week, by tool"
          description="At API prices. GitHub Copilot is what GitHub charged in credits."
        >
          <VolumeBars bars={bars} measure="cost" period="week" money={money} />
        </PanelSection>
      ) : null}

      <Models userId={person.userId} range={range} weeks={weeks} />
    </div>
  );
}

function PanelSection({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3">
      <div>
        <h3 className="text-base font-bold">{title}</h3>
        {description ? (
          <p className="mt-0.5 text-[13px] text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {children}
    </section>
  );
}

const WEEKDAYS = ["Mon", "", "Wed", "", "Fri", "", "Sun"];

/** Weeks across, weekdays down, as a contribution calendar. */
function Calendar({
  adoption,
  index,
  userId,
}: {
  adoption: AiAdoption;
  index: DayIndex;
  userId: number;
}) {
  const money = useEstimateMoney();
  const byDay = index.get(userId);
  const thisWeek = mondayOf(adoption.today);
  const weeks = Array.from({ length: VOLUME_WEEKS }, (_, week) =>
    addDays(thisWeek, -7 * (VOLUME_WEEKS - 1 - week)),
  );
  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-1">
        <div className="grid grid-rows-7 gap-1 pr-1 text-[10px] text-muted-foreground">
          {WEEKDAYS.map((label, row) => (
            <span key={row} className="flex h-4 items-center">
              {label}
            </span>
          ))}
        </div>
        {weeks.map((week) => (
          <div key={week} className="grid grid-rows-7 gap-1">
            {Array.from({ length: 7 }, (_, weekday) => {
              const day = addDays(week, weekday);
              if (day > adoption.today) {
                return <span key={day} className="size-4" />;
              }
              const use = byDay?.get(day);
              return (
                <span
                  key={day}
                  title={
                    use
                      ? `${weekdayDate(day)}: ${formatEstimateAmount(use.costUsd, money)}, ${use.tools.map((tool) => AI_PROVIDER_LABELS[tool]).join(", ")}`
                      : `${weekdayDate(day)}: no AI`
                  }
                  className={
                    use
                      ? "size-4 rounded-[4px]"
                      : "size-4 rounded-[4px] shadow-[inset_0_0_0_1px_var(--viz-grid)]"
                  }
                  style={
                    use
                      ? {
                          background:
                            VOLUME_STEP_COLORS[volumeStep(use.costUsd)],
                        }
                      : undefined
                  }
                />
              );
            })}
          </div>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">
        From the week of {shortDate(weeks[0] ?? thisWeek)}, shaded by how much
        AI was used each day.
      </p>
    </div>
  );
}

function Models({
  userId,
  range,
  weeks,
}: {
  userId: number;
  range: DayRange;
  weeks: number;
}) {
  const query = useQuery(
    aiAdminQueries.adoptionModels(userId, range.from, range.to),
  );
  const models = query.data?.models ?? [];
  const total = models.reduce((sum, model) => sum + model.records, 0);
  if (query.isError) {
    return (
      <PanelSection title="Models">
        <p className="text-sm text-muted-foreground">
          Could not load this person's models.
        </p>
      </PanelSection>
    );
  }
  if (!query.data || total === 0) return null;
  return (
    <PanelSection
      title="Models"
      description={`Share of the requests Token Ledger saw in the last ${weeks} weeks.`}
    >
      <ul className="grid grid-cols-[minmax(0,1fr)_7rem_4.5rem] items-center gap-x-3 gap-y-1.5 text-[13px]">
        {models.slice(0, 6).map((model) => (
          <li key={`${model.tool}:${model.model}`} className="contents">
            <span className="truncate">
              {model.model}
              <span className="ml-1.5 text-xs text-muted-foreground">
                {AI_PROVIDER_LABELS[model.tool]}
              </span>
            </span>
            <span className="h-1.5 overflow-hidden rounded-full bg-[var(--viz-grid)]">
              <span
                className="block h-full rounded-full bg-[var(--viz-seq-4)]"
                style={{ width: `${(model.records / total) * 100}%` }}
              />
            </span>
            <span className="text-right text-xs text-muted-foreground tabular-nums">
              {formatShare(model.records / total)}
            </span>
          </li>
        ))}
      </ul>
    </PanelSection>
  );
}

/** A day with its year, such as `Apr 17, 2023`. */
function longDate(day: string): string {
  return new Date(`${day}T00:00:00Z`).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  });
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function ordinal(n: number): string {
  const suffixes = ["th", "st", "nd", "rd"];
  const rest = n % 100;
  return `${n}${suffixes[(rest - 20) % 10] ?? suffixes[rest] ?? suffixes[0]}`;
}
