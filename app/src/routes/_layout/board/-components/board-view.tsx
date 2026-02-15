import { useSuspenseQuery } from "@tanstack/react-query";
import { queries } from "@/lib/api/queries/queries";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import { BoardColumn } from "./board-column";
import { BoardFilters } from "./board-filters";
import { useMemo } from "react";
import { useAtomValue } from "jotai";
import {
  memberFilterAtom,
  categoryFilterAtom,
} from "../-lib/board-preferences";

export function BoardView({
  organization,
  project,
  iterationPath,
  team,
}: {
  organization: string;
  project: string;
  iterationPath?: string;
  team?: string;
}) {
  const { data: items } = useSuspenseQuery(
    queries.board({
      organization,
      project,
      iterationPath,
      team,
    }),
  );
  const { data: user } = useSuspenseQuery(queries.me());
  const memberFilter = useAtomValue(memberFilterAtom);
  const categoryFilter = useAtomValue(categoryFilterAtom);

  // Extract unique assignees for the member multi-select
  const members = useMemo(() => {
    const seen = new Map<string, string>();
    for (const item of items) {
      if (item.assignedTo?.uniqueName) {
        seen.set(item.assignedTo.uniqueName, item.assignedTo.displayName);
      }
    }
    return Array.from(seen.entries())
      .map(([email, displayName]) => ({ email, displayName }))
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
  }, [items]);

  // Apply filters
  const filteredItems = useMemo(() => {
    return items.filter((item) => {
      // Category filter
      if (!categoryFilter.includes(item.category)) return false;

      // Member filter
      if (memberFilter.mode === "mine") {
        const assigneeEmail = item.assignedTo?.uniqueName?.toLowerCase();
        if (assigneeEmail !== user.email.toLowerCase()) return false;
      } else if (memberFilter.mode === "custom") {
        if (memberFilter.selectedEmails.length > 0) {
          const assigneeEmail = item.assignedTo?.uniqueName;
          if (!assigneeEmail || !memberFilter.selectedEmails.includes(assigneeEmail))
            return false;
        }
      }

      return true;
    });
  }, [items, categoryFilter, memberFilter, user.email]);

  const columns = useMemo(() => {
    const todo: BoardWorkItem[] = [];
    const inProgress: BoardWorkItem[] = [];
    const done: BoardWorkItem[] = [];

    for (const item of filteredItems) {
      switch (item.boardState) {
        case "todo":
          todo.push(item);
          break;
        case "inProgress":
          inProgress.push(item);
          break;
        case "done":
          done.push(item);
          break;
      }
    }

    return { todo, inProgress, done };
  }, [filteredItems]);

  return (
    <div className="flex flex-col gap-3">
      <BoardFilters members={members} />
      <div className="grid h-[calc(100vh-15rem)] grid-cols-1 gap-4 md:grid-cols-3">
        <BoardColumn title="To Do" items={columns.todo} organization={organization} project={project} />
        <BoardColumn title="In Progress" items={columns.inProgress} organization={organization} project={project} />
        <BoardColumn title="Done" items={columns.done} organization={organization} project={project} />
      </div>
    </div>
  );
}
