import { EditIcon, HistoryIcon, SearchCode } from "lucide-react";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import React from "react";
import { useMilltimeData } from "@/hooks/useMilltimeData";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./ui/select";
import { Input } from "./ui/input";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import dayjs from "dayjs";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import { DatabaseTimer, milltimeQueries } from "@/lib/api/queries/milltime";
import { ScrollArea } from "./ui/scroll-area";
import { Skeleton } from "./ui/skeleton";

export const TimerEditDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  timer: DatabaseTimer;
}) => {
  const [projectId, setProjectId] = React.useState(props.timer.projectId);
  const [activityName, setActivityName] = React.useState(
    props.timer.activityName,
  );
  const [note, setNote] = React.useState(props.timer.note);

  const { projects, activities } = useMilltimeData({
    projectId: projectId,
    enabled: props.open,
  });

  const selectedProject = React.useMemo(
    () => projects?.find((p) => p.projectId === projectId),
    [projects, projectId],
  );
  const selectedActivity = React.useMemo(
    () => activities?.find((a) => a.activityName === activityName),
    [activities, activityName],
  );

  const { mutate: updateTimerMutate } =
    milltimeMutations.useEditStandaloneTimer({
      onSuccess: () => {
        props.onOpenChange(false);
      },
    });

  const updateTimer = () => {
    updateTimerMutate(
      {
        projectId: projectId,
        projectName: selectedProject?.projectName ?? "",
        activityId: selectedActivity?.activity ?? "",
        activityName: activityName,
        userNote: note,
      },
      {
        onSuccess: () => {
          props.onOpenChange(false);
        },
      },
    );
  };

  React.useEffect(() => {
    setProjectId(props.timer.projectId);
    setActivityName(props.timer.activityName);
    setNote(props.timer.note);
  }, [props.timer]);

  // TODO: should skeleton while loading...
  if (!props.open || !projects) return null;

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        props.onOpenChange(open);
      }}
    >
      <DialogContent className="w-full max-w-2xl">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            updateTimer();
          }}
        >
          <DialogHeader>
            <DialogTitle className="flex flex-row items-center gap-2">
              Edit Timer
            </DialogTitle>
            <DialogDescription>
              Select the project and activity you want to track time for.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Select
                value={selectedProject?.projectId ?? ""}
                onValueChange={(v) => setProjectId(v)}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Project" />
                </SelectTrigger>
                <SelectContent>
                  {projects?.map((project) => (
                    <SelectItem
                      key={project.projectId}
                      value={project.projectId}
                    >
                      {project.projectName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                key={activities?.length}
                value={selectedActivity?.activityName ?? ""}
                onValueChange={(v) => setActivityName(v)}
                disabled={!projectId}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Activity" />
                </SelectTrigger>
                <SelectContent>
                  {activities?.map((activity) => (
                    <SelectItem
                      key={activity.activity}
                      value={activity.activityName}
                    >
                      {activity.activityName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Input
                placeholder="Note"
                value={note}
                onChange={(e) => setNote(e.target.value)}
              />
            </div>
            <TimerHistory
              onHistoryClick={(projectName, activityName, note) => {
                // already selected? start timer
                if (
                  projectName === selectedProject?.projectName &&
                  activityName === selectedActivity?.activityName &&
                  note === note
                ) {
                  updateTimer();
                } else {
                  setProjectId(
                    projects
                      ?.find((p) => p.projectName === projectName)
                      ?.projectId.toString() ?? "",
                  );
                  setActivityName(activityName);
                  setNote(note);
                }
              }}
            />
          </div>
          <DialogFooter>
            <Button
              type="submit"
              variant="default"
              size="sm"
              className="flex gap-2"
              disabled={!projectId || !activityName}
            >
              <EditIcon className="size-5" />
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

function TimerHistory(props: {
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
      from: dayjs().subtract(14, "days").format("YYYY-MM-DD"),
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
        <div className="flex flex-row items-center gap-2">
          <HistoryIcon className="size-4" />
          <h2 className="text-sm font-semibold">Recent entries</h2>
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
