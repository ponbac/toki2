import { createFileRoute } from "@tanstack/react-router";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useTimeTrackingData } from "@/hooks/useTimeTrackingData";
import { SearchBar } from "@/routes/_layout/time-tracking/-components/search-bar";
import { Summary } from "@/routes/_layout/time-tracking/-components/summary";
import { TimeEntriesList } from "./-components/time-entries-list";
import { DateRangeSelector } from "./-components/date-range-selector";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { apiErrorToast } from "@/lib/api/errors";
import { userQueries } from "@/lib/api/queries/user";
import { startOfWeek, endOfWeek, format } from "date-fns";
import React from "react";
import { atomWithStorage } from "jotai/utils";
import { useAtom, useAtomValue } from "jotai/react";
import { MergeEntriesSwitch } from "./-components/merge-entries-switch";
import { TimeTrackingSettings } from "./-components/time-tracking-settings";
import {
  buildRememberedTimerParams,
  lastActivityAtom,
  lastProjectAtom,
  rememberLastProjectAtom,
} from "@/lib/time-tracking-preferences";
import { TimeStats } from "./-components/time-stats";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { TIME_TRACKING_PROVIDER_URL } from "@/lib/time-tracking-provider";
import {
  FIRST_TIMER_OF_THE_WEEK_NOTE,
  TRY_CMD_K_NEXT_TIME_NOTE,
} from "@/lib/time-tracking-default-notes";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { NotLockedAlert } from "./-components/not-locked-alert";
import {
  TimerIcon,
  Plus,
  Clock,
  ExternalLink,
  Sparkles,
  List,
  CalendarDays,
} from "lucide-react";
import { toast } from "sonner";
import { NewEntryDialog } from "./-components/new-entry-dialog";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { TimelineView } from "./-components/timeline-view";

export const Route = createFileRoute("/_layout/time-tracking/")({
  loader: async ({ context }) => {
    const [connectionStatus] = await Promise.all([
      context.queryClient.ensureQueryData(
        timeTrackingQueries.connectionStatus(),
      ),
      context.queryClient.ensureQueryData(userQueries.me()),
    ]);

    if (!connectionStatus.connected) {
      return;
    }

    const currentWeekDateRange = getCurrentWeekDateRange();
    void context.queryClient.prefetchQuery(
      timeTrackingQueries.timeEntries(currentWeekDateRange),
    );
    void context.queryClient.prefetchQuery(
      timeTrackingQueries.timeInfo(currentWeekDateRange),
    );
    void context.queryClient.prefetchQuery(timeTrackingQueries.listProjects());
  },
  component: TimeTrackingPage,
});

