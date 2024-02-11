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

export function CmdK() {
  const [open, setOpen] = React.useState(true);

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
        <PRCommandGroup close={close} />
        <CommandGroup heading="Suggestions">
          <CommandItem>Calendar</CommandItem>
          <CommandItem>Search Emoji</CommandItem>
          <CommandItem>Calculator</CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}

function PRCommandGroup(props: { close: () => void }) {
  const navigate = useNavigate();

  const { data: pullRequests } = useQuery(queries.cachedPullRequests());

  return (
    <CommandGroup heading="Pull requests">
      {pullRequests?.map((pr) => (
        <CommandItem
          onSelect={() => {
            navigate({
              to: "/prs/$prId",
              params: { prId: pr.id.toString() },
            });
            props.close();
          }}
        >
          <div className="flex w-full flex-row items-center justify-between gap-2 truncate">
            <span className="truncate">{pr.title}</span>
            <AzureAvatar user={pr.createdBy} />
          </div>
        </CommandItem>
      ))}
    </CommandGroup>
  );
}
