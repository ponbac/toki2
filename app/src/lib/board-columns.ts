import type { BoardWorkItem } from "@/lib/api/queries/workItems";

const FALLBACK_COLUMN_ID_BY_STATE: Record<BoardWorkItem["boardState"], string> = {
  todo: "todo",
  inProgress: "inProgress",
  done: "done",
};

export function normalizeColumnName(name: string) {
  return name.trim().replace(/[A-Z]/g, (character) => character.toLowerCase());
}

export function resolveColumnIdForItem(
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

  const fallbackColumnId = FALLBACK_COLUMN_ID_BY_STATE[item.boardState];
  if (knownColumnIds.has(fallbackColumnId)) {
    return fallbackColumnId;
  }

  return undefined;
}
