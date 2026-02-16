import { AzureAvatar } from "@/components/azure-avatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import { BOARD_CATEGORY_OPTIONS } from "../-lib/category-meta";
import { CopyWorkItem } from "./copy-work-item";
import { PrApprovalHoverCard } from "./pr-approval-hover-card";
import {
  GitBranch,
  Check,
  ExternalLink,
  Loader2,
  MoreVertical,
} from "lucide-react";
import { useState, useCallback } from "react";

const CATEGORY_META_BY_VALUE = Object.fromEntries(
  BOARD_CATEGORY_OPTIONS.map((option) => [option.value, option]),
) as Record<string, (typeof BOARD_CATEGORY_OPTIONS)[number] | undefined>;

const PRIORITY_INDICATORS: Record<
  number,
  { label: string; className: string }
> = {
  1: { label: "P1", className: "text-red-400 font-semibold" },
  2: { label: "P2", className: "text-orange-400 font-medium" },
  3: { label: "P3", className: "text-yellow-400" },
  4: { label: "P4", className: "text-muted-foreground" },
};

function trimBranch(branch: string): string {
  return branch.replace("refs/heads/", "");
}

export function BoardCard({
  item,
  columnId,
  columns,
  organization,
  project,
  isMoving,
  isDragging,
  onDragStart,
  onDragEnd,
  onMoveToColumn,
}: {
  item: BoardWorkItem;
  columnId: string;
  columns: { id: string; name: string }[];
  organization: string;
  project: string;
  isMoving: boolean;
  isDragging: boolean;
  onDragStart: (itemId: string, sourceColumnId: string) => void;
  onDragEnd: () => void;
  onMoveToColumn: (
    itemId: string,
    sourceColumnId: string,
    targetColumnId: string,
  ) => void;
}) {
  const categoryMeta = CATEGORY_META_BY_VALUE[item.category];
  const colors = categoryMeta
    ? { bg: categoryMeta.bg, text: categoryMeta.text }
    : {
        bg: "bg-muted",
        text: "text-muted-foreground",
      };
  const categoryLabel = categoryMeta?.label ?? item.category;
  const priority =
    item.priority != null ? PRIORITY_INDICATORS[item.priority] : null;
  const isPriorityOne = item.priority === 1;
  const sourceBranch = item.pullRequests
    ?.map((pullRequest) => pullRequest.sourceBranch)
    .find((branch): branch is string => !!branch);
  const branchName = sourceBranch ? trimBranch(sourceBranch) : null;

  const [branchCopied, setBranchCopied] = useState(false);
  const handleCopyBranch = useCallback(
    async (e: React.MouseEvent) => {
      if (!branchName) {
        return;
      }
      e.stopPropagation();
      await navigator.clipboard.writeText(branchName);
      setBranchCopied(true);
      setTimeout(() => setBranchCopied(false), 2000);
    },
    [branchName],
  );
  const moveTargets = columns.filter((column) => column.id !== columnId);

  return (
    <div
      draggable={!isMoving}
      onDragStart={(event) => {
        if (isMoving) {
          event.preventDefault();
          return;
        }
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", item.id);
        onDragStart(item.id, columnId);
      }}
      onDragEnd={onDragEnd}
      className={cn(
        "group relative rounded-lg border border-border/50 bg-card/80 p-3 transition-all duration-200 hover:border-border hover:bg-card hover:shadow-sm",
        isPriorityOne && "bg-red-500/[0.06]",
        isDragging && "opacity-50",
        isMoving && "cursor-progress",
      )}
    >
      {isPriorityOne && (
        <div className="pointer-events-none absolute inset-x-3 top-0 h-px bg-gradient-to-r from-transparent via-red-300/90 to-transparent" />
      )}

      {/* Top row: type badge + id + actions */}
      <div className="mb-1.5 flex items-center gap-2">
        <span
          className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${colors.bg} ${colors.text}`}
        >
          {categoryLabel}
        </span>

        {item.tags.length > 0 && (
          <div className="flex items-center gap-1">
            {item.tags.slice(0, 2).map((tag) => (
              <span
                key={tag}
                className="rounded bg-muted/50 px-1.5 py-0.5 text-[10px] text-muted-foreground"
              >
                {tag}
              </span>
            ))}
            {item.tags.length > 2 && (
              <span className="rounded bg-muted/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                +{item.tags.length - 2}
              </span>
            )}
          </div>
        )}

        <a
          href={item.url}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => e.stopPropagation()}
          className="inline-flex items-center gap-0.5 text-xs text-muted-foreground hover:text-foreground"
        >
          #{item.id}
          <ExternalLink className="h-3 w-3 opacity-0 transition-opacity group-hover:opacity-100" />
        </a>

        {/* PR indicator - always visible */}
        {item.pullRequests && item.pullRequests.length > 0 && (
          <PrApprovalHoverCard pullRequests={item.pullRequests} />
        )}

        <div className="ml-auto flex items-center gap-0.5">
          {/* Move menu - shown on hover */}
          <div className="opacity-0 transition-opacity group-hover:opacity-100">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  onClick={(e) => e.stopPropagation()}
                  className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                  disabled={isMoving}
                >
                  {isMoving ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <MoreVertical className="h-3.5 w-3.5" />
                  )}
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Move to...</DropdownMenuLabel>
                <DropdownMenuSeparator />
                {moveTargets.length === 0 ? (
                  <DropdownMenuItem disabled>No other columns</DropdownMenuItem>
                ) : (
                  moveTargets.map((column) => (
                    <DropdownMenuItem
                      key={column.id}
                      onSelect={(event) => {
                        event.preventDefault();
                        onMoveToColumn(item.id, columnId, column.id);
                      }}
                      disabled={isMoving}
                    >
                      {column.name}
                    </DropdownMenuItem>
                  ))
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {/* Branch copy - shown on hover only when PR source branch is available */}
          {branchName && (
            <div className="opacity-0 transition-opacity group-hover:opacity-100">
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    onClick={handleCopyBranch}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                  >
                    {branchCopied ? (
                      <Check className="h-3.5 w-3.5 text-green-500" />
                    ) : (
                      <GitBranch className="h-3.5 w-3.5" />
                    )}
                  </button>
                </TooltipTrigger>
                <TooltipContent>
                  {branchCopied ? "Copied!" : branchName}
                </TooltipContent>
              </Tooltip>
            </div>
          )}

          {/* Copy work item - shown on hover */}
          <div className="opacity-0 transition-opacity group-hover:opacity-100">
            <CopyWorkItem
              workItemId={item.id}
              organization={organization}
              project={project}
            />
          </div>
        </div>
      </div>

      {/* Title */}
      <p className="mb-2 text-sm font-medium leading-snug">{item.title}</p>

      {/* Bottom row: assignee + priority */}
      <div className="flex items-center justify-between">
        {item.assignedTo ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="flex items-center gap-1.5">
                <AzureAvatar
                  disableTooltip
                  className="size-6"
                  user={{
                    id:
                      item.assignedTo.uniqueName ?? item.assignedTo.displayName,
                    displayName: item.assignedTo.displayName,
                    uniqueName:
                      item.assignedTo.uniqueName ?? item.assignedTo.displayName,
                    avatarUrl: item.assignedTo.imageUrl,
                  }}
                />
                <span className="max-w-[120px] truncate text-xs text-muted-foreground">
                  {item.assignedTo.displayName}
                </span>
              </div>
            </TooltipTrigger>
            <TooltipContent>{item.assignedTo.displayName}</TooltipContent>
          </Tooltip>
        ) : (
          <span className="text-xs italic text-muted-foreground/50">
            Unassigned
          </span>
        )}

        {priority && (
          <span
            className={cn(
              "text-xs",
              priority.className,
              isPriorityOne &&
                "rounded-full border border-red-400/50 bg-red-500/10 px-1.5 py-0.5",
            )}
          >
            {priority.label}
          </span>
        )}
      </div>
    </div>
  );
}
