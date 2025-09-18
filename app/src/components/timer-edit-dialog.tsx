import { EditIcon } from "lucide-react";
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
import { Combobox } from "./combobox";
import { flushSync } from "react-dom";
import { Input } from "./ui/input";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import { DatabaseTimer } from "@/lib/api/queries/milltime";
import { TimerHistory } from "./timer-history";
import dayjs from "dayjs";
import { Label } from "./ui/label";

export const TimerEditDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  timer: DatabaseTimer;
}) => {
  const [projectId, setProjectId] = React.useState<string | undefined>(
    props.timer?.projectId ?? undefined,
  );
  const [activityName, setActivityName] = React.useState<string | undefined>(
    props.timer?.activityName ?? undefined,
  );
  const [note, setNote] = React.useState<string | undefined>(
    props.timer?.note ?? undefined,
  );
  const [startTimeISO, setStartTimeISO] = React.useState<string | undefined>(
    props.timer?.startTime,
  );

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
        userNote: note ?? "",
        startTime: startTimeISO,
      },
      {
        onSuccess: () => {
          props.onOpenChange(false);
        },
      },
    );
  };

  React.useEffect(() => {
    // Synchronize state with props.timer when it changes
    setProjectId(props.timer?.projectId ?? undefined);
    setActivityName(props.timer?.activityName ?? undefined);
    setNote(props.timer?.note ?? undefined); // Convert null to undefined for consistency
    setStartTimeISO(props.timer?.startTime);
  }, [props.timer]);

  const activitiesRef = React.useRef<HTMLButtonElement>(null);
  const noteInputRef = React.useRef<HTMLInputElement>(null);

  const timeInputDisplayValue = React.useMemo(() => {
    return startTimeISO ? dayjs(startTimeISO).format("HH:mm") : "06:00";
  }, [startTimeISO]);

  // Handle time input change from "HH:mm"
  const handleTimeInputChange = (newTimeValue: string) => {
    if (!props.timer?.startTime) {
      console.warn(
        "Original timer start time not available to derive date for time input change.",
      );
      return;
    }
    const originalTimerDate = dayjs(props.timer.startTime); // Base date from original timer
    const [hours, minutes] = newTimeValue.split(":").map(Number);

    let newFullDateTime = originalTimerDate
      .hour(hours)
      .minute(minutes)
      .second(0)
      .millisecond(0);

    // If the new start time is in the future, set it to the current time
    const now = dayjs();
    if (newFullDateTime.isAfter(now)) {
      newFullDateTime = now.second(0).millisecond(0); // Also reset seconds and milliseconds for consistency
    }

    setStartTimeISO(newFullDateTime.toISOString());
  };

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
              <Combobox
                items={
                  projects?.map((project) => ({
                    value: project.projectId,
                    label: project.projectName,
                  })) || []
                }
                placeholder="Select project..."
                searchPlaceholder="Search projects..."
                onSelect={(value) => setProjectId(value)}
                emptyMessage="No projects found"
                value={projectId ?? ""}
                onChange={(projectId) => {
                  flushSync(() => {
                    setProjectId(projectId);
                    setActivityName(undefined);
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
                searchPlaceholder="Search activities..."
                onSelect={(value) => setActivityName(value)}
                emptyMessage="No activities found"
                disabled={!projectId}
                value={activityName ?? ""}
                onChange={(value) => {
                  setActivityName(value);
                  if (value) {
                    noteInputRef.current?.focus();
                  }
                }}
              />
              <Input
                ref={noteInputRef}
                placeholder="Note"
                value={note ?? ""}
                onChange={(e) => setNote(e.target.value)}
              />
              <div className="mt-2 flex w-32 flex-col gap-2">
                <Label htmlFor="timer-start-time">Start Time</Label>
                <Input
                  id="timer-start-time"
                  type="time"
                  value={timeInputDisplayValue}
                  onChange={(e) => handleTimeInputChange(e.target.value)}
                />
              </div>
            </div>
            <TimerHistory
              className="mt-4"
              onHistoryClick={(projectName, activityName, clickedNote) => {
                // already selected? start timer
                if (
                  projectName === selectedProject?.projectName &&
                  activityName === selectedActivity?.activityName &&
                  clickedNote === note
                ) {
                  updateTimer();
                } else {
                  setProjectId(
                    projects
                      ?.find((p) => p.projectName === projectName)
                      ?.projectId.toString() ?? "",
                  );
                  setActivityName(activityName);
                  setNote(clickedNote);
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
              disabled={!!projectId !== !!activityName}
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
