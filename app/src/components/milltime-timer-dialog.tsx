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
  const [activityId, setActivityId] = React.useState("");
  const [note, setNote] = React.useState("");

  const { projects, activities } = useMilltimeData({
    projectId: projectId,
  });

  const selectedProject = projects?.find(
    (p) => p.projectId.toString() === projectId,
  );
  const selectedActivity = activities?.find((a) => a.activity === activityId);

  const resetForm = () => {
    setProjectId("");
    setActivityId("");
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
            const activity = activities?.find((a) => a.activity === activityId);
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
                onValueChange={(v) => setActivityId(v)}
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
              onHistoryClick={(projectId, activityId, note) => {
                setProjectId(projectId);
                setActivityId(activityId);
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
              disabled={!projectId || !activityId}
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
  onHistoryClick: (projectId: string, activityId: string, note: string) => void;
}) {
  // TODO: this should be done in the backend
  const { data: timerHistory } = useQuery({
    ...milltimeQueries.timerHistory(),
    select: (data) =>
      data
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
        .filter(
          (timer, index, self) =>
            index ===
            self.findIndex(
              (t) =>
                t.projectId === timer.projectId &&
                t.activityId === timer.activityId &&
                t.note === timer.note,
            ),
        ),
  });

  if (!timerHistory?.length) {
    return null;
  }

  return (
    <div className="mt-4">
      <div className="mb-2 flex flex-row items-center gap-2">
        <HistoryIcon className="size-4" />
        <h2 className="text-sm font-semibold">Recent Timers</h2>
      </div>
      <ScrollArea className="flex max-h-72 flex-col gap-2">
        {timerHistory?.map((timer, index) => (
          <button
            className="group flex w-full cursor-pointer flex-col rounded-md py-1"
            key={index}
            onClick={() =>
              props.onHistoryClick(
                timer.projectId,
                timer.activityId,
                timer.note,
              )
            }
          >
            <div className="flex w-full items-center justify-between">
              <span className="text-sm font-medium transition-colors group-hover:text-primary">
                {timer.projectName}
              </span>
              <span className="text-xs text-muted-foreground transition-colors group-hover:text-primary/80">
                {timer.activityName}
              </span>
            </div>
            {timer.note && (
              <div className="mt-1 truncate text-sm text-muted-foreground transition-colors group-hover:text-primary/80">
                {timer.note}
              </div>
            )}
          </button>
        ))}
      </ScrollArea>
    </div>
  );
}
