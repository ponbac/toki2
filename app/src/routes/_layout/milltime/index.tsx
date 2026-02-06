import { createFileRoute } from "@tanstack/react-router";
import { cn } from "@/lib/utils";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useMilltimeData } from "@/hooks/useMilltimeData";
import { SearchBar } from "@/routes/_layout/milltime/-components/search-bar";
import { Summary } from "@/routes/_layout/milltime/-components/summary";
import { TimeEntriesList } from "./-components/time-entries-list";
import { DateRangeSelector } from "./-components/date-range-selector";
import { useQuery } from "@tanstack/react-query";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { startOfWeek, endOfWeek, format } from "date-fns";
import React from "react";
import { atomWithStorage } from "jotai/utils";
import { useAtom, useAtomValue } from "jotai/react";
import { MergeEntriesSwitch } from "./-components/merge-entries-switch";
import { MilltimeSettings } from "./-components/milltime-settings";
import {
  buildRememberedTimerParams,
  lastActivityAtom,
  lastProjectAtom,
  rememberLastProjectAtom,
} from "@/lib/milltime-preferences";
import { TimeStats } from "./-components/time-stats";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import {
  useMilltimeIsAuthenticating,
  useMilltimeTimer,
} from "@/hooks/useMilltimeStore";
import { useMilltimeActions } from "@/hooks/useMilltimeStore";
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
import { TimelineView } from "./-components/timeline-view";

