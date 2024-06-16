import React from "react";
import { Button } from "./ui/button";
import {
  CalendarClockIcon,
  Maximize2Icon,
  Minimize2Icon,
  SaveIcon,
  Trash2Icon,
} from "lucide-react";
import { Input } from "./ui/input";
import { cn } from "@/lib/utils";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";
import {
  useMilltimeActions,
  useMilltimeTimer,
} from "@/hooks/useMilltimeContext";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import dayjs from "dayjs";

export const MilltimeTimer = () => {
  const { setTimer } = useMilltimeActions();
  const { visible, timeSeconds, state: timerState } = useMilltimeTimer();

  const [isMinimized, setIsMinimized] = React.useState(false);

  const { data: timer, error: timerFetchError } = useQuery({
    ...milltimeQueries.getTimer(),
    enabled: timerState === "running" || timerState === undefined,
    refetchInterval: 60 * 1000,
  });

  const { mutate: stopTimer, isPending: isStoppingTimer } =
    milltimeMutations.useStopTimer();
  const { mutate: saveTimer, isPending: isSavingTimer } =
    milltimeMutations.useSaveTimer();

  // Sync local timer with fetched timer
  React.useEffect(() => {
    if (timer) {
      const totalSeconds =
        timer.seconds + timer.minutes * 60 + timer.hours * 3600;

      setTimer({
        visible: true,
        state: "running",
        timeSeconds: totalSeconds,
      });
    }
  }, [timer?.seconds, timer?.minutes, timer?.hours, timer, setTimer]);

  // Make it tick
  React.useEffect(() => {
    let interval: NodeJS.Timeout | null = null;
    if (timerState === "running") {
      interval = setInterval(() => {
        setTimer({
          timeSeconds: (timeSeconds ?? 0) + 1,
        });
      }, 1000);

      return () => clearInterval(interval!);
    } else {
      if (interval) {
        clearInterval(interval);
      }
    }
  }, [timeSeconds, timerState, setTimer]);

  // If the timer could not be fetched, it is probably not active
  React.useEffect(() => {
    if (timerFetchError) {
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });
    }
  }, [timerFetchError, setTimer]);

  const { hours, minutes, seconds } = secondsToHoursMinutesSeconds(
    timeSeconds ?? 0,
  );

  return visible ? (
    <>
      <div
        className={cn(
          "fixed right-4 top-4 w-[360px] rounded-lg bg-white p-4 shadow-lg dark:bg-gray-900",
          {
            "w-fit min-w-[170px] px-2 py-1": isMinimized,
          },
        )}
      >
        <div className="flex flex-col items-center justify-between space-y-1">
          <div className="flex w-full items-center justify-between gap-2">
            <div
              className={cn(
                "text-4xl font-bold tracking-tighter text-gray-900 dark:text-gray-50",
                {
                  "text-2xl": isMinimized,
                },
              )}
            >
              {hours}:{minutes}:{seconds}
            </div>
            <div
              className={cn("flex items-center space-x-2", {
                hidden: isMinimized,
              })}
            >
              <Button
                variant="ghost"
                size="icon"
                onClick={() => saveTimer()}
                disabled={isSavingTimer || isStoppingTimer}
              >
                <SaveIcon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                <span className="sr-only">Save</span>
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => stopTimer()}
                disabled={isSavingTimer || isStoppingTimer}
              >
                <Trash2Icon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                <span className="sr-only">Delete</span>
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setIsMinimized(true)}
              >
                <Minimize2Icon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                <span className="sr-only">Minimize</span>
              </Button>
            </div>
            <div
              className={cn("hidden", {
                flex: isMinimized,
              })}
            >
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setIsMinimized(false)}
              >
                <Maximize2Icon className="size-4 text-gray-500 dark:text-gray-400" />
                <span className="sr-only">Maximize</span>
              </Button>
            </div>
          </div>
          <div
            className={cn("flex w-full flex-col gap-2", {
              hidden: isMinimized,
            })}
          >
            <div className="flex w-full flex-col">
              <h2 className="text-sm">{timer?.projectName}</h2>
              <h3 className="text-xs">{timer?.activityName}</h3>
            </div>
            <div
              className={cn("w-full", {
                hidden: isMinimized,
              })}
            >
              <Input
                type="text"
                placeholder="Add a note..."
                value={timer?.userNote}
                disabled
                className="w-full rounded-md border border-gray-300 px-4 py-2 text-gray-900 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-50"
              />
            </div>
          </div>
          <TimeSummary className="pt-2" timerHours={Number.parseInt(hours)} />
        </div>
      </div>
    </>
  ) : null;
};

function secondsToHoursMinutesSeconds(seconds: number) {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = seconds % 60;

  return {
    hours: String(hours).padStart(2, "0"),
    minutes: String(minutes).padStart(2, "0"),
    seconds: String(remainingSeconds).padStart(2, "0"),
  };
}

function TimeSummary(props: { className?: string; timerHours: number }) {
  const { data: timeInfo } = useQuery({
    ...milltimeQueries.timeInfo({
      from: dayjs()
        .subtract(1, "day")
        .startOf("week")
        .add(1, "day")
        .format("YYYY-MM-DD"),
      to: dayjs()
        .subtract(1, "day")
        .endOf("week")
        .add(1, "day")
        .format("YYYY-MM-DD"),
    }),
  });

  if (!timeInfo) {
    return null;
  }

  const timeLeft = timeInfo?.periodTimeLeft - (props.timerHours ?? 0);

  return (
    <div
      className={cn("flex w-full flex-row justify-between", props.className)}
    >
      <div className="flex flex-row items-center gap-2">
        <CalendarClockIcon size={20} />
        <p className="text-sm">{timeLeft.toFixed(0)}h</p>
      </div>
      <div className="flex flex-row items-center gap-2">
        <CalendarClockIcon size={20} />
        <p className="text-sm">{timeLeft.toFixed(0)}h</p>
      </div>
      <div className="flex flex-row items-center gap-2">
        <CalendarClockIcon size={20} />
        <p className="text-sm">{timeLeft.toFixed(0)}h</p>
      </div>
    </div>
  );
}
