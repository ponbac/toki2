import type { TimerResponse } from "@/lib/api/queries/time-tracking";
import type {
  EditTimerMutationAsync,
  StartTimerMutationAsync,
} from "@/lib/api/mutations/time-tracking";
import { ClipboardCopy, TimerIcon } from "lucide-react";
import { toast } from "sonner";

export async function copyAndSyncTimeReport({
  text,
  timer,
  timerQuerySuccess,
  startTimer,
  editTimer,
  onTimerSyncError,
}: {
  text: string;
  timer: TimerResponse | null | undefined;
  timerQuerySuccess: boolean;
  startTimer: StartTimerMutationAsync;
  editTimer: EditTimerMutationAsync;
  onTimerSyncError?: () => void;
}) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    toast.error("Failed to copy time report text.");
    return;
  }

  toast.info(
    <div className="flex flex-row items-center">
      <ClipboardCopy className="mr-2 inline-block" size="1.25rem" />
      <p className="text-pretty">
        Copied <span className="font-mono">{text}</span> to clipboard
      </p>
    </div>,
  );

  if (!timerQuerySuccess) {
    return;
  }

  try {
    if (timer) {
      await editTimer({ userNote: text });
      toast.success(
        <div className="flex flex-row items-center">
          <TimerIcon className="mr-2 inline-block" size="1.25rem" />
          Timer note updated
        </div>,
      );
      return;
    }

    if (timer === null) {
      await startTimer({ userNote: text });
      toast.success(
        <div className="flex flex-row items-center">
          <TimerIcon className="mr-2 inline-block" size="1.25rem" />
          Timer started
        </div>,
      );
    }
  } catch {
    onTimerSyncError?.();
  }
}
