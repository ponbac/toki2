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
import {
  useMilltimeActions,
  useMilltimeIsAuthenticating,
} from "@/hooks/useMilltimeContext";
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

export const Route = createFileRoute("/_layout/milltime")({
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
      // fucking americans...
      from: dateRange.from,
      to: dateRange.to,
    }),
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

  return (
    <div>
      {!isAuthenticated ? (
        <form
          className="flex min-h-screen items-center justify-center"
          onSubmit={(e) => {
            e.preventDefault();
            const formData = new FormData(e.target as HTMLFormElement);
            const username = formData.get("username") as string;
            const password = formData.get("password") as string;

            authenticate({
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
                  disabled={isAuthenticating}
                >
                  Authenticate
                </Button>
              </div>
            </CardContent>
          </Card>
        </form>
      ) : (
        <div className={`min-h-screen`}>
          <div className="mx-auto w-[95%] max-w-[100rem] px-4 py-8">
            <header className="mb-8 flex items-center justify-between">
              <h1 className="text-3xl font-bold">Milltime</h1>
            </header>
            <div className="grid grid-cols-1 gap-8 lg:grid-cols-3">
              <div className="lg:col-span-2">
                <div className="mt-4 flex items-center justify-between">
                  <DateRangeSelector
                    dateRange={dateRange}
                    setDateRange={setDateRange}
                  />
                  <div className="flex flex-row items-center gap-4">
                    <MergeEntriesSwitch
                      mergeSameDay={mergeSameDay}
                      setMergeSameDay={setMergeSameDay}
                    />
                    <SearchBar search={search} setSearch={setSearch} />
                  </div>
                </div>
                <TimeEntriesList
                  timeEntries={filteredTimeEntries ?? []}
                  mergeSameDay={mergeSameDay}
                />
              </div>
              <div className="flex flex-col gap-4">
                <Summary timeEntries={timeEntries ?? []} />
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
