import { toast } from "sonner";
import { apiErrorToast } from "@/lib/api/errors";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";

export type StartAgainTimerParams = {
  note: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
};

export function useStartAgainTimer() {
  const { mutateAsync: startTimerAsync, isPending: isStarting } =
    timeTrackingMutations.useStartTimer();
  const { mutateAsync: editTimerAsync } = timeTrackingMutations.useEditTimer();
  const { state: timerState } = useTimeTrackingTimer();

  const startAgain = (params: StartAgainTimerParams) => {
    const isTimerActive = timerState === "running";

    if (isTimerActive) {
      editTimerAsync({
        userNote: params.note,
        projectId: params.projectId,
        projectName: params.projectName,
        activityId: params.activityId,
        activityName: params.activityName,
      })
        .then(() => toast.success("Timer updated"))
        .catch(apiErrorToast("Failed to update timer"));
      return;
    }

    startTimerAsync({
      userNote: params.note,
      projectId: params.projectId,
      activityId: params.activityId,
    })
      .then(() => toast.success("Timer started"))
      .catch(apiErrorToast("Failed to start timer"));
  };

  return { isStarting, startAgain };
}
