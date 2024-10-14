import { milltimeQueries } from "@/lib/api/queries/milltime";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import { HistoryIcon, SearchCode } from "lucide-react";
import React from "react";
import { Input } from "./ui/input";
import { Skeleton } from "./ui/skeleton";
import { ScrollArea } from "./ui/scroll-area";

export function TimerHistory(props: {
  onHistoryClick: (
    projectName: string,
    activityName: string,
    note: string,
  ) => void;
}) {
  const [searchTerm, setSearchTerm] = React.useState("");
  const inputRef = React.useRef<HTMLInputElement>(null);

  const { data: timeEntries, isLoading } = useQuery({
    ...milltimeQueries.timeEntries({
      from: dayjs().subtract(1, "month").format("YYYY-MM-DD"),
      to: dayjs().add(1, "day").format("YYYY-MM-DD"),
      unique: true,
    }),
  });

  const filteredEntries = React.useMemo(() => {
    if (!timeEntries?.length) return [];
    return timeEntries.filter((entry) =>
      [entry.projectName, entry.activityName, entry.note]
        .join(" ")
        .toLowerCase()
        .includes(searchTerm.toLowerCase()),
    );
  }, [timeEntries, searchTerm]);

  return (
    <div className="mt-4">
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
            className="h-9 pl-8 text-sm"
          />
        </div>
      </div>
      <ScrollArea className="flex max-h-72 w-full flex-col gap-2">
        {isLoading
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
                  props.onHistoryClick(
                    timeEntry.projectName,
                    timeEntry.activityName,
                    timeEntry.note ?? "",
                  )
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
                  <div className="mt-1 max-w-[55ch] truncate text-sm text-muted-foreground transition-colors group-hover:text-primary/80">
                    {timeEntry.note}
                  </div>
                )}
              </button>
            ))}
      </ScrollArea>
    </div>
  );
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
