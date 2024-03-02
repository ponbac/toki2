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
  Activity,
  FolderGit2,
  GitPullRequestIcon,
  HomeIcon,
} from "lucide-react";
import { PullRequest } from "@/lib/api/queries/pullRequests";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export function CmdK() {
  const [open, setOpen] = React.useState(false);

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

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Type a command or search..." />
      <CommandList className="max-w-2xl">
        <CommandEmpty>No results found.</CommandEmpty>
        <PagesCommandGroup close={close} />
        <PRCommandGroup close={close} />
      </CommandList>
    </CommandDialog>
  );
}

const PAGES = [
  { title: "Home", to: "/", icon: HomeIcon },
  { title: "Pull requests", to: "/prs", icon: GitPullRequestIcon },
  { title: "Commits", to: "/prs/commits", icon: Activity },
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

function PRCommandGroup(props: { close: () => void }) {
  const navigate = useNavigate();

  const { data: pullRequests } = useQuery(queries.cachedPullRequests());

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
                  <Tooltip>
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

function pullRequestValue(pr: PullRequest) {
  return `${pr.id} ${pr.title} ${pr.repoName} ${pr.createdBy.displayName} ${pr.workItems.map((wi) => `#${wi.id}`).join(" ")}`;
}
