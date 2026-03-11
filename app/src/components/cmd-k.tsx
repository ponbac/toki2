import React from "react";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./ui/command";
import { useQuery } from "@tanstack/react-query";
import { queries } from "@/lib/api/queries/queries";
import { useNavigate } from "@tanstack/react-router";
import { AzureAvatar } from "./azure-avatar";
import {
  DrumIcon,
  FolderGit2,
  GitPullRequestIcon,
  KanbanSquare,
  TimerIcon,
} from "lucide-react";
import { ListPullRequest } from "@/lib/api/queries/pullRequests";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { toast } from "sonner";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import {
  useTimeTrackingTimer,
  useTimeTrackingActions,
} from "@/hooks/useTimeTrackingStore";
import { useTitleStore } from "@/hooks/useTitleStore";
import { useAtomValue } from "jotai/react";
import {
  lastActivityAtom,
  lastProjectAtom,
  rememberLastProjectAtom,
  buildRememberedTimerParams,
} from "@/lib/time-tracking-preferences";
import { ConfirmDefaultTimerNoteDialog } from "./confirm-default-timer-note-dialog";
import {
  CONTINUING_MY_WORK_NOTE,
  DOING_SOMETHING_IMPORTANT_NOTE,
  isDefaultStartTimerNote,
} from "@/lib/time-tracking-default-notes";

export function CmdK() {
  const [open, setOpen] = React.useState(false);
  const [pendingSaveConfirmationNote, setPendingSaveConfirmationNote] =
    React.useState<string | null>(null);

  const { removeSegment } = useTitleStore();
  const { state: timerState } = useTimeTrackingTimer();
  const { setEditTimerDialogOpen } = useTimeTrackingActions();

  const lastProject = useAtomValue(lastProjectAtom);
  const lastActivity = useAtomValue(lastActivityAtom);
  const rememberLastProject = useAtomValue(rememberLastProjectAtom);

  // TODO: should probably handle the fetched timer and saveTimer in a centralized place
  const { data: timerResponse } = useQuery({
    ...timeTrackingQueries.getTimer(),
    enabled: false,
  });
  const timer = timerResponse?.timer;
  const rememberedTimerParams = React.useMemo(
    () =>
      buildRememberedTimerParams({
        rememberLastProject,
        lastProject,
        lastActivity,
      }),
    [lastActivity, lastProject, rememberLastProject],
  );

  const { mutate: startTimer } = timeTrackingMutations.useStartTimer();
  const { mutate: saveTimer, isPending: isSavingTimer } =
    timeTrackingMutations.useSaveTimer({
      onSuccess: () => {
        toast.success("Timer successfully saved");
        removeSegment("timer");
        startTimer({
          userNote: CONTINUING_MY_WORK_NOTE,
          ...rememberedTimerParams,
        });
      },
    });
  const saveTimerDisabled = !timer?.activityName || !timer?.projectName;

  const close = React.useCallback(() => setOpen(false), []);
  const clearPendingSaveConfirmation = React.useCallback(
    () => setPendingSaveConfirmationNote(null),
    [],
  );

  const executeSave = React.useCallback(
    (note: string) => {
      saveTimer({
        userNote: note,
      });
    },
    [saveTimer],
  );

  const handleStartEmptyTimer = React.useCallback(() => {
    startTimer({
      userNote: DOING_SOMETHING_IMPORTANT_NOTE,
      ...rememberedTimerParams,
    });
    close();
  }, [close, rememberedTimerParams, startTimer]);

  const handleSaveCurrentTimer = React.useCallback(() => {
    const note = timer?.note ?? "";

    if (isDefaultStartTimerNote(note)) {
      close();
      setPendingSaveConfirmationNote(note);
      return;
    }

    executeSave(note);
    close();
  }, [close, executeSave, timer]);

  const handleEditCurrentTimer = React.useCallback(() => {
    setEditTimerDialogOpen(true);
    close();
  }, [close, setEditTimerDialogOpen]);

  React.useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((open) => !open);
      }
    };
    document.addEventListener("keydown", down);

    return () => document.removeEventListener("keydown", down);
  }, []);

  return (
    <>
      <CommandDialog open={open} onOpenChange={setOpen}>
        <CommandInput placeholder="Type a command or search..." />
        <CommandList className="max-w-2xl">
          <CommandEmpty>No results found.</CommandEmpty>
          <PagesCommandGroup close={close} />
          <ActionsCommandGroup
            timerState={timerState}
            saveTimerDisabled={saveTimerDisabled}
            onStartEmptyTimer={handleStartEmptyTimer}
            onSaveCurrentTimer={handleSaveCurrentTimer}
            onEditCurrentTimer={handleEditCurrentTimer}
          />
          <PRCommandGroup close={close} />
        </CommandList>
      </CommandDialog>
      <ConfirmDefaultTimerNoteDialog
        open={pendingSaveConfirmationNote !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) {
            clearPendingSaveConfirmation();
          }
        }}
        onConfirm={() => {
          if (!pendingSaveConfirmationNote) {
            return;
          }

          executeSave(pendingSaveConfirmationNote);
          clearPendingSaveConfirmation();
        }}
        isPending={isSavingTimer}
      />
    </>
  );
}

