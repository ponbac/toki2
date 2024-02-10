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

export function CmdK() {
  const [open, setOpen] = React.useState(true);

  const { data: pullRequests } = useQuery(queries.cachedPullRequests());

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
      <CommandList>
        <CommandEmpty>No results found.</CommandEmpty>
        <CommandGroup heading="Pull requests">
          {pullRequests?.map((pr) => <PRCommandItem key={pr.id} pr={pr} />)}
        </CommandGroup>
        <CommandGroup heading="Suggestions">
          <CommandItem>Calendar</CommandItem>
          <CommandItem>Search Emoji</CommandItem>
          <CommandItem>Calculator</CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}

function PRCommandItem(props: { pr: any }) {
  return (
    <CommandItem value={props.pr.id} onSelect={(pr) => console.log(pr)}>
      <div className="flex flex-row items-center justify-between gap-2">
        <span>{props.pr.title}</span>
        <img
          src={props.pr.createdBy.avatarUrl}
          alt={props.pr.createdBy.displayName}
          className="h-6 w-6 rounded-full"
        />
      </div>
    </CommandItem>
  );
}
