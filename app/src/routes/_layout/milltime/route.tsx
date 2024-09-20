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
import React from "react";
import { AddEntryButton } from "@/routes/_layout/milltime/-components/add-entry-button";
import { DataVisualization } from "@/routes/_layout/milltime/-components/data-visualization";
import { SearchBar } from "@/routes/_layout/milltime/-components/search-bar";
import { Summary } from "@/routes/_layout/milltime/-components/summary";
import { TimeEntriesList } from "./-components/time-entries-list";
import { DateRangeSelector } from "./-components/date-range-selector";
import { useQuery } from "@tanstack/react-query";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import dayjs from "dayjs";

export const Route = createFileRoute("/_layout/milltime")({
  component: MilltimeComponent,
});

function MilltimeComponent() {
  const { authenticate } = useMilltimeActions();
  const isAuthenticating = useMilltimeIsAuthenticating();

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const [dateRange, setDateRange] = React.useState({
    start: new Date(),
    end: new Date(),
  });

  const { isAuthenticated } = useMilltimeData();

  const { data: timeEntries } = useQuery({
    ...milltimeQueries.timeEntries({
      // fucking americans...
      from: dayjs().startOf("week").add(1, "day").format("YYYY-MM-DD"),
      to: dayjs().endOf("week").add(1, "day").format("YYYY-MM-DD"),
    }),
  });

  return (
    <div>
      {!isAuthenticated && (
        <form
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
      )}
      <div className={`min-h-screen`}>
        <div className="mx-auto w-[95%] max-w-[100rem] px-4 py-8">
          <header className="mb-8 flex items-center justify-between">
            <h1 className="text-3xl font-bold">Milltime</h1>
          </header>
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-3">
            <div className="lg:col-span-2">
              <div className="mt-4 flex items-center justify-between">
                <DateRangeSelector onRangeChange={setDateRange} />
                <SearchBar />
              </div>
              <TimeEntriesList timeEntries={timeEntries ?? []} />
              <AddEntryButton />
            </div>
            <div className="flex flex-col gap-4">
              <Summary timeEntries={timeEntries ?? []} />
              <DataVisualization timeEntries={timeEntries ?? []} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
