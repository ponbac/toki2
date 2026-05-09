import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { getCachedTimeEntries } from "@/lib/api/time-tracking-cache";
import { cn } from "@/lib/utils";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import dayjs from "dayjs";
import { HistoryIcon, SearchCode } from "lucide-react";
import React from "react";
import { Input } from "./ui/input";
import { Skeleton } from "./ui/skeleton";
import { ScrollArea } from "./ui/scroll-area";

export function TimerHistory(props: {
  onHistoryClick: (timeEntry: {
    projectId: string;
    projectName: string;
    activityId: string;
    activityName: string;
    note: string;
  }) => void;
  className?: string;
  searchInputClassName?: string;
  scrollAreaClassName?: string;
}) {
  const [searchTerm, setSearchTerm] = React.useState("");
  const inputRef = React.useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();

  const { data: timeEntries, isLoading } = useQuery({
    ...timeTrackingQueries.timeEntries({
      from: dayjs().subtract(1, "month").format("YYYY-MM-DD"),
      to: dayjs().add(1, "day").format("YYYY-MM-DD"),
      unique: true,
    }),
  });

  const cachedSuggestions = React.useMemo(() => {
    return dedupeSuggestions(getCachedTimeEntries(queryClient));
  }, [queryClient]);

  const suggestions = React.useMemo(
    () => dedupeSuggestions([...(timeEntries ?? []), ...cachedSuggestions]),
    [cachedSuggestions, timeEntries],
  );

  const filteredEntries = React.useMemo(() => {
    if (!suggestions.length) return [];
    return suggestions.filter((entry) =>
      [entry.projectName, entry.activityName, entry.note]
        .join(" ")
        .toLowerCase()
        .includes(searchTerm.toLowerCase()),
    );
  }, [suggestions, searchTerm]);

  return (
    <div className={props.className}>
      <div className="mb-2 flex flex-row items-center justify-between">
        <div className="flex flex-row items-center gap-1">
          <HistoryIcon className="size-5" />
          <h2 className="text-sm font-semibold">
            Recent entries{" "}
            <span className="text-sm text-muted-foreground">
              (last 30 days)
            </span>
          </h2>
        </div>
        <div className="relative flex w-48 items-center">
          <SearchCode
            onClick={() => inputRef.current?.focus()}
            className="absolute left-2 top-1/2 size-4 -translate-y-1/2 transform cursor-pointer"
          />
          <Input
            autoFocus
            ref={inputRef}
            placeholder="Search entries..."
            value={searchTerm ?? ""}
            onChange={(event) => {
              const value = event.target.value;
              setSearchTerm(value);
            }}
            className={cn("h-9 pl-8 text-sm", props.searchInputClassName)}
          />
        </div>
      </div>
      <ScrollArea
        className={cn(
          "flex max-h-72 w-full flex-col gap-2",
          props.scrollAreaClassName,
        )}
      >
        {isLoading && !filteredEntries.length
          ? Array.from({ length: 10 }).map((_, index) => (
              <HistoryEntrySkeleton key={index} />
            ))
          : filteredEntries.map((timeEntry, index) => (
              <button
                type="button"
                className={cn(
                  "group flex w-full cursor-pointer flex-col rounded-md py-1",
                  "transition-colors focus:bg-accent/50 focus:text-primary focus:outline-none",
                )}
                key={index}
                onClick={() =>
                  props.onHistoryClick({
                    projectId: timeEntry.projectId,
                    projectName: timeEntry.projectName,
                    activityId: timeEntry.activityId,
                    activityName: timeEntry.activityName,
                    note: timeEntry.note ?? "",
                  })
                }
              >
                <div className="flex w-full items-center justify-between">
                  <span className="text-sm font-medium transition-colors group-hover:text-primary">
                    {timeEntry.projectName}
                  </span>
                  <span className="text-xs text-muted-foreground transition-colors group-hover:text-primary/80">
                    {timeEntry.activityName}
                  </span>
                </div>
                {timeEntry.note && (
                  <div className="mt-1 max-w-[55ch] truncate text-left text-sm text-muted-foreground transition-colors group-hover:text-primary/80">
                    {timeEntry.note}
                  </div>
                )}
              </button>
            ))}
      </ScrollArea>
    </div>
  );
}

function dedupeSuggestions<T extends {
  projectName: string;
  activityName: string;
  note: string | null;
}>(entries: Array<T>) {
  const seen = new Set<string>();
  return entries.filter((entry) => {
    const key = `${entry.projectName}\u0000${entry.activityName}\u0000${entry.note ?? ""}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function HistoryEntrySkeleton() {
  return (
    <div className="group flex w-full cursor-pointer flex-col rounded-md py-1">
      <div className="flex w-full items-center justify-between">
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-3 w-20" />
      </div>
      <Skeleton className="mt-1 h-3 w-60" />
    </div>
  );
}
