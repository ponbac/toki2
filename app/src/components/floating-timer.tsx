import React from "react";
import { Button } from "./ui/button";
import {
  CalendarClockIcon,
  EditIcon,
  Minimize2Icon,
  PiggyBankIcon,
  SaveIcon,
  Trash2Icon,
  WatchIcon,
} from "lucide-react";
import { Input } from "./ui/input";
import { cn, formatHoursMinutes } from "@/lib/utils";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import dayjs from "dayjs";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { toast } from "sonner";
import { TimerEditDialog } from "./timer-edit-dialog";
import { TimerHistory } from "./timer-history";
import { useTimeTrackingActions, useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { useTitleStore } from "@/hooks/useTitleStore";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/popover";
import { HistoryIcon } from "lucide-react";
import { useAtomValue, useSetAtom } from "jotai/react";
import {
  lastActivityAtom,
  lastProjectAtom,
  rememberLastProjectAtom,
} from "@/lib/time-tracking-preferences";

export const FloatingTimer = () => {
  const queryClient = useQueryClient();

  const { setTimer } = useTimeTrackingActions();
  const { visible, timeSeconds, state: timerState } = useTimeTrackingTimer();
  const { hours, minutes, seconds } = secondsToHoursMinutesSeconds(
    timeSeconds ?? 0,
  );

  const [isEditDialogOpen, setIsEditDialogOpen] = React.useState(false);
  const [isMinimized, setIsMinimized] = React.useState(false);
  const [userNote, setUserNote] = React.useState("");
  const [isHistoryOpen, setIsHistoryOpen] = React.useState(false);

  const setLastProject = useSetAtom(lastProjectAtom);
  const setLastActivity = useSetAtom(lastActivityAtom);
  const rememberLastProject = useAtomValue(rememberLastProjectAtom);

  const { data: timerResponse, error: timerFetchError } = useQuery({
    ...timeTrackingQueries.getTimer(),
    enabled: timerState === "running" || timerState === undefined,
    refetchInterval: 60 * 1000,
    retry: 1,
  });
  const timer = timerResponse?.timer;

  const { mutate: startTimer } = timeTrackingMutations.useStartTimer();
  const { mutate: stopTimer, isPending: isStoppingTimer } =
    timeTrackingMutations.useStopTimer({
      onSuccess: () => {
        removeSegment("timer");
      },
    });
  const { mutate: saveTimer, isPending: isSavingTimer } =
    timeTrackingMutations.useSaveTimer();
  const { mutate: editTimer } = timeTrackingMutations.useEditTimer({
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
      // All timer types now have hours/minutes/seconds directly
      const totalSeconds =
        timer.seconds + timer.minutes * 60 + timer.hours * 3600;

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

  // If the timer could not be fetched or response indicates no active timer, reset state
  React.useEffect(() => {
    if (timerFetchError || (timerResponse && timerResponse.timer === null)) {
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });
    }
  }, [timerFetchError, timerResponse, setTimer]);

  return visible ? (
    <>
      {isMinimized ? (
        <button
          type="button"
          onClick={() => setIsMinimized(false)}
          className="fixed bottom-4 left-1/2 flex -translate-x-1/2 cursor-pointer items-center gap-2.5 rounded-full border border-border/50 bg-card/95 px-4 py-2 shadow-elevated-lg backdrop-blur-xl transition-all hover:scale-[1.02] hover:shadow-elevated-xl active:scale-[0.98] md:left-auto md:right-4 md:translate-x-0"
        >
          <span className="relative flex size-2">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
            <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
          </span>
          <span className="text-sm font-semibold tabular-nums tracking-tight text-foreground">
            {hours}:{minutes}:{seconds}
          </span>
          {(timer?.projectName || timer?.note) && (
            <span className="max-w-[120px] truncate text-xs text-muted-foreground">
              {timer?.note || timer?.projectName}
            </span>
          )}
        </button>
      ) : (
        <div className="fixed bottom-4 left-1/2 w-[90%] -translate-x-1/2 rounded-lg border border-border/50 bg-card/95 p-4 shadow-elevated-lg backdrop-blur-xl sm:w-[400px] md:left-auto md:right-4 md:translate-x-0">
          <div className="flex flex-col items-center justify-between space-y-1">
            <div className="flex w-full items-center justify-between gap-2">
              <div className="text-4xl font-bold tracking-tighter text-foreground">
                {hours}:{minutes}:{seconds}
              </div>
              <div className="flex items-center space-x-2">
                {timer?.activityName && timer.projectName ? (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={(e) => {
                          const shouldAutoRestart = !(e.ctrlKey || e.metaKey);
                          saveTimer(
                            {
                              userNote: userNote ?? "",
                            },
                            {
                              onSuccess: () => {
                                toast.success(
                                  "Timer successfully saved",
                                );
                                removeSegment("timer");
                                // Remember this project and activity for next time
                                if (timer.projectId && timer.projectName) {
                                  setLastProject({
                                    projectId: timer.projectId,
                                    projectName: timer.projectName,
                                  });
                                }
                                if (timer.activityId && timer.activityName) {
                                  setLastActivity({
                                    activityId: timer.activityId,
                                    activityName: timer.activityName,
                                  });
                                }
                                if (shouldAutoRestart) {
                                  startTimer({
                                    userNote: "Continuing my work...",
                                    ...(rememberLastProject &&
                                    timer.projectId &&
                                    timer.projectName
                                      ? {
                                          projectId: timer.projectId,
                                          projectName: timer.projectName,
                                        }
                                      : {}),
                                    ...(rememberLastProject &&
                                    timer.activityId &&
                                    timer.activityName
                                      ? {
                                          activityId: timer.activityId,
                                          activityName: timer.activityName,
                                        }
                                      : {}),
                                  });
                                }
                              },
                            },
                          );
                        }}
                        disabled={isSavingTimer || isStoppingTimer}
                      >
                        <SaveIcon className="h-6 w-6 text-muted-foreground" />
                        <span className="sr-only">Save</span>
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      Save (Ctrl/Cmd+Click to save without creating a new timer)
                    </TooltipContent>
                  </Tooltip>
                ) : null}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setIsEditDialogOpen(true)}
                      disabled={isSavingTimer || isStoppingTimer}
                    >
                      <EditIcon className="h-6 w-6 text-muted-foreground" />
                      <span className="sr-only">Edit</span>
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Edit</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => stopTimer()}
                      disabled={isSavingTimer || isStoppingTimer}
                    >
                      <Trash2Icon className="h-6 w-6 text-muted-foreground" />
                      <span className="sr-only">Delete</span>
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Delete</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setIsMinimized(true)}
                    >
                      <Minimize2Icon className="h-6 w-6 text-muted-foreground" />
                      <span className="sr-only">Minimize</span>
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Minimize</TooltipContent>
                </Tooltip>
              </div>
            </div>
            <div className="flex w-full flex-col gap-2">
              <div className="flex w-full flex-col">
                <h2 className="text-sm">{timer?.projectName}</h2>
                <h3 className="text-xs">{timer?.activityName}</h3>
              </div>
              <div className="w-full">
                <div className="relative">
                  <Input
                    type="text"
                    placeholder="Add a note..."
                    value={userNote}
                    onChange={(e) => setUserNote(e.target.value)}
                    onBlur={() =>
                      userNote !== timer?.note
                        ? editTimer({ userNote })
                        : undefined
                    }
                    className={cn(
                      "w-full rounded-md border border-border bg-background px-4 py-2 pr-10 text-foreground",
                    )}
                  />
                  <Popover open={isHistoryOpen} onOpenChange={setIsHistoryOpen}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <PopoverTrigger asChild>
                          <button
                            type="button"
                            className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-gray-500 hover:bg-accent hover:text-primary focus:outline-none"
                            aria-label="Show recent entries"
                            onMouseEnter={() =>
                              queryClient.prefetchQuery({
                                ...timeTrackingQueries.timeEntries({
                                  from: dayjs()
                                    .subtract(1, "month")
                                    .format("YYYY-MM-DD"),
                                  to: dayjs()
                                    .add(1, "day")
                                    .format("YYYY-MM-DD"),
                                  unique: true,
                                }),
                              })
                            }
                          >
                            <HistoryIcon className="size-4" />
                          </button>
                        </PopoverTrigger>
                      </TooltipTrigger>
                      <TooltipContent>Show recent entries</TooltipContent>
                    </Tooltip>
                    <PopoverContent
                      align="end"
                      className="w-[calc(100vw-2rem)] bg-card/95 p-2 backdrop-blur-xl sm:w-[42rem]"
                    >
                      <TimerHistory
                        scrollAreaClassName="min-h-72"
                        searchInputClassName="focus-visible:ring-0 focus-visible:ring-shadow-none focus-visible:shadow-none focus-visible:ring-offset-0"
                        onHistoryClick={(timeEntry) => {
                          setUserNote(timeEntry.note ?? "");
                          editTimer({
                            userNote: timeEntry.note ?? "",
                            projectId: timeEntry.projectId,
                            activityId: timeEntry.activityId,
                            projectName: timeEntry.projectName,
                            activityName: timeEntry.activityName,
                          });
                          setIsHistoryOpen(false);
                        }}
                      />
                    </PopoverContent>
                  </Popover>
                </div>
              </div>
            </div>
            <TimeSummary
              className="pt-2"
              timerHours={Number.parseInt(hours)}
              timerMinutes={Number.parseInt(minutes)}
              timerSeconds={Number.parseInt(seconds)}
            />
          </div>
        </div>
      )}
      {timer && (
        <TimerEditDialog
          key={`${isEditDialogOpen}`}
          open={isEditDialogOpen}
          onOpenChange={setIsEditDialogOpen}
          timer={timer}
        />
      )}
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
    ...timeTrackingQueries.timeInfo({
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
    ...timeTrackingQueries.timeInfo({
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
