import { useSuspenseQuery } from "@tanstack/react-query";
import { queries } from "@/lib/api/queries/queries";
import { mutations } from "@/lib/api/mutations/mutations";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import {
  normalizeColumnName,
  resolveColumnIdForItem,
} from "@/lib/board-columns";
import { BoardColumn } from "./board-column";
import { BoardFilters } from "./board-filters";
import { useCallback, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  boardColumnScopeKey,
  hiddenColumnsByScopeAtom,
  memberFilterAtom,
  categoryFilterAtom,
} from "../-lib/board-preferences";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

type DragState = {
  itemId: string;
  sourceColumnId: string;
};

const TOGGLEABLE_CATEGORIES = new Set([
  "userStory",
  "bug",
  "task",
  "feature",
  "epic",
]);

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
  const { data: board } = useSuspenseQuery(
    queries.board({
      organization,
      project,
      iterationPath,
      team,
    }),
  );
  const { data: user } = useSuspenseQuery(queries.me());
  const { mutateAsync: moveBoardItem } = mutations.useMoveBoardItem();
  const memberFilter = useAtomValue(memberFilterAtom);
  const categoryFilter = useAtomValue(categoryFilterAtom);
  const [movingItemIds, setMovingItemIds] = useState<string[]>([]);
  const movingItemIdsRef = useRef(new Set<string>());
  const [dragState, setDragState] = useState<DragState | null>(null);
  const [dragOverColumnId, setDragOverColumnId] = useState<string | null>(null);
  const [hiddenColumnsByScope, setHiddenColumnsByScope] = useAtom(
    hiddenColumnsByScopeAtom,
  );
  const columnScope = useMemo(
    () => boardColumnScopeKey({ organization, project, team }),
    [organization, project, team],
  );
  const hiddenColumnIds = useMemo(
    () => hiddenColumnsByScope[columnScope] ?? [],
    [hiddenColumnsByScope, columnScope],
  );
  const hiddenColumnIdSet = useMemo(
    () => new Set(hiddenColumnIds),
    [hiddenColumnIds],
  );
  const movingItemIdSet = useMemo(
    () => new Set(movingItemIds),
    [movingItemIds],
  );

  // Extract unique assignees for the member multi-select
  const members = useMemo(() => {
    const seen = new Map<string, string>();
    for (const item of board.items) {
      if (item.assignedTo?.uniqueName) {
        seen.set(item.assignedTo.uniqueName, item.assignedTo.displayName);
      }
    }
    return Array.from(seen.entries())
      .map(([email, displayName]) => ({ email, displayName }))
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
  }, [board.items]);

  // Apply filters
  const filteredItems = useMemo(() => {
    return board.items.filter((item) => {
      // Category filter
      const categoryHasToggle = TOGGLEABLE_CATEGORIES.has(item.category);
      if (categoryHasToggle && !categoryFilter.includes(item.category)) return false;

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
  }, [board.items, categoryFilter, memberFilter, user.email]);

  const columnsWithItems = useMemo(() => {
    const columns = [...board.columns].sort(
      (a, b) => a.order - b.order || a.name.localeCompare(b.name),
    );
    const knownColumnIds = new Set(columns.map((column) => column.id));
    const columnIdsByName = new Map(
      columns.map((column) => [normalizeColumnName(column.name), column.id]),
    );
    const itemsByColumn = new Map<string, BoardWorkItem[]>(
      columns.map((column) => [column.id, []]),
    );

    for (const item of filteredItems) {
      const columnId = resolveColumnIdForItem(
        item,
        knownColumnIds,
        columnIdsByName,
      );

      if (!columnId) {
        continue;
      }

      itemsByColumn.get(columnId)?.push(item);
    }

    return columns.map((column) => ({
      ...column,
      items: itemsByColumn.get(column.id) ?? [],
    }));
  }, [board.columns, filteredItems]);
  const allColumns = useMemo(
    () => columnsWithItems.map((column) => ({ id: column.id, name: column.name })),
    [columnsWithItems],
  );
  const allColumnsRef = useRef(allColumns);
  allColumnsRef.current = allColumns;

  const visibleColumns = useMemo(
    () =>
      columnsWithItems.filter((column) => !hiddenColumnIdSet.has(column.id)),
    [columnsWithItems, hiddenColumnIdSet],
  );

  const toggleColumnVisibility = useCallback(
    (columnId: string) => {
      setHiddenColumnsByScope((prev) => {
        const current = new Set(prev[columnScope] ?? []);
        if (current.has(columnId)) {
          current.delete(columnId);
        } else {
          current.add(columnId);
        }

        if (current.size === 0) {
          const next = { ...prev };
          delete next[columnScope];
          return next;
        }

        return { ...prev, [columnScope]: Array.from(current) };
      });
    },
    [columnScope, setHiddenColumnsByScope],
  );

  const showAllColumns = useCallback(() => {
    setHiddenColumnsByScope((prev) => {
      if (!(columnScope in prev)) {
        return prev;
      }

      const next = { ...prev };
      delete next[columnScope];
      return next;
    });
  }, [columnScope, setHiddenColumnsByScope]);

  const moveItem = useCallback(
    async (itemId: string, sourceColumnId: string, targetColumnId: string) => {
      if (sourceColumnId === targetColumnId) {
        return;
      }

      if (movingItemIdsRef.current.has(itemId)) {
        return;
      }

      const targetColumn = allColumnsRef.current.find(
        (column) => column.id === targetColumnId,
      );
      if (!targetColumn) {
        return;
      }

      movingItemIdsRef.current.add(itemId);
      setMovingItemIds((prev) =>
        prev.includes(itemId) ? prev : [...prev, itemId],
      );

      try {
        await moveBoardItem({
          organization,
          project,
          workItemId: itemId,
          targetColumnName: targetColumn.name,
          iterationPath,
          team,
        });
      } catch {
        toast.error("Failed to move work item.");
      } finally {
        movingItemIdsRef.current.delete(itemId);
        setMovingItemIds((prev) => prev.filter((id) => id !== itemId));
      }
    },
    [
      iterationPath,
      moveBoardItem,
      organization,
      project,
      team,
    ],
  );

  const handleCardDragStart = useCallback(
    (itemId: string, sourceColumnId: string) => {
      if (movingItemIdsRef.current.has(itemId)) {
        return;
      }
      setDragState({ itemId, sourceColumnId });
      setDragOverColumnId(sourceColumnId);
    },
    [],
  );

  const handleCardDragEnd = useCallback(() => {
    setDragState(null);
    setDragOverColumnId(null);
  }, []);

  const handleColumnDragOver = useCallback((columnId: string) => {
    setDragOverColumnId(columnId);
  }, []);

  const handleColumnDrop = useCallback(
    (columnId: string) => {
      if (!dragState) {
        return;
      }

      void moveItem(dragState.itemId, dragState.sourceColumnId, columnId);
      setDragState(null);
      setDragOverColumnId(null);
    },
    [dragState, moveItem],
  );

  const handleMoveItem = useCallback(
    (itemId: string, sourceColumnId: string, targetColumnId: string) => {
      void moveItem(itemId, sourceColumnId, targetColumnId);
    },
    [moveItem],
  );

  return (
    <div className="flex w-full flex-col gap-3">
      <div className="mx-auto w-full max-w-[110rem] md:w-[95%]">
        <BoardFilters
          members={members}
          columns={columnsWithItems.map((column) => ({
            id: column.id,
            name: column.name,
            count: column.items.length,
          }))}
          hiddenColumnIds={hiddenColumnIds}
          onToggleColumn={toggleColumnVisibility}
          onShowAllColumns={showAllColumns}
        />
      </div>
      <div className="h-[calc(100vh-15rem)] w-full">
        {visibleColumns.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 rounded-xl border border-border/40 bg-muted/20">
            <p className="text-sm text-muted-foreground">
              All columns are hidden.
            </p>
            <Button size="sm" onClick={showAllColumns}>
              Show all columns
            </Button>
          </div>
        ) : (
          <div className="flex h-full w-full gap-4 overflow-x-auto pb-2">
            {visibleColumns.map((column) => (
              <div key={column.id} className="h-full min-h-0 min-w-[20rem] flex-1">
                <BoardColumn
                  columnId={column.id}
                  title={column.name}
                  items={column.items}
                  allColumns={allColumns}
                  organization={organization}
                  project={project}
                  isDropTarget={
                    dragState !== null && dragOverColumnId === column.id
                  }
                  draggingItemId={dragState?.itemId ?? null}
                  movingItemIdSet={movingItemIdSet}
                  onCardDragStart={handleCardDragStart}
                  onCardDragEnd={handleCardDragEnd}
                  onColumnDragOver={handleColumnDragOver}
                  onColumnDrop={handleColumnDrop}
                  onMoveItem={handleMoveItem}
                />
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
