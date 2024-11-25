import React from "react";
import { match } from "ts-pattern";
import { Button } from "./ui/button";
import {
  CalendarClockIcon,
  EditIcon,
  Maximize2Icon,
  Minimize2Icon,
  PiggyBankIcon,
  SaveIcon,
  Trash2Icon,
  WatchIcon,
} from "lucide-react";
import { Input } from "./ui/input";
import { cn, formatHoursMinutes } from "@/lib/utils";
import {
  DatabaseTimer,
  milltimeQueries,
  TimerType,
  type MilltimeTimer,
} from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import dayjs from "dayjs";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { toast } from "sonner";
import { TimerEditDialog } from "./timer-edit-dialog";
import { useMilltimeActions, useMilltimeTimer } from "@/hooks/useMilltimeStore";
import { useTitleStore } from "@/hooks/useTitleStore";

export const FloatingMilltimeTimer = () => {
  const { setTimer } = useMilltimeActions();
  const { visible, timeSeconds, state: timerState } = useMilltimeTimer();
  const { hours, minutes, seconds } = secondsToHoursMinutesSeconds(
    timeSeconds ?? 0,
  );

  const [isEditDialogOpen, setIsEditDialogOpen] = React.useState(false);
  const [isMinimized, setIsMinimized] = React.useState(false);
  const [userNote, setUserNote] = React.useState("");

  const { data: timer, error: timerFetchError } = useQuery({
    ...milltimeQueries.getTimer(),
    enabled: timerState === "running" || timerState === undefined,
    refetchInterval: 60 * 1000,
    retry: 1,
  });

  const { mutate: startStandaloneTimer } =
    milltimeMutations.useStartStandaloneTimer();
  const { mutate: stopTimer, isPending: isStoppingTimer } =
    milltimeMutations.useStopTimer({
      onSuccess: () => {
        removeSegment("timer");
      },
    });
  const { mutate: saveTimer, isPending: isSavingTimer } =
    milltimeMutations.useSaveTimer({
      onSuccess: () => {
        toast.success("Timer successfully saved to Milltime");
        removeSegment("timer");
        startStandaloneTimer({ userNote: "Continuing my work..." });
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
  const { mutate: editStandaloneTimer } =
    milltimeMutations.useEditStandaloneTimer({
      onSuccess: () => {
        toast.success("Timer successfully updated");
      },
      onError: () => {
        toast.error(`Failed to update timer, try refreshing the page`);
      },
    });

  // Store the start time
  const startTimeRef = React.useRef<Date | null>(null);

  const { addSegment, removeSegment } = useTitleStore();

  // Sync local timer with fetched timer
  React.useEffect(() => {
    if (timer) {
      const totalSeconds = match(timer.timerType)
        .with("Milltime", () => {
          const t = timer as MilltimeTimer;
          return t.seconds + t.minutes * 60 + t.hours * 3600;
        })
        .with("Standalone", () => {
          const t = timer as DatabaseTimer;
          return dayjs().diff(dayjs(t.startTime), "second");
        })
        .exhaustive();

      // Set the start time
      startTimeRef.current = dayjs().subtract(totalSeconds, "second").toDate();

      setTimer({
        visible: true,
        state: "running",
        timeSeconds: totalSeconds,
      });
      setUserNote(timer.note || "");
    }
  }, [timer, setTimer]);

  // Make it tick
  React.useEffect(() => {
    const updateTimer = () => {
      if (startTimeRef.current) {
        const now = dayjs();
        const elapsedSeconds = now.diff(startTimeRef.current, "second");

        setTimer({
          timeSeconds: elapsedSeconds,
        });

        const { hours, minutes, seconds } =
          secondsToHoursMinutesSeconds(elapsedSeconds);

        addSegment({
          id: "timer",
          title: `${hours}:${minutes}:${seconds}${timer?.note ? ` - ${timer.note}` : ""}${
            timer?.projectName && timer?.activityName
              ? ` (${timer.projectName} - ${timer.activityName})`
              : ""
          }`,
        });
      }
    };

    updateTimer(); // Update immediately on mount

    let interval: NodeJS.Timeout | null = null;
    if (timerState === "running") {
      interval = setInterval(updateTimer, 1000);
      return () => {
        clearInterval(interval!);
        removeSegment("timer");
      };
    } else {
      if (interval) {
        clearInterval(interval);
      }
      removeSegment("timer");
    }
  }, [
    timerState,
    setTimer,
    timer?.note,
    timer?.projectName,
    timer?.activityName,
    addSegment,
    removeSegment,
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
          "fixed bottom-4 left-1/2 w-[400px] -translate-x-1/2 rounded-lg bg-gray-900/95 p-4 shadow-lg md:left-auto md:right-4 md:translate-x-0",
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
              {timer?.activityName && timer.projectName ? (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() =>
                    saveTimer({
                      timerType:
                        timer?.timerType ?? ("Unreachable" as TimerType),
                      userNote: timer?.note ?? "",
                    })
                  }
                  disabled={
                    isSavingTimer || isStoppingTimer || (timeSeconds ?? 0) < 60
                  }
                >
                  <SaveIcon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                  <span className="sr-only">Save</span>
                </Button>
              ) : null}
              {timer?.timerType === "Standalone" && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setIsEditDialogOpen(true)}
                  disabled={isSavingTimer || isStoppingTimer}
                >
                  <EditIcon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                  <span className="sr-only">Edit</span>
                </Button>
              )}
              <Button
                variant="ghost"
                size="icon"
                onClick={() =>
                  stopTimer({
                    timerType: timer?.timerType ?? ("Unreachable" as TimerType),
                  })
                }
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
                  userNote !== timer?.note
                    ? timer?.timerType === "Standalone"
                      ? editStandaloneTimer({
                          userNote,
                        })
                      : editTimer({ userNote })
                    : undefined
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
      <TimerEditDialog
        key={`${isEditDialogOpen}`}
        open={isEditDialogOpen}
        onOpenChange={setIsEditDialogOpen}
        timer={timer as DatabaseTimer}
      />
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

  const timeLeft =
    timeInfo.periodTimeLeft - (props.timerHours + props.timerMinutes / 60);
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
        {formatHoursMinutes(timeLeft)}
      </SummaryIcon>
      <SummaryIcon icon={<WatchIcon size={20} />} tooltip="Time worked today">
        {timeTodayHours}:{timeTodayMinutes}:{timeTodaySeconds}
      </SummaryIcon>
      <SummaryIcon
        icon={<PiggyBankIcon size={20} />}
        tooltip="Total flex"
        className={cn(flexTimeTotal < 0 && "text-red-500")}
      >
        {flexTimeTotal}h
      </SummaryIcon>
    </div>
  );
}

function SummaryIcon(props: {
  icon: React.ReactNode;
  children: React.ReactNode;
  tooltip: string;
  className?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger className={cn("cursor-default", props.className)}>
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
