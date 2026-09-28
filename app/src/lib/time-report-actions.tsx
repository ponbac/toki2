import type { TimerResponse } from "@/lib/api/queries/time-tracking";
import type {
  EditTimerMutationAsync,
  StartTimerMutationAsync,
} from "@/lib/api/mutations/time-tracking";
import { TimerIcon } from "lucide-react";
import { toast } from "sonner";

export async function syncTimeReportToTimer({
  text,
  timer,
  timerQuerySuccess,
  startTimer,
  editTimer,
}: {
  text: string;
  timer: TimerResponse | null | undefined;
  timerQuerySuccess: boolean;
  startTimer: StartTimerMutationAsync;
  editTimer: EditTimerMutationAsync;
}) {
  if (!timerQuerySuccess) {
    toast.error("Timer is unavailable, could not set timer note.");
    return;
  }

  try {
    if (timer) {
      await editTimer({ userNote: text });
      toast.success(
        <div className="flex flex-row items-center">
          <TimerIcon className="mr-2 inline-block" size="1.25rem" />
          <p className="text-pretty">
            Timer note set to <span className="font-mono">{text}</span>
          </p>
        </div>,
      );
      return;
    }

    if (timer === null) {
      await startTimer({ userNote: text });
      toast.success(
        <div className="flex flex-row items-center">
          <TimerIcon className="mr-2 inline-block" size="1.25rem" />
          <p className="text-pretty">
            Timer started: <span className="font-mono">{text}</span>
          </p>
        </div>,
      );
    }
  } catch {
    toast.error("Failed to update timer.");
  }
}
