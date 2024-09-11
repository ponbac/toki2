import { HistoryIcon, PlayCircleIcon } from "lucide-react";
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
import { getWeekNumber } from "@/lib/utils";
import { toast } from "sonner";
import { useQuery } from "@tanstack/react-query";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { ScrollArea } from "./ui/scroll-area";

export const MilltimeTimerDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const [projectId, setProjectId] = React.useState("");
  const [activityName, setActivityName] = React.useState("");
  const [note, setNote] = React.useState("");

  const { projects, activities } = useMilltimeData({
    projectId: projectId,
  });

  const selectedProject = projects?.find(
    (p) => p.projectId.toString() === projectId,
  );
  const selectedActivity = activities?.find(
    (a) => a.activityName === activityName,
  );

  const resetForm = () => {
    setProjectId("");
    setActivityName("");
    setNote("");
  };

  const { mutate: startTimer } = milltimeMutations.useStartTimer({
    onSuccess: () => {
      toast.success("Timer started!");
      props.onOpenChange(false);
      resetForm();
    },
  });

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        props.onOpenChange(open);
        resetForm();
      }}
    >
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            const project = projects?.find(
              (p) => p.projectId.toString() === projectId,
            );
            const activity = activities?.find(
              (a) => a.activityName === activityName,
            );
            if (!project || !activity) {
              return;
            }

            startTimer({
              activity: activity.activity,
              activityName: activity.activityName,
              projectId: project.projectId,
              projectName: project.projectName,
              userNote: note,
              regDay: dayjs().format("YYYY-MM-DD"),
              weekNumber: getWeekNumber(new Date()),
            });
          }}
        >
          <DialogHeader>
            <DialogTitle className="flex flex-row items-center gap-2">
              Start Milltime Timer
            </DialogTitle>
            <DialogDescription>
              Select the project and activity you want to track time for.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Select
                value={selectedProject?.projectId.toString() ?? ""}
                onValueChange={(v) => setProjectId(v)}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Project" />
                </SelectTrigger>
                <SelectContent>
                  {projects?.map((project) => (
                    <SelectItem
                      key={project.projectId}
                      value={project.projectId.toString()}
                    >
                      {project.projectName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                key={activities?.length}
                value={selectedActivity?.activity ?? ""}
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
                      value={activity.activity}
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
                setProjectId(
                  projects
                    ?.find((p) => p.projectName === projectName)
                    ?.projectId.toString() ?? "",
                );
                setActivityName(activityName);
                setNote(note);
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
              <PlayCircleIcon className="size-5" />
              Start
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
  const { data: timeEntries } = useQuery({
    ...milltimeQueries.timeEntries({
      from: dayjs().startOf("month").format("YYYY-MM-DD"),
      to: dayjs().endOf("month").format("YYYY-MM-DD"),
    }),
  });

  if (!timeEntries?.length) {
    return null;
  }

  return (
    <div className="mt-4">
      <div className="mb-2 flex flex-row items-center gap-2">
        <HistoryIcon className="size-4" />
        <h2 className="text-sm font-semibold">Recent Timers</h2>
      </div>
      <ScrollArea className="flex max-h-72 w-full flex-col gap-2">
        {timeEntries?.map((timeEntry, index) => (
          <button
            className="group flex w-full cursor-pointer flex-col rounded-md py-1"
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
              <div className="mt-1 truncate text-sm text-muted-foreground transition-colors group-hover:text-primary/80">
                {timeEntry.note}
              </div>
            )}
          </button>
        ))}
      </ScrollArea>
    </div>
  );
}
