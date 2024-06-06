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
import {
  useMilltimeActions,
  useMilltimeIsAuthenticating,
} from "@/hooks/useMilltimeContext";

export const MilltimeLoginDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const { authenticate, setNewTimerDialogOpen } = useMilltimeActions();
  const isAuthenticating = useMilltimeIsAuthenticating();

  const resetForm = () => {
    setProjectId("");
    setActivityId("");
    setNote("");
  };

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
              userId: "104", // TODO: Get from auth!
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
            <Select onValueChange={(v) => setProjectId(v)}>
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
              Sign in
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};