export const Route = createFileRoute("/_layout/milltime/")({
  loader: async ({ context }) => {
    try {
      await context.queryClient.ensureQueryData(
        milltimeQueries.timeEntries({
          from: format(
            startOfWeek(new Date(), { weekStartsOn: 1 }),
            "yyyy-MM-dd"
          ),
          to: format(endOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
        })
      );
    } catch (error) {
      console.error(error);
    }
  },
  component: MilltimeComponent,
});

const mergeSameDayPersistedAtom = atomWithStorage(
  "milltime-mergeSameDay",
  false
);

const viewModePersistedAtom = atomWithStorage<"list" | "timeline">(
  "milltime-viewMode",
  "list"
);

function MilltimeComponent() {
  const { authenticate } = useMilltimeActions();
  const isAuthenticating = useMilltimeIsAuthenticating();
  const { isAuthenticated } = useMilltimeData();

  const [dateRange, setDateRange] = React.useState({
    from: format(startOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
    to: format(endOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
  });
  const [mergeSameDay, setMergeSameDay] = useAtom(mergeSameDayPersistedAtom);
  const [viewMode, setViewMode] = useAtom(viewModePersistedAtom);
  const [rememberLastProject, setRememberLastProject] = useAtom(
    rememberLastProjectAtom
  );
  const lastProject = useAtomValue(lastProjectAtom);
  const lastActivity = useAtomValue(lastActivityAtom);
  const [search, setSearch] = React.useState("");

  const { data: timeEntries } = useQuery({
    ...milltimeQueries.timeEntries({
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
            .includes(search.toLowerCase())
        )
      : [];
  }, [timeEntries, search]);

  const { state: timerState } = useMilltimeTimer();
  const { mutate: startStandaloneTimer, isPending: isStartingStandaloneTimer } =
    milltimeMutations.useStartStandaloneTimer();
  const { mutate: createProjectRegistration } =
    milltimeMutations.useCreateProjectRegistration({
      onSuccess: () => {
        setIsNewEntryOpen(false);
        toast.success("Entry created");
      },
      onError: () => toast.error("Failed to create entry"),
    });

  const [isNewEntryOpen, setIsNewEntryOpen] = React.useState(false);

  return (
    <div className="min-h-screen">
      {!isAuthenticated ? (
        <LoginForm
          onSubmit={authenticate}
          isAuthenticating={isAuthenticating}
        />
      ) : (
        <div className="animate-fade-in">
          {/* Hero Header */}
          <header className="relative overflow-hidden border-b border-border/50 bg-gradient-to-b from-card/80 to-background px-6 pb-6 pt-8">
            {/* Background decoration */}
            <div className="pointer-events-none absolute inset-0 overflow-hidden">
              <div className="absolute -right-20 -top-20 h-64 w-64 rounded-full bg-primary/5 blur-3xl" />
              <div className="absolute -left-20 top-20 h-48 w-48 rounded-full bg-primary/3 blur-2xl" />
            </div>

            <div className="relative mx-auto max-w-[1600px]">
              <div className="flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
                {/* Title section */}
                <div className="space-y-2">
                  <a
                    href={import.meta.env.VITE_MILLTIME_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="group inline-flex items-center gap-2"
                  >
                    <h1 className="font-display text-3xl font-bold tracking-tight md:text-4xl">
                      <span className="text-gradient">Milltime</span>
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
                      disabled={isStartingStandaloneTimer}
                      onClick={() =>
                        startStandaloneTimer({
                          userNote: "Try Ctrl+K to start a timer next time",
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
                {/* View Toggle */}
                <div className="flex">
                  <button
                    onClick={() => setViewMode("list")}
                    className={cn(
                      "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all",
                      viewMode === "list"
                        ? "bg-primary text-primary-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
                    )}
                  >
                    <List className="h-3.5 w-3.5" />
                    List
                  </button>
                  <button
                    onClick={() => setViewMode("timeline")}
                    className={cn(
                      "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all",
                      viewMode === "timeline"
                        ? "bg-primary text-primary-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
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
                      : "pointer-events-none opacity-0"
                  )}
                >
                  <div className="mx-1 h-5 w-px bg-border/50" />
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
                <MilltimeSettings
                  rememberLastProject={rememberLastProject}
                  setRememberLastProject={setRememberLastProject}
                />
              </div>
            </div>

            {/* Content Grid */}
            <div className="grid grid-cols-1 gap-8 xl:grid-cols-[1fr_380px]">
              {/* Time Entries */}
              <div className="min-w-0">
                {timeEntries?.length ? (
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
                    onStartTimer={() =>
                      startStandaloneTimer({
                        userNote: "First timer of the week...",
                      })
                    }
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

function LoginForm(props: {
  onSubmit: (credentials: { username: string; password: string }) => void;
  isAuthenticating: boolean;
}) {
  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      {/* Background decorations */}
      <div className="pointer-events-none fixed inset-0 overflow-hidden">
        <div className="absolute left-1/4 top-1/4 h-96 w-96 rounded-full bg-primary/10 blur-3xl" />
        <div className="absolute bottom-1/4 right-1/4 h-64 w-64 rounded-full bg-primary/5 blur-2xl" />
      </div>

      <form
        className="relative w-full max-w-md"
        onSubmit={(e) => {
          e.preventDefault();
          const formData = new FormData(e.target as HTMLFormElement);
          const username = formData.get("username") as string;
          const password = formData.get("password") as string;

          props.onSubmit({
            username,
            password,
          });
        }}
      >
        <Card className="card-elevated overflow-hidden rounded-2xl border-border/50">
          {/* Header with gradient */}
          <div className="relative bg-gradient-to-br from-primary/10 via-primary/5 to-transparent px-8 pb-6 pt-8">
            <div className="absolute inset-0 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNDAiIGhlaWdodD0iNDAiIHZpZXdCb3g9IjAgMCA0MCA0MCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48ZyBmaWxsPSJub25lIiBmaWxsLXJ1bGU9ImV2ZW5vZGQiPjxjaXJjbGUgZmlsbD0iY3VycmVudENvbG9yIiBjeD0iMiIgY3k9IjIiIHI9IjEiIG9wYWNpdHk9IjAuMSIvPjwvZz48L3N2Zz4=')] opacity-50" />
            <div className="relative">
              <div className="mb-4 inline-flex h-12 w-12 items-center justify-center rounded-xl bg-primary/20">
                <TimerIcon className="h-6 w-6 text-primary" />
              </div>
              <h2 className="font-display text-2xl font-bold tracking-tight">
                Connect to Milltime
              </h2>
              <p className="mt-1 text-sm text-muted-foreground">
                Enter your credentials to sync your time entries
              </p>
            </div>
          </div>

          <CardContent className="p-8">
            <div className="space-y-5">
              <div className="space-y-2">
                <Label htmlFor="username" className="text-sm font-medium">
                  Username
                </Label>
                <Input
                  id="username"
                  name="username"
                  type="text"
                  placeholder="Enter your username"
                  required
                  className="h-11 rounded-xl border-border/50 bg-muted/30 transition-all focus:border-primary/50 focus:bg-background"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="password" className="text-sm font-medium">
                  Password
                </Label>
                <Input
                  id="password"
                  name="password"
                  type="password"
                  placeholder="Enter your password"
                  className="h-11 rounded-xl border-border/50 bg-muted/30 transition-all focus:border-primary/50 focus:bg-background"
                />
              </div>
              <Button
                type="submit"
                className="btn-glow h-11 w-full rounded-xl font-semibold"
                disabled={props.isAuthenticating}
              >
                {props.isAuthenticating ? (
                  <span className="flex items-center gap-2">
                    <span className="h-4 w-4 animate-spin rounded-full border-2 border-primary-foreground/30 border-t-primary-foreground" />
                    Authenticating...
                  </span>
                ) : (
                  "Authenticate"
                )}
              </Button>
            </div>
          </CardContent>
        </Card>
      </form>
    </div>
  );
}
