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
import type { TimeReportMode } from "@/lib/time-report";
import { BOARD_CATEGORY_OPTIONS } from "../-lib/category-meta";
import { CopyWorkItem } from "./copy-work-item";
import { PrApprovalHoverCard } from "./pr-approval-hover-card";
import { WorkItemDescriptionHoverCard } from "./work-item-description-hover-card";
import {
  Check,
  CodeXmlIcon,
  GitBranch,
  Loader2,
  MessageCircleCodeIcon,
  MoreVertical,
  TimerIcon,
} from "lucide-react";
import { useState, useCallback } from "react";
import { toast } from "sonner";

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
  onTimerAction,
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
  onTimerAction: (item: BoardWorkItem, mode: TimeReportMode) => Promise<void>;
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
  const pullRequests = item.pullRequests ?? [];
  const hasPullRequests = pullRequests.length > 0;
  const branchName =
    pullRequests
      .find((pullRequest) => pullRequest.sourceBranch)
      ?.sourceBranch?.replace("refs/heads/", "") ?? null;

  const [branchCopied, setBranchCopied] = useState(false);
  const [isTimerActionPending, setIsTimerActionPending] = useState(false);
  const handleCopyBranch = useCallback(
    async (e: React.MouseEvent) => {
      if (!branchName) {
        return;
      }
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(branchName);
        setBranchCopied(true);
        setTimeout(() => setBranchCopied(false), 2000);
      } catch {
        toast.error("Failed to copy branch name.");
      }
    },
    [branchName],
  );
  const handleTimerActionSelect = useCallback(
    (mode: TimeReportMode) => {
      if (isTimerActionPending) {
        return;
      }

      setIsTimerActionPending(true);
      void onTimerAction(item, mode).finally(() => {
        setIsTimerActionPending(false);
      });
    },
    [isTimerActionPending, item, onTimerAction],
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
        "group relative min-w-0 overflow-hidden rounded-lg border border-border/50 bg-card/80 p-3 transition-all duration-200 hover:border-border hover:bg-card hover:shadow-sm",
        isPriorityOne && "bg-red-500/[0.06]",
        isDragging && "opacity-50",
        isMoving && "cursor-progress",
      )}
    >
      {isPriorityOne && (
        <div className="pointer-events-none absolute inset-x-3 top-0 h-px bg-gradient-to-r from-transparent via-red-300/90 to-transparent" />
      )}

      {/* Header row: metadata */}
      <div className={cn("mb-1.5 min-h-7", branchName ? "pr-[7.5rem]" : "pr-24")}>
        <div className="flex min-w-0 items-center gap-1.5 overflow-hidden">
          <span
            className={cn(
              "inline-flex max-w-[7.5rem] shrink-0 items-center truncate rounded-md px-1.5 py-0.5 text-[10px] font-medium",
              colors.bg,
              colors.text,
            )}
          >
            {categoryLabel}
          </span>

          {item.tags.length > 0 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <span
                  tabIndex={0}
                  className="inline-flex max-w-8 items-center truncate rounded bg-muted/50 px-1.5 py-0.5 text-[10px] text-muted-foreground outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                >
                  +{item.tags.length}
                </span>
              </TooltipTrigger>
              <TooltipContent className="max-w-[16rem]">
                <div className="space-y-1">
                  {item.tags.map((tag, index) => (
                    <p
                      key={`${tag}-${index}`}
                      className="text-xs leading-tight [overflow-wrap:anywhere]"
                    >
                      - {tag}
                    </p>
                  ))}
                </div>
              </TooltipContent>
            </Tooltip>
          )}

          <div className="ml-0.5 flex shrink-0 items-center gap-1 leading-none">
            <WorkItemDescriptionHoverCard
              id={item.id}
              url={item.url}
              descriptionRenderedHtml={item.descriptionRenderedHtml}
              reproStepsRenderedHtml={item.reproStepsRenderedHtml}
            />

            {/* PR indicator - always visible */}
            {hasPullRequests && (
              <PrApprovalHoverCard pullRequests={pullRequests} />
            )}
          </div>
        </div>
      </div>

      <div className="pointer-events-none absolute right-2 top-2 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        {/* Timer actions */}
        <div className="pointer-events-auto">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                onClick={(event) => event.stopPropagation()}
                className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                disabled={isMoving || isTimerActionPending}
                aria-label="Timer actions"
              >
                {isTimerActionPending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <TimerIcon className="h-3.5 w-3.5" />
                )}
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Timer</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onSelect={() => {
                  handleTimerActionSelect("review");
                }}
                disabled={isMoving || isTimerActionPending}
              >
                <MessageCircleCodeIcon className="mr-2 h-3.5 w-3.5" />
                Review
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() => {
                  handleTimerActionSelect("develop");
                }}
                disabled={isMoving || isTimerActionPending}
              >
                <CodeXmlIcon className="mr-2 h-3.5 w-3.5" />
                Develop
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {/* Move menu - shown on hover */}
        <div className="pointer-events-auto">
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
          <div className="pointer-events-auto">
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
        <div className="pointer-events-auto">
          <CopyWorkItem
            workItemId={item.id}
            organization={organization}
            project={project}
          />
        </div>
      </div>

      {/* Title */}
      <p className="mb-2 min-w-0 break-words text-sm font-medium leading-snug [overflow-wrap:anywhere]">
        {item.title}
      </p>

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
                      item.assignedTo.uniqueName ??
                      `display-name:${item.assignedTo.displayName}`,
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