const PAGES = [
  { title: "Pull requests", to: "/prs", icon: GitPullRequestIcon },
  { title: "Board", to: "/board", icon: KanbanSquare },
  { title: "Time Tracking", to: "/time-tracking", icon: TimerIcon },
  { title: "Repositories", to: "/repositories", icon: FolderGit2 },
] as const;

function PagesCommandGroup(props: { close: () => void }) {
  const navigate = useNavigate();

  return (
    <CommandGroup heading="Pages">
      {PAGES.map((page) => (
        <CommandItem
          key={page.to}
          onSelect={() => {
            navigate({
              to: page.to,
            });
            props.close();
          }}
        >
          <div className="flex flex-row items-center gap-2">
            <page.icon className="h-1 w-1" />
            {page.title}
          </div>
        </CommandItem>
      ))}
    </CommandGroup>
  );
}

function ActionsCommandGroup(props: {
  timerState: "running" | "stopped" | undefined;
  saveTimerDisabled: boolean;
  onStartEmptyTimer: () => void;
  onSaveCurrentTimer: () => void;
  onEditCurrentTimer: () => void;
}) {
  return (
    <CommandGroup heading="Actions">
      {props.timerState !== "running" ? (
        <>
          <CommandItem onSelect={props.onStartEmptyTimer}>
            <div className="flex flex-row items-center gap-2">
              <DrumIcon className="h-1 w-1" />
              Start empty timer
            </div>
          </CommandItem>
        </>
      ) : (
        <>
          <CommandItem
            disabled={props.saveTimerDisabled}
            onSelect={props.onSaveCurrentTimer}
          >
            <div className="flex flex-row items-center gap-2">
              <TimerIcon className="h-1 w-1" />
              Save current timer
              {props.saveTimerDisabled && (
                <span className="text-muted-foreground">
                  (disabled, no project or activity selected)
                </span>
              )}
            </div>
          </CommandItem>
          <CommandItem onSelect={props.onEditCurrentTimer}>
            <div className="flex flex-row items-center gap-2">
              <TimerIcon className="h-1 w-1" />
              Edit current timer
            </div>
          </CommandItem>
        </>
      )}
    </CommandGroup>
  );
}

function PRCommandGroup(props: { close: () => void }) {
  const navigate = useNavigate();

  const { data: pullRequests } = useQuery(queries.listPullRequests());

  return (
    <CommandGroup heading="Pull requests">
      {pullRequests?.map((pr) => (
        <CommandItem
          key={`${pr.id}-${pr.title}`}
          value={pullRequestValue(pr)}
          onSelect={() => {
            navigate({
              to: "/prs/$prId",
              params: { prId: pr.id.toString() },
            });
            props.close();
          }}
        >
          <div className="flex w-full flex-row items-center justify-between gap-2 truncate">
            <div className="flex max-w-[75%] flex-row items-center gap-2">
              <span className="text-muted-foreground">!{pr.id}</span>
              <span className="truncate">{pr.title}</span>
            </div>
            <div className="flex flex-row items-center gap-2">
              <div className="flex flex-row items-center gap-1">
                {pr.workItems.map((wi) => (
                  <Tooltip key={wi.id}>
                    <TooltipTrigger className="text-muted-foreground">
                      #{wi.id}
                    </TooltipTrigger>
                    <TooltipContent side="left">{wi.title}</TooltipContent>
                  </Tooltip>
                ))}
              </div>
              <AzureAvatar user={pr.createdBy} disableTooltip />
            </div>
          </div>
        </CommandItem>
      ))}
    </CommandGroup>
  );
}

function pullRequestValue(pr: ListPullRequest) {
  return `!${pr.id} ${pr.title} ${pr.repoName} ${pr.createdBy.displayName} ${pr.workItems.map((wi) => `#${wi.id}`).join(" ")}`;
}
