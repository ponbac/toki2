import { ScrollArea } from "@/components/ui/scroll-area";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import { BoardCard } from "./board-card";

export function BoardColumn({
  title,
  items,
  organization,
  project,
}: {
  title: string;
  items: BoardWorkItem[];
  organization: string;
  project: string;
}) {
  return (
    <div className="flex min-w-0 flex-1 flex-col rounded-xl border border-border/30 bg-muted/20">
      {/* Column header */}
      <div className="flex items-center gap-2 border-b border-border/30 px-4 py-3">
        <h3 className="text-sm font-semibold">{title}</h3>
        <span className="flex h-5 min-w-5 items-center justify-center rounded-full bg-muted px-1.5 text-xs font-medium text-muted-foreground">
          {items.length}
        </span>
      </div>

      {/* Card list */}
      <ScrollArea className="flex-1">
        <div className="flex flex-col gap-2 p-3">
          {items.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              No items
            </p>
          ) : (
            items.map((item) => (
              <BoardCard
                key={item.id}
                item={item}
                organization={organization}
                project={project}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
