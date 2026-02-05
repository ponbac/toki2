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
  Loader2,
  TimerIcon,
  BriefcaseIcon,
} from "lucide-react";
import { ListPullRequest } from "@/lib/api/queries/pullRequests";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { milltimeQueries, TimerType } from "@/lib/api/queries/milltime";
import { toast } from "sonner";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import {
  useMilltimeTimer,
  useMilltimeActions,
  useMilltimeIsAuthenticated,
} from "@/hooks/useMilltimeStore";
import { useTitleStore } from "@/hooks/useTitleStore";
import { useAtomValue } from "jotai/react";
import {
  lastActivityAtom,
  lastProjectAtom,
  rememberLastProjectAtom,
  buildRememberedTimerParams,
} from "@/lib/milltime-preferences";
import { SearchResult } from "@/lib/api/queries/search";
import { useDebounce } from "@/hooks/useDebounce";

export function CmdK() {
  const [open, setOpen] = React.useState(false);
  const [searchInput, setSearchInput] = React.useState("");
  const debouncedSearchInput = useDebounce(searchInput, 300);

  const close = () => setOpen(false);

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

  React.useEffect(() => {
    if (!open) {
      setSearchInput("");
    }
  }, [open]);

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput
        placeholder="Type a command or search..."
        value={searchInput}
        onValueChange={setSearchInput}
      />
      <CommandList className="max-w-2xl">
        <CommandEmpty>No results found.</CommandEmpty>
        <PagesCommandGroup close={close} />
        <ActionsCommandGroup close={close} />
        <SearchCommandGroup
          close={close}
          searchQuery={debouncedSearchInput}
          isDebouncing={searchInput !== debouncedSearchInput}
        />
        <PRCommandGroup close={close} />
      </CommandList>
    </CommandDialog>
  );
}

const PAGES = [
  { title: "Pull requests", to: "/prs", icon: GitPullRequestIcon },
  { title: "Milltime", to: "/milltime", icon: TimerIcon },
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

function ActionsCommandGroup(props: { close: () => void }) {
  const { removeSegment } = useTitleStore();
  const { state: timerState } = useMilltimeTimer();
  const { setNewTimerDialogOpen, setLoginDialogOpen, setEditTimerDialogOpen } =
    useMilltimeActions();
  const isAuthenticatedToMilltime = useMilltimeIsAuthenticated();

  const lastProject = useAtomValue(lastProjectAtom);
  const lastActivity = useAtomValue(lastActivityAtom);
  const rememberLastProject = useAtomValue(rememberLastProjectAtom);

  // TODO: should probably handle the fetched timer and saveTimer in a centralized place
  const { data: timerResponse } = useQuery({
    ...milltimeQueries.getTimer(),
    enabled: false,
  });
  const timer = timerResponse?.timer;

  const { mutate: startStandaloneTimer } =
    milltimeMutations.useStartStandaloneTimer();

  const { mutate: saveTimer } = milltimeMutations.useSaveTimer({
    onSuccess: () => {
      toast.success("Timer successfully saved to Milltime");
      removeSegment("timer");
      startStandaloneTimer({
        userNote: "Continuing my work...",
        ...buildRememberedTimerParams({
          rememberLastProject,
          lastProject,
          lastActivity,
        }),
      });
    },
  });

  const saveTimerDisabled = !timer?.activityName || !timer?.projectName;

  return (
    <CommandGroup heading="Actions">
      {timerState !== "running" ? (
        <>
          <CommandItem
            onSelect={() => {
              startStandaloneTimer({
                userNote: "Doing something important...",
                ...buildRememberedTimerParams({
                  rememberLastProject,
                  lastProject,
                  lastActivity,
                }),
              });
              props.close();
            }}
          >
            <div className="flex flex-row items-center gap-2">
              <DrumIcon className="h-1 w-1" />
              Start empty timer
            </div>
          </CommandItem>
          <CommandItem
            disabled
            onSelect={() => {
              // TODO: should probably extract this to some kind of guard hook
              if (isAuthenticatedToMilltime) {
                setNewTimerDialogOpen(true);
                props.close();
              } else {
                setLoginDialogOpen(true);
                props.close();
              }
            }}
          >
            <div className="flex flex-row items-center gap-2 line-through">
              <TimerIcon className="h-1 w-1" />
              Start Milltime timer
            </div>
          </CommandItem>
        </>
      ) : (
        <>
          <CommandItem
            disabled={saveTimerDisabled}
            onSelect={() => {
              saveTimer({
                timerType: timer?.timerType ?? ("Unreachable" as TimerType),
                userNote: timer?.note ?? "",
              });
              props.close();
            }}
          >
            <div className="flex flex-row items-center gap-2">
              <TimerIcon className="h-1 w-1" />
              Save current timer
              {saveTimerDisabled && (
                <span className="text-muted-foreground">
                  (disabled, no project or activity selected)
                </span>
              )}
            </div>
          </CommandItem>
          <CommandItem
            onSelect={() => {
              setEditTimerDialogOpen(true);
              props.close();
            }}
          >
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

  const { data: pullRequests } = useQuery(queries.pullRequests.listPullRequests());

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

function SearchCommandGroup(props: {
  close: () => void;
  searchQuery: string;
  isDebouncing: boolean;
}) {
  const navigate = useNavigate();

  const { data: searchResults, isLoading, isFetching, isError } = useQuery(
    queries.search.search(props.searchQuery, 10)
  );

  if (!props.searchQuery && !props.isDebouncing) {
    return null;
  }

  if (isLoading || props.isDebouncing) {
    return (
      <CommandGroup heading="Search results">
        <CommandItem disabled>
          <div className="flex flex-row items-center gap-2 text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            Searching...
          </div>
        </CommandItem>
      </CommandGroup>
    );
  }

  if (isError) {
    return (
      <CommandGroup heading="Search results">
        <CommandItem disabled>
          <span className="text-destructive">Search failed. Try again.</span>
        </CommandItem>
      </CommandGroup>
    );
  }

  if (!searchResults || searchResults.length === 0) {
    return null;
  }

  return (
    <CommandGroup
      heading={
        <span className="flex items-center gap-2">
          Search results
          {isFetching && (
            <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
          )}
        </span>
      }
    >
      {searchResults.map((result) => (
        <CommandItem
          key={`${result.sourceType}-${result.externalId}`}
          value={searchResultValue(result)}
          onSelect={() => {
            if (result.sourceType === "Pr") {
              navigate({
                to: "/prs/$prId",
                params: { prId: result.externalId.toString() },
              });
            } else {
              window.open(result.url, "_blank", "noopener,noreferrer");
            }
            props.close();
          }}
        >
          <div className="flex w-full flex-row items-center justify-between gap-2 truncate">
            <div className="flex max-w-[75%] flex-row items-center gap-2">
              {result.sourceType === "Pr" ? (
                <GitPullRequestIcon className="h-4 w-4 text-muted-foreground" />
              ) : (
                <BriefcaseIcon className="h-4 w-4 text-muted-foreground" />
              )}
              <span className="text-muted-foreground">
                {result.sourceType === "Pr" ? "!" : "#"}
                {result.externalId}
              </span>
              <span className="truncate">{result.title}</span>
            </div>
            {result.authorName && (
              <span className="text-xs text-muted-foreground">
                {result.authorName}
              </span>
            )}
          </div>
        </CommandItem>
      ))}
    </CommandGroup>
  );
}

function searchResultValue(result: SearchResult) {
  return `${result.sourceType === "Pr" ? "!" : "#"}${result.externalId} ${result.title} ${result.authorName ?? ""}`;
}
