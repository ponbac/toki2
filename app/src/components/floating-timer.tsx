import React from "react";
import { Button } from "./ui/button";
import { ConfirmDefaultTimerNoteDialog } from "./confirm-default-timer-note-dialog";
import {
  CalendarClockIcon,
  EditIcon,
  Minimize2Icon,
  PiggyBankIcon,
  SaveIcon,
  Trash2Icon,
  WatchIcon,
} from "lucide-react";
import { cn, formatHoursMinutes } from "@/lib/utils";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  timeTrackingMutations,
  type EditTimerPayload,
} from "@/lib/api/mutations/time-tracking";
import { apiErrorToast } from "@/lib/api/errors";
import dayjs from "dayjs";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { toast } from "sonner";
import { TimerEditDialog } from "./timer-edit-dialog";
import { TimerHistory } from "./timer-history";
import {
  useTimeTrackingActions,
  useTimeTrackingTimer,
} from "@/hooks/useTimeTrackingStore";
import { useTitleStore } from "@/hooks/useTitleStore";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/popover";
import { HistoryIcon } from "lucide-react";
import { useAtomValue, useSetAtom } from "jotai/react";
import {
  lastActivityAtom,
  lastProjectAtom,
  rememberLastProjectAtom,
} from "@/lib/time-tracking-preferences";
import {
  CONTINUING_MY_WORK_NOTE,
  isDefaultStartTimerNote,
} from "@/lib/time-tracking-default-notes";
import { Textarea } from "./ui/textarea";
import { ScrollArea } from "./ui/scroll-area";
import {
  confirmTimerNoteDraftSaved,
  editTimerNoteDraft,
  EMPTY_TIMER_NOTE_DRAFT,
  isTimerNoteDraftDirty,
  syncTimerNoteDraft,
  type ServerTimerNote,
} from "@/lib/timer-note-draft";

type PendingSaveConfirmation = {
  note: string;
  shouldAutoRestart: boolean;
};

const NOTE_TEXTAREA_MIN_HEIGHT = 40;
const NOTE_TEXTAREA_MAX_HEIGHT = 240;

function measureTimerNoteTextarea(textarea: HTMLTextAreaElement) {
  textarea.style.height = `${NOTE_TEXTAREA_MIN_HEIGHT}px`;
  const contentHeight = Math.max(
    textarea.scrollHeight,
    NOTE_TEXTAREA_MIN_HEIGHT,
  );
  textarea.style.height = `${contentHeight}px`;

  return Math.min(contentHeight, NOTE_TEXTAREA_MAX_HEIGHT);
}

function syncTimerNoteTextareaHeight(
  textarea: HTMLTextAreaElement,
  setViewportHeight: React.Dispatch<React.SetStateAction<number>>,
) {
  const viewportHeight = measureTimerNoteTextarea(textarea);
  setViewportHeight((currentHeight) =>
    currentHeight === viewportHeight ? currentHeight : viewportHeight,
  );
}

function scheduleTimerNoteTextareaHeightSync(
  textareaRef: React.RefObject<HTMLTextAreaElement | null>,
  setViewportHeight: React.Dispatch<React.SetStateAction<number>>,
) {
  requestAnimationFrame(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    syncTimerNoteTextareaHeight(textarea, setViewportHeight);
  });
}

