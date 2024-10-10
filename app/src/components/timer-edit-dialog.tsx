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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./ui/select";
import { Input } from "./ui/input";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import { DatabaseTimer } from "@/lib/api/queries/milltime";
import { TimerHistory } from "./timer-history";

export const TimerEditDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  timer: DatabaseTimer;
}) => {
  const [projectId, setProjectId] = React.useState<string | undefined>();
  const [activityName, setActivityName] = React.useState<string | undefined>();
  const [note, setNote] = React.useState<string | undefined>();

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
      },
      {
        onSuccess: () => {
          props.onOpenChange(false);
        },
      },
    );
  };

  React.useEffect(() => {
    setProjectId(props.timer?.projectId ?? undefined);
    setActivityName(props.timer?.activityName ?? undefined);
    setNote(props.timer?.note ?? "");
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
