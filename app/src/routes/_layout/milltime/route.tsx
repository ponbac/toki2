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
import { ExportButton } from "@/routes/_layout/milltime/-components/export-button";
import { SearchBar } from "@/routes/_layout/milltime/-components/search-bar";
import { Summary } from "@/routes/_layout/milltime/-components/summary";
import { TimeEntriesList } from "./-components/time-entries-list";
import { DateRangeSelector } from "./-components/date-range-selector";

export const Route = createFileRoute("/_layout/milltime")({
  component: MilltimeComponent,
});

function MilltimeComponent() {
  const { authenticate, setNewTimerDialogOpen } = useMilltimeActions();
  const isAuthenticating = useMilltimeIsAuthenticating();

  const [activeProjectId, setActiveProjectId] = React.useState<string>();
  const [dateRange, setDateRange] = React.useState({
    start: new Date(),
    end: new Date(),
  });

  const { projects, activities, isAuthenticated } = useMilltimeData({
    projectId: activeProjectId,
  });

  return (
    <div>
      <h1>Milltime</h1>
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
        <div className="">
          <div className="container mx-auto px-4 py-8">
            <header className="mb-8 flex items-center justify-between">
              <h1 className="text-3xl font-bold">Time Tracker</h1>
            </header>
            <div className="grid grid-cols-1 gap-8 lg:grid-cols-3">
              <div className="lg:col-span-2">
                <DateRangeSelector onRangeChange={setDateRange} />
                <div className="mt-4 flex items-center justify-between">
                  <SearchBar />
                  <ExportButton />
                </div>
                <TimeEntriesList dateRange={dateRange} />
                <AddEntryButton />
              </div>
              <div>
                <Summary dateRange={dateRange} />
                <DataVisualization dateRange={dateRange} />
              </div>
            </div>
          </div>
        </div>
      </div>
      <Button onClick={() => setNewTimerDialogOpen(true)}>New Timer</Button>
    </div>
  );
}
