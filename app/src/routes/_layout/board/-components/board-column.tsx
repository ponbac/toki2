import { ScrollArea } from "@/components/ui/scroll-area";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import { cn } from "@/lib/utils";
import { BoardCard } from "./board-card";

export function BoardColumn({
  columnId,
  title,
  items,
  allColumns,
  organization,
  project,
  isDropTarget,
  draggingItemId,
  movingItemIdSet,
  onCardDragStart,
  onCardDragEnd,
  onColumnDragOver,
  onColumnDrop,
  onMoveItem,
}: {
  columnId: string;
  title: string;
  items: BoardWorkItem[];
  allColumns: { id: string; name: string }[];
  organization: string;
  project: string;
  isDropTarget: boolean;
  draggingItemId: string | null;
  movingItemIdSet: Set<string>;
  onCardDragStart: (itemId: string, sourceColumnId: string) => void;
  onCardDragEnd: () => void;
  onColumnDragOver: (columnId: string) => void;
  onColumnDrop: (columnId: string) => void;
  onMoveItem: (
    itemId: string,
    sourceColumnId: string,
    targetColumnId: string,
  ) => void;
}) {
  const isDragging = draggingItemId !== null;

  return (
    <div
      className={cn(
        "flex h-full min-h-0 min-w-0 flex-1 flex-col rounded-xl border border-border/30 bg-muted/20 transition-colors",
        isDropTarget && "border-primary/60 bg-primary/5",
      )}
      onDragEnter={(event) => {
        if (!isDragging) {
          return;
        }
        event.preventDefault();
        onColumnDragOver(columnId);
      }}
      onDragOver={(event) => {
        if (!isDragging) {
          return;
        }
        event.preventDefault();
        onColumnDragOver(columnId);
      }}
      onDrop={(event) => {
        if (!isDragging) {
          return;
        }
        event.preventDefault();
        onColumnDrop(columnId);
      }}
    >
      {/* Column header */}
      <div className="flex items-center gap-2 border-b border-border/30 px-4 py-3">
        <h3 className="text-sm font-semibold">{title}</h3>
        <span className="flex h-5 min-w-5 items-center justify-center rounded-full bg-muted px-1.5 text-xs font-medium text-muted-foreground">
          {items.length}
        </span>
      </div>

      {/* Card list */}
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex min-h-full flex-col gap-2 p-3">
          {items.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              No items
            </p>
          ) : (
            items.map((item) => (
              <BoardCard
                key={item.id}
                item={item}
                columnId={columnId}
                columns={allColumns}
                organization={organization}
                project={project}
                isMoving={movingItemIdSet.has(item.id)}
                isDragging={draggingItemId === item.id}
                onDragStart={onCardDragStart}
                onDragEnd={onCardDragEnd}
                onMoveToColumn={onMoveItem}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
