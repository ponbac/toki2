import { AzureAvatar } from "@/components/azure-avatar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import { CopyWorkItem } from "./copy-work-item";
import { GitPullRequest, GitBranch, Check, ExternalLink } from "lucide-react";
import { useState, useCallback } from "react";

const CATEGORY_COLORS: Record<string, { bg: string; text: string }> = {
  userStory: { bg: "bg-blue-500/15", text: "text-blue-400" },
  bug: { bg: "bg-red-500/15", text: "text-red-400" },
  task: { bg: "bg-yellow-500/15", text: "text-yellow-400" },
  feature: { bg: "bg-purple-500/15", text: "text-purple-400" },
  epic: { bg: "bg-orange-500/15", text: "text-orange-400" },
};

const CATEGORY_LABELS: Record<string, string> = {
  userStory: "Story",
  bug: "Bug",
  task: "Task",
  feature: "Feature",
  epic: "Epic",
};

const PRIORITY_INDICATORS: Record<number, { label: string; className: string }> = {
  1: { label: "P1", className: "text-red-400 font-semibold" },
  2: { label: "P2", className: "text-orange-400 font-medium" },
  3: { label: "P3", className: "text-yellow-400" },
  4: { label: "P4", className: "text-muted-foreground" },
};

function slugify(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 50);
}

export function BoardCard({
  item,
  organization,
  project,
}: {
  item: BoardWorkItem;
  organization: string;
  project: string;
}) {
  const colors = CATEGORY_COLORS[item.category] ?? {
    bg: "bg-muted",
    text: "text-muted-foreground",
  };
  const categoryLabel = CATEGORY_LABELS[item.category] ?? item.category;
  const priority = item.priority != null ? PRIORITY_INDICATORS[item.priority] : null;
  const isPriorityOne = item.priority === 1;
  const branchName = `feature/${item.id}-${slugify(item.title)}`;

  const [branchCopied, setBranchCopied] = useState(false);
  const handleCopyBranch = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      await navigator.clipboard.writeText(branchName);
      setBranchCopied(true);
      setTimeout(() => setBranchCopied(false), 2000);
    },
    [branchName],
  );

  return (
    <div
      className={cn(
        "group relative rounded-lg border border-border/50 bg-card/80 p-3 transition-all duration-200 hover:border-border hover:bg-card hover:shadow-sm",
        isPriorityOne && "bg-red-500/[0.06]",
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
          <Tooltip>
            <TooltipTrigger asChild>
              <a
                href={item.pullRequests[0].url}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => e.stopPropagation()}
                className="inline-flex items-center text-green-500 hover:text-green-400"
              >
                <GitPullRequest className="h-3.5 w-3.5" />
              </a>
            </TooltipTrigger>
            <TooltipContent>
              {item.pullRequests.length === 1
                ? `PR #${item.pullRequests[0].id}`
                : `${item.pullRequests.length} PRs linked`}
            </TooltipContent>
          </Tooltip>
        )}

        <div className="ml-auto flex items-center gap-0.5">
          {/* Branch copy - shown on hover */}
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
                    id: item.assignedTo.uniqueName ?? item.assignedTo.displayName,
                    displayName: item.assignedTo.displayName,
                    uniqueName: item.assignedTo.uniqueName ?? item.assignedTo.displayName,
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
