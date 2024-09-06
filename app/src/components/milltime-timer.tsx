import React from "react";
import { Button } from "./ui/button";
import {
  CalendarClockIcon,
  Maximize2Icon,
  Minimize2Icon,
  PiggyBankIcon,
  SaveIcon,
  Trash2Icon,
  WatchIcon,
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
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { toast } from "sonner";

export const MilltimeTimer = () => {
  const { setTimer } = useMilltimeActions();
  const { visible, timeSeconds, state: timerState } = useMilltimeTimer();
  const { hours, minutes, seconds } = secondsToHoursMinutesSeconds(
    timeSeconds ?? 0,
  );

  const [isMinimized, setIsMinimized] = React.useState(false);
  const [userNote, setUserNote] = React.useState("");

  const { data: timer, error: timerFetchError } = useQuery({
    ...milltimeQueries.getTimer(),
    enabled: timerState === "running" || timerState === undefined,
    refetchInterval: 60 * 1000,
  });

  const { mutate: stopTimer, isPending: isStoppingTimer } =
    milltimeMutations.useStopTimer({
      onSuccess: () => {
        document.title = "Toki2";
      },
    });
  const { mutate: saveTimer, isPending: isSavingTimer } =
    milltimeMutations.useSaveTimer({
      onSuccess: () => {
        toast.success("Timer successfully saved to Milltime");
        document.title = "Toki2";
      },
    });
  const { mutate: editTimer } = milltimeMutations.useEditTimer({
    onSuccess: () => {
      toast.success("Timer successfully updated");
    },
    onError: () => {
      toast.error(`Failed to update timer, try refreshing the page`);
    },
  });

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
      setUserNote(timer.userNote || "");
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
        document.title = `${hours !== "00" ? `${hours}:` : ""}${minutes}:${String(parseInt(seconds) + 1).padStart(2, "0")} - ${timer?.userNote}, ${timer?.projectName}`;
      }, 1000);

      return () => clearInterval(interval!);
    } else {
      if (interval) {
        clearInterval(interval);
        document.title = "Toki2";
      }
    }
  }, [
    timeSeconds,
    timerState,
    setTimer,
    hours,
    minutes,
    seconds,
    timer?.userNote,
    timer?.projectName,
  ]);

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
                onClick={() =>
                  saveTimer({
                    userNote: timer?.userNote,
                  })
                }
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
                value={userNote}
                onChange={(e) => setUserNote(e.target.value)}
                onBlur={() =>
                  userNote !== timer?.userNote && editTimer({ userNote })
                }
                className={cn(
                  "w-full rounded-md border border-gray-300 px-4 py-2 text-gray-900 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-50",
                )}
              />
            </div>
          </div>
          {!isMinimized && (
            <TimeSummary
              className="pt-2"
              timerHours={Number.parseInt(hours)}
              timerMinutes={Number.parseInt(minutes)}
              timerSeconds={Number.parseInt(seconds)}
            />
          )}
        </div>
      </div>
    </>
  ) : null;
};

function TimeSummary(props: {
  className?: string;
  timerHours: number;
  timerMinutes: number;
  timerSeconds: number;
}) {
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
    staleTime: 5 * 60 * 1000,
  });

  const { data: timeInfoToday } = useQuery({
    ...milltimeQueries.timeInfo({
      from: dayjs().format("YYYY-MM-DD"),
      to: dayjs().format("YYYY-MM-DD"),
    }),
    staleTime: 60 * 1000,
  });

  if (!timeInfo || !timeInfoToday) {
    return null;
  }

  const timeLeft = Math.floor(
    timeInfo.periodTimeLeft - (props.timerHours + props.timerMinutes / 60),
  );
  const flexTimeTotal = Math.floor(
    timeInfo.flexTimeCurrent + props.timerHours + props.timerMinutes / 60,
  );

  const {
    hours: timeTodayHours,
    minutes: timeTodayMinutes,
    seconds: timeTodaySeconds,
  } = secondsToHoursMinutesSeconds(
    timeInfoToday.workedPeriodWithAbsenceTime * 3600 +
      props.timerHours * 3600 +
      props.timerMinutes * 60 +
      props.timerSeconds,
  );

  return (
    <div
      className={cn("flex w-full flex-row justify-between", props.className)}
    >
      <SummaryIcon
        icon={<CalendarClockIcon size={20} />}
        tooltip="Hours left to work this week"
      >
        {timeLeft}h
      </SummaryIcon>
      <SummaryIcon icon={<WatchIcon size={20} />} tooltip="Time worked today">
        {timeTodayHours}:{timeTodayMinutes}:{timeTodaySeconds}
      </SummaryIcon>
      <SummaryIcon icon={<PiggyBankIcon size={20} />} tooltip="Total flex">
        {flexTimeTotal}h
      </SummaryIcon>
    </div>
  );
}

function SummaryIcon(props: {
  icon: React.ReactNode;
  children: React.ReactNode;
  tooltip: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger className="cursor-default">
        <div className="flex flex-row items-center gap-2">
          {props.icon}
          <p className="text-sm">{props.children}</p>
        </div>
      </TooltipTrigger>
      <TooltipContent>{props.tooltip}</TooltipContent>
    </Tooltip>
  );
}

function secondsToHoursMinutesSeconds(seconds: number) {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = Math.floor(seconds % 60);

  return {
    hours: String(hours).padStart(2, "0"),
    minutes: String(minutes).padStart(2, "0"),
    seconds: String(remainingSeconds).padStart(2, "0"),
  };
}
