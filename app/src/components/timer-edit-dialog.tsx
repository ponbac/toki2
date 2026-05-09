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
import { useTimeTrackingData } from "@/hooks/useTimeTrackingData";
import { Combobox } from "./combobox";
import { flushSync } from "react-dom";
import { Input } from "./ui/input";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import {
  TimerResponse,
  timeTrackingQueries,
} from "@/lib/api/queries/time-tracking";
import { TimerHistory } from "./timer-history";
import dayjs from "dayjs";
import { Label } from "./ui/label";
import { useQueryClient } from "@tanstack/react-query";

export const TimerEditDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  timer: TimerResponse;
}) => {
  const [projectId, setProjectId] = React.useState<string | undefined>(
    props.timer.projectId ?? undefined,
  );
  const [activityName, setActivityName] = React.useState<string | undefined>(
    props.timer.activityName ?? undefined,
  );
  const [note, setNote] = React.useState<string | undefined>(props.timer.note);
  const [startTimeISO, setStartTimeISO] = React.useState<string | undefined>(
    props.timer.startTime,
  );
  const activitiesRef = React.useRef<HTMLButtonElement>(null);
  const noteInputRef = React.useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();

  const { projects, activities, isProjectsLoading, isActivitiesLoading } =
    useTimeTrackingData({
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

  const prefetchActivities = React.useCallback(
    (nextProjectId: string | undefined) => {
      if (!nextProjectId) return;
      void queryClient.prefetchQuery(
        timeTrackingQueries.listActivities(nextProjectId),
      );
    },
    [queryClient],
  );

  const handleProjectChange = React.useCallback(
    (nextProjectId: string) => {
      flushSync(() => {
        setProjectId(nextProjectId);
        setActivityName(undefined);
      });
      prefetchActivities(nextProjectId);
      activitiesRef.current?.focus();
    },
    [prefetchActivities],
  );

  const closeDialog = () => {
    props.onOpenChange(false);
  };

  const { mutate: updateTimerMutate } = timeTrackingMutations.useEditTimer({
    onSuccess: closeDialog,
  });

  const updateTimer = () => {
    updateTimerMutate({
      projectId: projectId,
      projectName: selectedProject?.projectName ?? "",
      activityId: selectedActivity?.activity ?? "",
      activityName: activityName,
      userNote: note ?? "",
      startTime: startTimeISO,
    });
  };

  const hydrateDraftFromCurrentTimer = React.useEffectEvent(() => {
    setProjectId(props.timer.projectId ?? undefined);
    setActivityName(props.timer.activityName ?? undefined);
    setNote(props.timer.note);
    setStartTimeISO(props.timer.startTime);
  });

  React.useEffect(() => {
    // Initialize draft state only when dialog opens.
    // While open, keep local edits as source of truth even if timer query refetches.
    if (props.open) {
      hydrateDraftFromCurrentTimer();
      void queryClient.prefetchQuery(timeTrackingQueries.listProjects());
      prefetchActivities(props.timer.projectId ?? undefined);
    }
  }, [prefetchActivities, props.open, props.timer.projectId, queryClient]);

  const timeInputDisplayValue = React.useMemo(() => {
    return startTimeISO ? dayjs(startTimeISO).format("HH:mm") : "06:00";
  }, [startTimeISO]);

  // Handle time input change from "HH:mm"
  const handleTimeInputChange = (newTimeValue: string) => {
    const baseStartTime = startTimeISO ?? props.timer.startTime;

    if (!baseStartTime) {
      console.warn(
        "Original timer start time not available to derive date for time input change.",
      );
      return;
    }
    const originalTimerDate = dayjs(baseStartTime);
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

  if (!props.open) return null;

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-2xl">
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
                emptyMessage="No projects found"
                isLoading={isProjectsLoading}
                onOpenChange={(open) => {
                  if (open) {
                    void queryClient.prefetchQuery(
                      timeTrackingQueries.listProjects(),
                    );
                  }
                }}
                onItemMouseEnter={(nextProjectId) => {
                  prefetchActivities(nextProjectId);
                }}
                value={projectId ?? ""}
                onChange={handleProjectChange}
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
                emptyMessage="No activities found"
                isLoading={isActivitiesLoading}
                loadingMessage="Loading activities..."
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
              onHistoryClick={(timeEntry) => {
                // already selected? start timer
                if (
                  timeEntry.projectName === selectedProject?.projectName &&
                  timeEntry.activityName === selectedActivity?.activityName &&
                  timeEntry.note === note
                ) {
                  updateTimer();
                } else {
                  setProjectId(timeEntry.projectId);
                  prefetchActivities(timeEntry.projectId);
                  setActivityName(timeEntry.activityName);
                  setNote(timeEntry.note);
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
