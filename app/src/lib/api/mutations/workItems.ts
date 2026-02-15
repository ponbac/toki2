import { useMutation, useQueryClient } from "@tanstack/react-query";

import { api } from "../api";
import {
  normalizeColumnName,
  resolveColumnIdForItem,
} from "@/lib/board-columns";
import {
  type BoardColumn,
  type BoardResponse,
  type BoardWorkItem,
  workItemsQueries,
} from "../queries/workItems";
import type { DefaultMutationOptions } from "./mutations";

export type MoveBoardItemPayload = {
  organization: string;
  project: string;
  workItemId: string;
  targetColumnName: string;
  iterationPath?: string;
  team?: string;
};

export const workItemsMutations = {
  useMoveBoardItem,
};

type MoveBoardItemMutationContext = {
  boardQueryKey: readonly unknown[];
  previousItem?: BoardWorkItem;
};

function toOptimisticBoardState(columnName: string): BoardWorkItem["boardState"] {
  if (
    columnName === "New" ||
    columnName === "Proposed" ||
    columnName === "To Do" ||
    columnName === "Approved" ||
    columnName === "Ready for development"
  ) {
    return "todo";
  }

  if (
    columnName === "Done" ||
    columnName === "Closed" ||
    columnName === "Completed" ||
    columnName === "Removed"
  ) {
    return "done";
  }

  return "inProgress";
}

function sortItemsByColumnAndPriority(
  items: BoardWorkItem[],
  columns: BoardColumn[],
): BoardWorkItem[] {
  const orderedColumns = [...columns].sort(
    (a, b) => a.order - b.order || a.name.localeCompare(b.name),
  );
  const knownColumnIds = new Set(orderedColumns.map((column) => column.id));
  const columnIdsByName = new Map(
    orderedColumns.map((column) => [normalizeColumnName(column.name), column.id]),
  );
  const columnRank = new Map(
    orderedColumns.map((column, index) => [column.id, index]),
  );

  return [...items].sort((a, b) => {
    const rankA = resolveColumnIdForItem(a, knownColumnIds, columnIdsByName);
    const rankB = resolveColumnIdForItem(b, knownColumnIds, columnIdsByName);
    const rankValueA = rankA ? (columnRank.get(rankA) ?? Number.MAX_SAFE_INTEGER) : Number.MAX_SAFE_INTEGER;
    const rankValueB = rankB ? (columnRank.get(rankB) ?? Number.MAX_SAFE_INTEGER) : Number.MAX_SAFE_INTEGER;

    if (rankValueA !== rankValueB) {
      return rankValueA - rankValueB;
    }

    const pa = a.priority;
    const pb = b.priority;
    if (pa != null && pb != null && pa !== pb) {
      return pa - pb;
    }
    if (pa != null && pb == null) {
      return -1;
    }
    if (pa == null && pb != null) {
      return 1;
    }

    if (a.id < b.id) {
      return -1;
    }
    if (a.id > b.id) {
      return 1;
    }
    return 0;
  });
}

function useMoveBoardItem(
  options?: DefaultMutationOptions<
    MoveBoardItemPayload,
    Response,
    unknown,
    MoveBoardItemMutationContext
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["work-items", "move"],
    mutationFn: (body: MoveBoardItemPayload) =>
      api.post("work-items/move", {
        json: body,
      }),
    ...options,
    onMutate: async (vars) => {
      const boardQueryKey = workItemsQueries
        .board({
          organization: vars.organization,
          project: vars.project,
          iterationPath: vars.iterationPath,
          team: vars.team,
        })
        .queryKey;

      await queryClient.cancelQueries({ queryKey: boardQueryKey });
      const previousBoard = queryClient.getQueryData<BoardResponse>(boardQueryKey);
      const previousItem = previousBoard?.items.find(
        (item) => item.id === vars.workItemId,
      );

      if (previousBoard) {
        const targetColumn = previousBoard.columns.find(
          (column) =>
            normalizeColumnName(column.name) ===
            normalizeColumnName(vars.targetColumnName),
        );

        if (targetColumn) {
          const updatedItems = previousBoard.items.map((item) =>
            item.id !== vars.workItemId
              ? item
              : {
                  ...item,
                  boardColumnId: targetColumn.id,
                  boardColumnName: targetColumn.name,
                  boardState: toOptimisticBoardState(targetColumn.name),
                },
          );

          queryClient.setQueryData<BoardResponse>(boardQueryKey, {
            ...previousBoard,
            items: sortItemsByColumnAndPriority(
              updatedItems,
              previousBoard.columns,
            ),
          });
        }
      }

      const context: MoveBoardItemMutationContext = {
        boardQueryKey,
        previousItem,
      };
      await options?.onMutate?.(vars);
      return context;
    },
    onError: (err, vars, ctx) => {
      if (ctx?.previousItem) {
        const board = queryClient.getQueryData<BoardResponse>(ctx.boardQueryKey);
        if (board) {
          const restoredItems = board.items.map((item) =>
            item.id === ctx.previousItem?.id ? ctx.previousItem : item,
          );

          queryClient.setQueryData<BoardResponse>(ctx.boardQueryKey, {
            ...board,
            items: sortItemsByColumnAndPriority(restoredItems, board.columns),
          });
        }
      }
      options?.onError?.(err, vars, ctx);
    },
    onSuccess: (data, vars, ctx) => {
      options?.onSuccess?.(data, vars, ctx);
    },
    onSettled: (data, err, vars, ctx) => {
      queryClient.invalidateQueries({
        queryKey: ctx?.boardQueryKey ?? workItemsQueries
          .board({
            organization: vars.organization,
            project: vars.project,
            iterationPath: vars.iterationPath,
            team: vars.team,
          })
          .queryKey,
      });
      options?.onSettled?.(data, err, vars, ctx);
    },
  });
}
