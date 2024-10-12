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
import { Input } from "./ui/input";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import dayjs from "dayjs";
import { getWeekNumber } from "@/lib/utils";
import { TimerHistory } from "./timer-history";
import { Combobox } from "./combobox";
import { flushSync } from "react-dom";

export const MilltimeTimerDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const [projectId, setProjectId] = React.useState("");
  const [activityName, setActivityName] = React.useState("");
  const [note, setNote] = React.useState("");

  const activitiesRef = React.useRef<HTMLButtonElement>(null);
  const noteInputRef = React.useRef<HTMLInputElement>(null);

  const { projects, activities } = useMilltimeData({
    projectId: projectId,
    enabled: props.open,
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

  const { mutate: startTimerMutate } = milltimeMutations.useStartTimer({
    onSuccess: () => {
      props.onOpenChange(false);
      resetForm();
    },
  });

  const startTimer = () => {
    if (!selectedProject || !selectedActivity) {
      return;
    }

    startTimerMutate({
      activity: selectedActivity.activity,
      activityName: selectedActivity.activityName,
      projectId: selectedProject.projectId,
      projectName: selectedProject.projectName,
      userNote: note,
      regDay: dayjs().format("YYYY-MM-DD"),
      weekNumber: getWeekNumber(new Date()),
    });
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        props.onOpenChange(open);
        resetForm();
      }}
    >
      <DialogContent className="w-full max-w-2xl">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            startTimer();
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
              <Combobox
                items={
                  projects?.map((project) => ({
                    value: project.projectId.toString(),
                    label: project.projectName,
                  })) || []
                }
                placeholder="Select project..."
                onSelect={(value) => setProjectId(value)}
                emptyMessage="No projects found"
                value={projectId}
                onChange={(projectId) => {
                  flushSync(() => {
                    setProjectId(projectId);
                    setActivityName("");
                  });
                  activitiesRef.current?.focus();
                }}
              />
              <Combobox
                ref={activitiesRef}
                items={
                  activities?.map((activity) => ({
                    value: activity.activityName,
                    label: activity.activityName,
                  })) || []
                }
                placeholder="Select activity..."
                onSelect={(value) => setActivityName(value)}
                emptyMessage="No activities found"
                disabled={!projectId}
                value={activityName}
                onChange={(value) => {
                  setActivityName(value);
                  // Focus on the note input after selecting an activity
                  if (value) {
                    setTimeout(() => noteInputRef.current?.focus(), 0);
                  }
                }}
              />
              <Input
                ref={noteInputRef}
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
                  startTimer();
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
              <PlayCircleIcon className="size-5" />
              Start
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};