export const FloatingTimer = () => {
  const queryClient = useQueryClient();

  const { setTimer } = useTimeTrackingActions();
  const { visible, timeSeconds, state: timerState } = useTimeTrackingTimer();
  const { hours, minutes, seconds } = secondsToHoursMinutesSeconds(
    timeSeconds ?? 0,
  );

  const [isEditDialogOpen, setIsEditDialogOpen] = React.useState(false);
  const [isMinimized, setIsMinimized] = React.useState(false);
  const [timerNoteDraft, setTimerNoteDraft] = React.useState(
    EMPTY_TIMER_NOTE_DRAFT,
  );
  const userNote = timerNoteDraft.note;
  const [isHistoryOpen, setIsHistoryOpen] = React.useState(false);
  const [isSaveQueued, setIsSaveQueued] = React.useState(false);
  const [pendingSaveConfirmation, setPendingSaveConfirmation] =
    React.useState<PendingSaveConfirmation | null>(null);
  const noteTextareaRef = React.useRef<HTMLTextAreaElement>(null);
  const timerMutationQueueRef = React.useRef<Promise<void>>(Promise.resolve());
  const [noteTextareaViewportHeight, setNoteTextareaViewportHeight] =
    React.useState(NOTE_TEXTAREA_MIN_HEIGHT);

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
  const timerForEditDialog = React.useMemo(
    () => (timer ? { ...timer, note: userNote } : null),
    [timer, userNote],
  );
  const restartTimerParams = React.useMemo(() => {
    if (!rememberLastProject || !timer) {
      return {};
    }

    return {
      ...(timer.projectId && timer.projectName
        ? {
            projectId: timer.projectId,
            projectName: timer.projectName,
          }
        : {}),
      ...(timer.activityId && timer.activityName
        ? {
            activityId: timer.activityId,
            activityName: timer.activityName,
          }
        : {}),
    };
  }, [rememberLastProject, timer]);

  const { mutate: stopTimer, isPending: isStoppingTimer } =
    timeTrackingMutations.useStopTimer({
      onSuccess: () => {
        removeSegment("timer");
      },
    });
  const { mutateAsync: saveTimer, isPending: isSavingTimer } =
    timeTrackingMutations.useSaveTimer({
      onError: apiErrorToast("Failed to save timer"),
    });
  const { mutateAsync: editTimer } = timeTrackingMutations.useEditTimer({
    onSuccess: () => {
      toast.success("Timer successfully updated");
    },
    onError: apiErrorToast("Failed to update timer"),
  });

  // Store the start time
  const startTimeRef = React.useRef<Date | null>(null);

  const { addSegment, removeSegment } = useTitleStore();

  const queueTimerEdit = React.useCallback(
    (body: EditTimerPayload, savedTimerNote?: ServerTimerNote) => {
      const editOperation = timerMutationQueueRef.current.then(async () => {
        await editTimer(body);

        if (savedTimerNote) {
          setTimerNoteDraft((currentDraft) =>
            confirmTimerNoteDraftSaved(currentDraft, savedTimerNote),
          );
        }
      });
      const settledOperation = editOperation.catch(() => undefined);
      timerMutationQueueRef.current = settledOperation;

      return settledOperation;
    },
    [editTimer],
  );

  const clearPendingSaveConfirmation = React.useCallback(
    () => setPendingSaveConfirmation(null),
    [],
  );
  const rememberCurrentTimerSelection = React.useCallback(() => {
    if (!timer) {
      return;
    }

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
  }, [setLastActivity, setLastProject, timer]);

  const executeSave = React.useCallback(
    (note: string, shouldAutoRestart: boolean) => {
      if (!timer) {
        return;
      }

      setIsSaveQueued(true);
      const saveOperation = timerMutationQueueRef.current.then(async () => {
        try {
          await saveTimer(
            {
              userNote: note,
              restartTimer: shouldAutoRestart
                ? {
                    userNote: CONTINUING_MY_WORK_NOTE,
                    ...restartTimerParams,
                  }
                : undefined,
            },
            {
              onSuccess: () => {
                toast.success("Timer successfully saved");
                if (!shouldAutoRestart) {
                  removeSegment("timer");
                }
                rememberCurrentTimerSelection();
              },
            },
          );
        } finally {
          setIsSaveQueued(false);
        }
      });
      const settledOperation = saveOperation.catch(() => undefined);
      timerMutationQueueRef.current = settledOperation;
    },
    [
      removeSegment,
      rememberCurrentTimerSelection,
      restartTimerParams,
      saveTimer,
      timer,
    ],
  );

  const handleSaveButtonClick = React.useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      const note = noteTextareaRef.current?.value ?? userNote;
      const shouldAutoRestart = !(e.ctrlKey || e.metaKey);

      if (isDefaultStartTimerNote(note)) {
        setPendingSaveConfirmation({ note, shouldAutoRestart });
        return;
      }

      executeSave(note, shouldAutoRestart);
    },
    [executeSave, userNote],
  );

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
      const serverTimerNote: ServerTimerNote = {
        timerStartTime: timer.startTime,
        note: timer.note || "",
      };
      setTimerNoteDraft((currentDraft) =>
        syncTimerNoteDraft(currentDraft, serverTimerNote),
      );
      scheduleTimerNoteTextareaHeightSync(
        noteTextareaRef,
        setNoteTextareaViewportHeight,
      );
    }
  }, [timer, setTimer]);

  React.useLayoutEffect(() => {
    if (isMinimized) {
      return;
    }

    const textarea = noteTextareaRef.current;
    if (!textarea) {
      return;
    }

    syncTimerNoteTextareaHeight(textarea, setNoteTextareaViewportHeight);
  }, [isMinimized]);

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

    if (timerState !== "running") {
      removeSegment("timer");
      return;
    }

    const interval = setInterval(updateTimer, 1000);
    return () => {
      clearInterval(interval);
      removeSegment("timer");
    };
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
          className="fixed bottom-4 left-1/2 z-40 flex -translate-x-1/2 cursor-pointer items-center gap-2.5 rounded-full border border-border/50 bg-card/75 px-4 py-2 shadow-elevated-lg backdrop-blur-xl transition-all hover:scale-[1.02] hover:shadow-elevated-xl active:scale-[0.98] md:left-auto md:right-4 md:translate-x-0"
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
        <div className="fixed bottom-4 left-1/2 z-40 w-[90%] -translate-x-1/2 rounded-lg border border-border/50 bg-card/75 p-4 shadow-elevated-lg backdrop-blur-xl sm:w-[400px] md:left-auto md:right-4 md:translate-x-0">
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
                        onClick={handleSaveButtonClick}
                        disabled={
                          isSaveQueued || isSavingTimer || isStoppingTimer
                        }
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
                      disabled={
                        isSaveQueued || isSavingTimer || isStoppingTimer
                      }
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
                      disabled={
                        isSaveQueued || isSavingTimer || isStoppingTimer
                      }
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
                  <ScrollArea
                    style={{
                      height: noteTextareaViewportHeight,
                      minHeight: NOTE_TEXTAREA_MIN_HEIGHT,
                    }}
                    className="rounded-md border border-border bg-background pr-10 ring-offset-background transition-[height] duration-150 ease-out focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-2 motion-reduce:transition-none"
                  >
                    <div className="pl-1.5">
                      <Textarea
                        ref={(textarea) => {
                          noteTextareaRef.current = textarea;
                          if (!textarea) {
                            return;
                          }

                          syncTimerNoteTextareaHeight(
                            textarea,
                            setNoteTextareaViewportHeight,
                          );

                          return () => {
                            noteTextareaRef.current = null;
                          };
                        }}
                        placeholder="Add a note..."
                        rows={1}
                        wrap="off"
                        value={userNote}
                        disabled={isSaveQueued}
                        onChange={(e) => {
                          const note = e.currentTarget.value;
                          syncTimerNoteTextareaHeight(
                            e.currentTarget,
                            setNoteTextareaViewportHeight,
                          );
                          setTimerNoteDraft((currentDraft) =>
                            editTimerNoteDraft(currentDraft, note),
                          );
                        }}
                        onBlur={() => {
                          const nextNote =
                            noteTextareaRef.current?.value ?? userNote;
                          const nextDraft = editTimerNoteDraft(
                            timerNoteDraft,
                            nextNote,
                          );

                          if (!timer || !isTimerNoteDraftDirty(nextDraft)) {
                            return;
                          }

                          setTimerNoteDraft(nextDraft);
                          void queueTimerEdit(
                            { userNote: nextNote },
                            {
                              timerStartTime: timer.startTime,
                              note: nextNote,
                            },
                          );
                        }}
                        style={{
                          minHeight: NOTE_TEXTAREA_MIN_HEIGHT,
                          scrollbarWidth: "none",
                        }}
                        className="block min-h-10 resize-none overflow-x-auto overflow-y-hidden whitespace-pre rounded-none border-0 bg-transparent py-2 pl-0 pr-1 text-foreground shadow-none outline-none ring-offset-transparent focus-visible:ring-0 focus-visible:ring-offset-0 [&::-webkit-scrollbar]:hidden"
                      />
                    </div>
                  </ScrollArea>
                  <Popover open={isHistoryOpen} onOpenChange={setIsHistoryOpen}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <PopoverTrigger asChild>
                          <button
                            type="button"
                            disabled={isSaveQueued}
                            className="absolute right-2 top-2 z-10 grid size-6 place-items-center rounded-md border border-border/70 bg-background/95 text-muted-foreground shadow-sm backdrop-blur transition-colors hover:bg-accent hover:text-primary focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
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
                          const note = timeEntry.note ?? "";
                          setTimerNoteDraft((currentDraft) =>
                            editTimerNoteDraft(currentDraft, note),
                          );
                          if (timer) {
                            void queueTimerEdit(
                              {
                                userNote: note,
                                projectId: timeEntry.projectId,
                                activityId: timeEntry.activityId,
                                projectName: timeEntry.projectName,
                                activityName: timeEntry.activityName,
                              },
                              {
                                timerStartTime: timer.startTime,
                                note,
                              },
                            );
                          }
                          scheduleTimerNoteTextareaHeightSync(
                            noteTextareaRef,
                            setNoteTextareaViewportHeight,
                          );
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
              timerHours={Number.parseInt(hours, 10)}
              timerMinutes={Number.parseInt(minutes, 10)}
              timerSeconds={Number.parseInt(seconds, 10)}
            />
          </div>
        </div>
      )}
      {timerForEditDialog && (
        <TimerEditDialog
          key={`${isEditDialogOpen}`}
          open={isEditDialogOpen}
          onOpenChange={setIsEditDialogOpen}
          timer={timerForEditDialog}
        />
      )}
      <ConfirmDefaultTimerNoteDialog
        open={pendingSaveConfirmation !== null}
        onOpenChange={(open) => {
          if (!open) {
            clearPendingSaveConfirmation();
          }
        }}
        onConfirm={() => {
          if (!pendingSaveConfirmation) {
            return;
          }

          executeSave(
            pendingSaveConfirmation.note,
            pendingSaveConfirmation.shouldAutoRestart,
          );
          clearPendingSaveConfirmation();
        }}
        isPending={isSaveQueued || isSavingTimer || isStoppingTimer}
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
    timeInfo.remainingHours - (props.timerHours + props.timerMinutes / 60);
  const flexTime =
    timeInfo.periodFlexHours + props.timerHours + props.timerMinutes / 60;
  const flexLabel = `${flexTime > 0 ? "+" : ""}${formatHoursMinutes(flexTime)}`;
  const flexColor =
    flexTime > 0
      ? "text-emerald-500"
      : flexTime < 0
        ? "text-amber-500"
        : undefined;

  const {
    hours: timeTodayHours,
    minutes: timeTodayMinutes,
    seconds: timeTodaySeconds,
  } = secondsToHoursMinutesSeconds(
    timeInfoToday.coveredHours * 3600 +
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
        tooltip="Estimated flex this week, including leave"
        className={flexColor}
      >
        {flexLabel}
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
