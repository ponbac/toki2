import { useMutation, useQueryClient } from "@tanstack/react-query";

import { api } from "../api";
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
  previousBoard?: BoardResponse;
};

function normalizeColumnName(name: string) {
  return name.trim().toLowerCase();
}

function resolveColumnIdForItem(
  item: BoardWorkItem,
  knownColumnIds: Set<string>,
  columnIdsByName: Map<string, string>,
) {
  if (item.boardColumnId && knownColumnIds.has(item.boardColumnId)) {
    return item.boardColumnId;
  }

  if (item.boardColumnName) {
    const byName = columnIdsByName.get(normalizeColumnName(item.boardColumnName));
    if (byName) {
      return byName;
    }
  }

  const fallbackIdByState: Record<BoardWorkItem["boardState"], string> = {
    todo: "todo",
    inProgress: "inProgress",
    done: "done",
  };

  const fallbackColumnId = fallbackIdByState[item.boardState];
  if (knownColumnIds.has(fallbackColumnId)) {
    return fallbackColumnId;
  }

  return undefined;
}

function toOptimisticBoardState(columnName: string): BoardWorkItem["boardState"] {
  const normalized = normalizeColumnName(columnName);
  if (
    normalized === "new" ||
    normalized === "proposed" ||
    normalized === "to do" ||
    normalized === "approved" ||
    normalized === "ready for development"
  ) {
    return "todo";
  }

  if (
    normalized === "done" ||
    normalized === "closed" ||
    normalized === "completed" ||
    normalized === "removed"
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

    return a.id.localeCompare(b.id);
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
        previousBoard,
      };
      await options?.onMutate?.(vars);
      return context;
    },
    onError: (err, vars, ctx) => {
      if (ctx?.previousBoard) {
        queryClient.setQueryData(ctx.boardQueryKey, ctx.previousBoard);
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