function getCurrentWeekDateRange() {
  return {
    from: format(startOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
    to: format(endOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
  };
}

const mergeSameDayPersistedAtom = atomWithStorage(
  "time-tracking-mergeSameDay",
  false,
);

const viewModePersistedAtom = atomWithStorage<"list" | "timeline">(
  "time-tracking-viewMode",
  "list",
);

function TimeTrackingPage() {
  const { isAuthenticated } = useTimeTrackingData();

  const [dateRange, setDateRange] = React.useState(getCurrentWeekDateRange);
  const [mergeSameDay, setMergeSameDay] = useAtom(mergeSameDayPersistedAtom);
  const [storedViewMode, setViewMode] = useAtom(viewModePersistedAtom);
  const isDesktop = useMediaQuery("(min-width: 768px)");
  const queryClient = useQueryClient();
  const viewMode = isDesktop ? storedViewMode : "list";
  const [rememberLastProject, setRememberLastProject] = useAtom(
    rememberLastProjectAtom,
  );
  const lastProject = useAtomValue(lastProjectAtom);
  const lastActivity = useAtomValue(lastActivityAtom);
  const [search, setSearch] = React.useState("");
  const { data: user } = useQuery(userQueries.me());
  const isAdmin = user?.roles.includes("Admin") ?? false;

  const { data: timeEntries, isLoading: isTimeEntriesLoading } = useQuery({
    ...timeTrackingQueries.timeEntries({
      from: dateRange.from,
      to: dateRange.to,
    }),
    enabled: isAuthenticated,
  });

  const filteredTimeEntries = React.useMemo(() => {
    return timeEntries?.length
      ? timeEntries.filter((entry) =>
          `${entry.note} ${entry.projectName} ${entry.activityName}`
            .toLowerCase()
            .includes(search.toLowerCase()),
        )
      : [];
  }, [timeEntries, search]);

  const { state: timerState } = useTimeTrackingTimer();
  const { mutate: startTimer, isPending: isStartingTimer } =
    timeTrackingMutations.useStartTimer();
  const { mutate: createProjectRegistration } =
    timeTrackingMutations.useCreateProjectRegistration({
      onSuccess: () => {
        setIsNewEntryOpen(false);
        toast.success("Entry created");
      },
      onError: apiErrorToast("Failed to create entry"),
    });

  const [isNewEntryOpen, setIsNewEntryOpen] = React.useState(false);

  const onStartTimer = React.useCallback(() => {
    startTimer({
      userNote: FIRST_TIMER_OF_THE_WEEK_NOTE,
      ...buildRememberedTimerParams({
        rememberLastProject,
        lastProject,
        lastActivity,
      }),
    });
  }, [rememberLastProject, lastProject, lastActivity, startTimer]);

  const prefetchProjects = React.useCallback(() => {
    void queryClient.prefetchQuery(timeTrackingQueries.listProjects());
  }, [queryClient]);

  return (
    <div className="min-h-screen">
      {!isAuthenticated ? (
        <NotConnectedState
          isAdmin={isAdmin}
          rememberLastProject={rememberLastProject}
          setRememberLastProject={setRememberLastProject}
        />
      ) : (
        <div className="animate-fade-in">
          {/* Hero Header */}
          <header className="relative overflow-hidden border-b border-border/50 bg-gradient-to-b from-card/80 to-background px-6 pb-6 pt-8">
            {/* Background decoration */}
            <div className="pointer-events-none absolute inset-0 overflow-hidden">
              <div className="absolute -right-20 -top-20 h-64 w-64 rounded-full bg-primary/5 blur-3xl" />
              <div className="bg-primary/3 absolute -left-20 top-20 h-48 w-48 rounded-full blur-2xl" />
            </div>

            <div className="relative mx-auto max-w-[1600px]">
              <div className="flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
                {/* Title section */}
                <div className="space-y-2">
                  <a
                    href={TIME_TRACKING_PROVIDER_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="group inline-flex items-center gap-2"
                  >
                    <h1 className="font-display text-3xl font-bold tracking-tight md:text-4xl">
                      <span className="text-gradient">Kleer</span>
                    </h1>
                    <ExternalLink className="h-4 w-4 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                  </a>
                  <p className="text-sm text-muted-foreground">
                    Track your time, stay productive
                  </p>
                </div>

                {/* Action buttons */}
                <div className="flex flex-wrap gap-3">
                  {timerState !== "running" && (
                    <Button
                      variant="outline"
                      disabled={isStartingTimer}
                      onClick={() =>
                        startTimer({
                          userNote: TRY_CMD_K_NEXT_TIME_NOTE,
                          ...buildRememberedTimerParams({
                            rememberLastProject,
                            lastProject,
                            lastActivity,
                          }),
                        })
                      }
                      className="group h-11 gap-2 rounded-xl border-border/50 bg-card/50 px-5 shadow-sm backdrop-blur-sm transition-all hover:border-primary/30 hover:bg-card hover:shadow-glow-sm"
                    >
                      <TimerIcon className="h-4 w-4 transition-transform group-hover:scale-110" />
                      <span>Start Timer</span>
                    </Button>
                  )}
                  <Button
                    onClick={() => setIsNewEntryOpen(true)}
                    onMouseEnter={prefetchProjects}
                    onFocus={prefetchProjects}
                    className="btn-glow h-11 gap-2 rounded-xl bg-primary px-5 font-semibold text-primary-foreground shadow-md transition-all hover:bg-primary/90 hover:shadow-glow"
                  >
                    <Plus className="h-4 w-4" />
                    <span>New Entry</span>
                  </Button>
                </div>
              </div>
            </div>
          </header>

          {/* Main Content */}
          <main className="mx-auto max-w-[1600px] px-6 py-8">
            <NotLockedAlert />

            {/* Controls Bar */}
            <div className="mb-8 flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
              <DateRangeSelector
                dateRange={dateRange}
                setDateRange={setDateRange}
              />
              <div className="flex items-center gap-1 rounded-xl border border-border/50 bg-card/40 p-1 backdrop-blur-sm">
                {/* View Toggle — hidden on mobile (always list) */}
                <div className="hidden md:flex">
                  <button
                    type="button"
                    onClick={() => setViewMode("list")}
                    className={cn(
                      "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all",
                      viewMode === "list"
                        ? "bg-primary text-primary-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                    )}
                  >
                    <List className="h-3.5 w-3.5" />
                    List
                  </button>
                  <button
                    type="button"
                    onClick={() => setViewMode("timeline")}
                    className={cn(
                      "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all",
                      viewMode === "timeline"
                        ? "bg-primary text-primary-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                    )}
                  >
                    <CalendarDays className="h-3.5 w-3.5" />
                    Timeline
                  </button>
                </div>
                {/* Divider + Merge (always rendered, fades when not list) */}
                <div
                  className={cn(
                    "flex items-center transition-opacity duration-200",
                    viewMode === "list"
                      ? "opacity-100"
                      : "pointer-events-none opacity-0",
                  )}
                >
                  <div className="mx-1 hidden h-5 w-px bg-border/50 md:block" />
                  <MergeEntriesSwitch
                    mergeSameDay={mergeSameDay}
                    setMergeSameDay={setMergeSameDay}
                  />
                </div>
                {/* Divider + Search */}
                <div className="mx-1 h-5 w-px bg-border/50" />
                <SearchBar search={search} setSearch={setSearch} />
                {/* Divider + Settings */}
                <div className="mx-1 h-5 w-px bg-border/50" />
                <TimeTrackingSettings
                  rememberLastProject={rememberLastProject}
                  setRememberLastProject={setRememberLastProject}
                  isAdmin={isAdmin}
                />
              </div>
            </div>

            {/* Content Grid */}
            <div className="grid grid-cols-1 gap-8 xl:grid-cols-[1fr_380px]">
              {/* Time Entries */}
              <div className="min-w-0">
                {isTimeEntriesLoading ? (
                  <div className="min-h-[400px] rounded-2xl border border-border/50 bg-card/30 p-8 text-sm text-muted-foreground">
                    Loading entries...
                  </div>
                ) : timeEntries?.length ? (
                  viewMode === "timeline" ? (
                    <TimelineView
                      timeEntries={filteredTimeEntries ?? []}
                      dateRange={dateRange}
                    />
                  ) : (
                    <TimeEntriesList
                      timeEntries={filteredTimeEntries ?? []}
                      mergeSameDay={mergeSameDay}
                    />
                  )
                ) : (
                  <EmptyState
                    timerState={timerState}
                    onStartTimer={onStartTimer}
                  />
                )}
              </div>

              {/* Sidebar */}
              <aside className="space-y-6">
                <TimeStats />
                {!!timeEntries?.length && <Summary timeEntries={timeEntries} />}
              </aside>
            </div>
          </main>
        </div>
      )}
      <NewEntryDialog
        open={isNewEntryOpen}
        onOpenChange={setIsNewEntryOpen}
        onCreate={(payload) => {
          createProjectRegistration(payload);
        }}
      />
    </div>
  );
}

function EmptyState(props: {
  timerState: string | undefined;
  onStartTimer: () => void;
}) {
  return (
    <div className="flex min-h-[400px] flex-col items-center justify-center rounded-2xl border border-dashed border-border/50 bg-card/30 p-12 text-center">
      <div className="mb-6 flex h-16 w-16 items-center justify-center rounded-2xl bg-primary/10">
        <Clock className="h-8 w-8 text-primary" />
      </div>
      <h3 className="mb-2 font-display text-xl font-semibold">
        No time entries yet
      </h3>
      <p className="mb-6 max-w-sm text-sm text-muted-foreground">
        {props.timerState === "running"
          ? "Your timer is running. Save it when you're ready."
          : "Start tracking your time to see your entries here."}
      </p>
      {props.timerState !== "running" && (
        <Button
          onClick={props.onStartTimer}
          className="btn-glow gap-2 rounded-xl"
        >
          <Sparkles className="h-4 w-4" />
          Start Your First Timer
        </Button>
      )}
    </div>
  );
}

function NotConnectedState({
  isAdmin,
  rememberLastProject,
  setRememberLastProject,
}: {
  isAdmin: boolean;
  rememberLastProject: boolean;
  setRememberLastProject: (value: boolean) => void;
}) {
  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-xl animate-fade-in text-center">
        <div className="mb-5 inline-flex h-16 w-16 items-center justify-center rounded-2xl bg-primary text-primary-foreground shadow-glow">
          <Clock className="h-8 w-8" />
        </div>
        <h1 className="font-display text-4xl font-bold tracking-tight">
          Kleer
        </h1>
        <p className="mx-auto mt-3 max-w-md text-sm leading-relaxed text-muted-foreground">
          Your Toki account is not connected to a Kleer user yet. Contact an
          admin to enable time tracking for your account.
        </p>

        {isAdmin && (
          <div className="mt-6 inline-flex items-center gap-2 rounded-xl border border-border/60 bg-card/70 p-2 shadow-sm">
            <span className="pl-2 text-xs text-muted-foreground">
              Admin mapping
            </span>
            <TimeTrackingSettings
              rememberLastProject={rememberLastProject}
              setRememberLastProject={setRememberLastProject}
              isAdmin={isAdmin}
            />
          </div>
        )}
      </div>
    </div>
  );
}
