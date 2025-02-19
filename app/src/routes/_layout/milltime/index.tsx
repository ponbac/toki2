import { createFileRoute } from "@tanstack/react-router";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
import { useAtom } from "jotai/react";
import { MergeEntriesSwitch } from "./-components/merge-entries-switch";
import { TimeStats } from "./-components/time-stats";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import {
  useMilltimeIsAuthenticating,
  useMilltimeTimer,
} from "@/hooks/useMilltimeStore";
import { useMilltimeActions } from "@/hooks/useMilltimeStore";
import { NotLockedAlert } from "./-components/not-locked-alert";
import { TimerIcon } from "lucide-react";

export const Route = createFileRoute("/_layout/milltime/")({
  loader: async ({ context }) => {
    try {
      await context.queryClient.ensureQueryData(
        milltimeQueries.timeEntries({
          from: format(
            startOfWeek(new Date(), { weekStartsOn: 1 }),
            "yyyy-MM-dd",
          ),
          to: format(endOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
        }),
      );
    } catch (error) {
      console.error(error);
    }
  },
  component: MilltimeComponent,
});

const mergeSameDayPersistedAtom = atomWithStorage(
  "milltime-mergeSameDay",
  false,
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
            .includes(search.toLowerCase()),
        )
      : [];
  }, [timeEntries, search]);

  const { state: timerState } = useMilltimeTimer();
  const { mutate: startStandaloneTimer, isPending: isStartingStandaloneTimer } =
    milltimeMutations.useStartStandaloneTimer();

  return (
    <div>
      {!isAuthenticated ? (
        <LoginForm
          onSubmit={authenticate}
          isAuthenticating={isAuthenticating}
        />
      ) : (
        <div className={`min-h-screen`}>
          <div className="mx-auto w-[95%] max-w-[100rem] px-4 py-8">
            <header className="mb-8 flex flex-col gap-4 md:h-12 md:flex-row md:items-center md:justify-between">
              <h1 className="text-2xl font-bold md:text-3xl">Milltime</h1>
              {timerState !== "running" && (
                <Button
                  variant="outline"
                  disabled={isStartingStandaloneTimer}
                  onClick={() =>
                    startStandaloneTimer({
                      userNote: "Try Ctrl+K to start a timer next time",
                    })
                  }
                  className="w-full md:w-auto"
                >
                  <TimerIcon className="mr-2 h-4 w-4" />
                  Start New Timer
                </Button>
              )}
            </header>
            <NotLockedAlert />
            <div className="grid grid-cols-1 gap-8 lg:grid-cols-3">
              <div className="lg:col-span-2">
                <div className="mt-4 flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
                  <DateRangeSelector
                    dateRange={dateRange}
                    setDateRange={setDateRange}
                  />
                  <div className="flex flex-col gap-4 md:flex-row md:items-center">
                    <MergeEntriesSwitch
                      mergeSameDay={mergeSameDay}
                      setMergeSameDay={setMergeSameDay}
                    />
                    <SearchBar search={search} setSearch={setSearch} />
                  </div>
                </div>
                {timeEntries?.length ? (
                  <TimeEntriesList
                    timeEntries={filteredTimeEntries ?? []}
                    mergeSameDay={mergeSameDay}
                  />
                ) : (
                  <div className="flex h-full flex-col items-center justify-center">
                    <p className="text-xl font-semibold">
                      No time entries found
                    </p>
                    <p className="text-center text-sm text-muted-foreground">
                      Try changing the filters or{" "}
                      {timerState === "running" ? (
                        <span className="">saving your current timer</span>
                      ) : (
                        <span
                          className="underline transition-colors hover:cursor-pointer hover:text-primary"
                          onClick={() =>
                            startStandaloneTimer({
                              userNote: "First timer of the week...",
                            })
                          }
                        >
                          starting a new timer
                        </span>
                      )}
                      .
                    </p>
                  </div>
                )}
              </div>
              <div className="flex flex-col gap-4">
                <TimeStats />
                {!!timeEntries?.length && <Summary timeEntries={timeEntries} />}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function LoginForm(props: {
  onSubmit: (credentials: { username: string; password: string }) => void;
  isAuthenticating: boolean;
}) {
  return (
    <form
      className="flex min-h-screen items-center justify-center"
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
      <Card className="mx-auto max-w-sm">
        <CardHeader>
          <CardTitle className="text-xl">Authenticate</CardTitle>
          <CardDescription>
            Allow Toki to access your Milltime account.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="username">Username</Label>
              <Input
                id="username"
                name="username"
                type="text"
                placeholder="pbac"
                required
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="password">Password</Label>
              <Input id="password" name="password" type="password" />
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={props.isAuthenticating}
            >
              Authenticate
            </Button>
          </div>
        </CardContent>
      </Card>
    </form>
  );
}
