import { PlayCircleIcon } from "lucide-react";
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
                  <SelectItem key={activity.activity} value={activity.activity}>
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
  const { data: timerHistory } = useQuery(milltimeQueries.timerHistory());

  if (!timerHistory?.length) {
    return null;
  }

  return (
    <div>
      <h2 className="text-base font-semibold">History</h2>
      <div className="flex flex-col gap-2">
        {timerHistory?.map((timer, index) => (
          <div
            key={index}
            className="flex cursor-pointer flex-row items-center gap-1 hover:underline"
            onClick={() =>
              props.onHistoryClick(
                timer.projectId,
                timer.activityId,
                timer.note,
              )
            }
          >
            <div>
              <span className="text-sm font-semibold">{timer.projectName}</span>{" "}
              -{" "}
              <span className="text-sm font-semibold">
                {timer.activityName}
              </span>
            </div>
            <div className="text-sm">{timer.note}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
