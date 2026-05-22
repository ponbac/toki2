import {
  keepPreviousData,
  useQuery,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { queries } from "@/lib/api/queries/queries";
import { mutations } from "@/lib/api/mutations/mutations";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import type { AgentRunIssueSummary } from "@/lib/api/queries/agentRuns";
import {
  normalizeColumnName,
  resolveColumnIdForItem,
} from "@/lib/board-columns";
import {
  buildWorkItemTimeReportText,
  type TimeReportMode,
} from "@/lib/time-report";
import { copyAndSyncTimeReport } from "@/lib/time-report-actions";
import { BoardColumn } from "./board-column";
import { BoardFilters } from "./board-filters";
import { AgentRunDrawer } from "./agent-run-drawer";
import {
  LaunchAgentDialog,
  type AgentLaunchTargetRepo,
} from "./launch-agent-dialog";
import {
  useCallback,
  useMemo,
  useRef,
  useState,
  type SetStateAction,
} from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  DEFAULT_MEMBER_FILTER,
  boardColumnScopeKey,
  boardProjectScopeKey,
  hiddenColumnsByScopeAtom,
  memberFilterByScopeAtom,
  categoryFilterAtom,
  type MemberFilter,
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
  "other",
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
  const {
    data: board,
    isError: isBoardError,
    refetch: refetchBoard,
  } = useQuery({
    ...queries.board({
      organization,
      project,
      iterationPath,
      team,
    }),
    placeholderData: keepPreviousData,
  });
  const { data: user } = useSuspenseQuery(queries.me());
  const { data: repositories = [] } = useQuery({
    ...queries.differs(),
    staleTime: 30_000,
  });
  const { mutateAsync: moveBoardItem } = mutations.useMoveBoardItem();
  const { mutateAsync: createAgentRun, isPending: isLaunchingAgent } =
    mutations.useCreateAgentRun();
  const { data: timerResponse, isSuccess: timerQuerySuccess } = useQuery({
    ...timeTrackingQueries.getTimer(),
    retry: false,
  });
  const timer = timerResponse?.timer;
  const { mutateAsync: startTimer } = timeTrackingMutations.useStartTimer();
  const { mutateAsync: editTimer } = timeTrackingMutations.useEditTimer();
  const [memberFilterByScope, setMemberFilterByScope] = useAtom(
    memberFilterByScopeAtom,
  );
  const memberFilterScope = useMemo(
    () => boardProjectScopeKey({ organization, project }),
    [organization, project],
  );
  const memberFilter = useMemo(
    () => memberFilterByScope[memberFilterScope] ?? DEFAULT_MEMBER_FILTER,
    [memberFilterByScope, memberFilterScope],
  );
  const setMemberFilter = useCallback(
    (update: SetStateAction<MemberFilter>) => {
      setMemberFilterByScope((prev) => {
        const current = prev[memberFilterScope] ?? DEFAULT_MEMBER_FILTER;
        const next =
          typeof update === "function"
            ? (update as (value: MemberFilter) => MemberFilter)(current)
            : update;

        if (
          next.mode === DEFAULT_MEMBER_FILTER.mode &&
          next.selectedEmails.length === 0
        ) {
          if (!(memberFilterScope in prev)) {
            return prev;
          }
          const rest = { ...prev };
          delete rest[memberFilterScope];
          return rest;
        }

        return { ...prev, [memberFilterScope]: next };
      });
    },
    [memberFilterScope, setMemberFilterByScope],
  );
  const categoryFilter = useAtomValue(categoryFilterAtom);
  const movingItemIdsRef = useRef(new Set<string>());
  const [, setMovingItemVersion] = useState(0);
  const [dragState, setDragState] = useState<DragState | null>(null);
  const [dragOverColumnId, setDragOverColumnId] = useState<string | null>(null);
  const [agentLaunchItem, setAgentLaunchItem] = useState<BoardWorkItem | null>(
    null,
  );
  const [activeAgentRunId, setActiveAgentRunId] = useState<string | null>(null);
  const [isAgentDrawerOpen, setIsAgentDrawerOpen] = useState(false);
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
  const selectedMemberEmailSet = useMemo(
    () =>
      new Set(memberFilter.selectedEmails.map((email) => email.toLowerCase())),
    [memberFilter.selectedEmails],
  );
  const boardItems = useMemo(() => board?.items ?? [], [board]);
  const boardColumns = useMemo(() => board?.columns ?? [], [board]);

  // Extract unique assignees for the member multi-select
  const members = useMemo(() => {
    const seen = new Map<string, string>();
    for (const item of boardItems) {
      if (item.assignedTo?.uniqueName) {
        seen.set(item.assignedTo.uniqueName, item.assignedTo.displayName);
      }
    }
    return Array.from(seen.entries())
      .map(([email, displayName]) => ({ email, displayName }))
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
  }, [boardItems]);

  // Apply filters
  const filteredItems = useMemo(() => {
    return boardItems.filter((item) => {
      // Category filter
      const normalizedCategory = TOGGLEABLE_CATEGORIES.has(item.category)
        ? item.category
        : "other";
      if (!categoryFilter.includes(normalizedCategory)) return false;

      // Member filter
      if (memberFilter.mode === "mine") {
        const assigneeEmail = item.assignedTo?.uniqueName?.toLowerCase();
        if (assigneeEmail !== user.email.toLowerCase()) return false;
      } else if (memberFilter.mode === "custom") {
        if (memberFilter.selectedEmails.length > 0) {
          const assigneeEmail = item.assignedTo?.uniqueName?.toLowerCase();
          if (!assigneeEmail || !selectedMemberEmailSet.has(assigneeEmail))
            return false;
        }
      }

      return true;
    });
  }, [
    boardItems,
    categoryFilter,
    memberFilter,
    selectedMemberEmailSet,
    user.email,
  ]);

  const columnsWithItems = useMemo(() => {
    const columns = [...boardColumns].sort(
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
  }, [boardColumns, filteredItems]);
  const visibleColumns = useMemo(
    () =>
      columnsWithItems.filter((column) => !hiddenColumnIdSet.has(column.id)),
    [columnsWithItems, hiddenColumnIdSet],
  );
  const visibleWorkItemIds = useMemo(
    () =>
      Array.from(
        new Set(
          visibleColumns.flatMap((column) =>
            column.items.map((item) => item.id),
          ),
        ),
      ),
    [visibleColumns],
  );
  const { data: latestAgentRunsResponse } = useQuery({
    ...queries.latestByWorkItems({
      sourceProvider: "azureDevOpsWorkItem",
      organization,
      project,
      workItemIds: visibleWorkItemIds,
    }),
    retry: false,
  });
  const latestRunByWorkItemId = useMemo(() => {
    const runs = latestAgentRunsResponse?.runs ?? [];
    return new Map<string, AgentRunIssueSummary>(
      runs.map((run) => [run.workItemId, run]),
    );
  }, [latestAgentRunsResponse]);
  const allColumns = useMemo(
    () =>
      columnsWithItems.map((column) => ({ id: column.id, name: column.name })),
    [columnsWithItems],
  );
  const allColumnsRef = useRef(allColumns);
  allColumnsRef.current = allColumns;

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
      setMovingItemVersion((version) => version + 1);

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
        if (movingItemIdsRef.current.delete(itemId)) {
          setMovingItemVersion((version) => version + 1);
        }
      }
    },
    [iterationPath, moveBoardItem, organization, project, team],
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
  const handleTimerAction = useCallback(
    async (item: BoardWorkItem, mode: TimeReportMode) => {
      const text = buildWorkItemTimeReportText({
        workItemId: item.id,
        title: item.title,
        parentWorkItemId: item.parent?.id ?? null,
        mode,
      });

      await copyAndSyncTimeReport({
        text,
        timer,
        timerQuerySuccess,
        startTimer,
        editTimer,
        onTimerSyncError: () => {
          console.warn("Failed to synchronize timer state from board action.");
        },
      });
    },
    [editTimer, startTimer, timer, timerQuerySuccess],
  );
  const handleLaunchAgent = useCallback((item: BoardWorkItem) => {
    setAgentLaunchItem(item);
  }, []);
  const handleOpenAgentRun = useCallback((runId: string) => {
    setActiveAgentRunId(runId);
    setIsAgentDrawerOpen(true);
  }, []);
  const handleConfirmAgentLaunch = useCallback(
    async ({
      targetRepo,
      prompt,
    }: {
      targetRepo: AgentLaunchTargetRepo;
      prompt?: string;
    }) => {
      if (!agentLaunchItem) {
        return;
      }

      try {
        const run = await createAgentRun({
          source: {
            id: agentLaunchItem.id,
            title: agentLaunchItem.title,
            url: agentLaunchItem.url,
            organization,
            project,
          },
          targetRepo,
          prompt,
        });
        setAgentLaunchItem(null);
        setActiveAgentRunId(run.id);
        setIsAgentDrawerOpen(true);
        toast.success("Agent run created.");
      } catch {
        toast.error("Failed to launch agent.");
      }
    },
    [agentLaunchItem, createAgentRun, organization, project],
  );

  if (!board) {
    if (isBoardError) {
      return (
        <div className="flex h-[60vh] items-center justify-center">
          <div className="flex max-w-xl flex-col items-center gap-3 text-center">
            <p className="text-sm text-muted-foreground">
              Failed to load board data. Please retry.
            </p>
            <Button size="sm" onClick={() => void refetchBoard()}>
              Retry board load
            </Button>
          </div>
        </div>
      );
    }

    return (
      <div className="flex h-[60vh] items-center justify-center">
        <p className="text-sm text-muted-foreground">Loading board...</p>
      </div>
    );
  }

  return (
    <div className="flex w-full min-w-0 flex-col gap-3">
      <BoardFilters
        memberFilter={memberFilter}
        setMemberFilter={setMemberFilter}
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
      <div className="h-[calc(100vh-15rem)] w-full min-w-0">
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
          <div className="flex h-full w-full min-w-0 gap-4 overflow-x-auto pb-2">
            {visibleColumns.map((column) => (
              <div
                key={column.id}
                className="h-full min-h-0 min-w-[20rem] flex-1"
              >
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
                  movingItemIdSet={movingItemIdsRef.current}
                  onCardDragStart={handleCardDragStart}
                  onCardDragEnd={handleCardDragEnd}
                  onColumnDragOver={handleColumnDragOver}
                  onColumnDrop={handleColumnDrop}
                  onMoveItem={handleMoveItem}
                  onTimerAction={handleTimerAction}
                  onLaunchAgent={handleLaunchAgent}
                  latestRunByWorkItemId={latestRunByWorkItemId}
                  onOpenAgentRun={handleOpenAgentRun}
                />
              </div>
            ))}
          </div>
        )}
      </div>
      <LaunchAgentDialog
        item={agentLaunchItem}
        organization={organization}
        project={project}
        repositories={repositories}
        open={agentLaunchItem !== null}
        isLaunching={isLaunchingAgent}
        onOpenChange={(open) => {
          if (!open) {
            setAgentLaunchItem(null);
          }
        }}
        onLaunch={(payload) => {
          void handleConfirmAgentLaunch(payload);
        }}
      />
      <AgentRunDrawer
        runId={activeAgentRunId}
        open={isAgentDrawerOpen}
        onOpenChange={setIsAgentDrawerOpen}
      />
    </div>
  );
}
