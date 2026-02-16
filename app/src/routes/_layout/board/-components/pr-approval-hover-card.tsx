import { AzureAvatar } from "@/components/azure-avatar";
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import type { PullRequestRef } from "@/lib/api/queries/workItems";
import { cn } from "@/lib/utils";
import {
  AlertCircle,
  CheckCircle2,
  ExternalLink,
  GitPullRequest,
  MinusCircle,
} from "lucide-react";
import { type ReactNode } from "react";

export function PrApprovalHoverCard({
  pullRequests,
}: {
  pullRequests: PullRequestRef[];
}) {
  return (
    <HoverCard openDelay={120} closeDelay={160}>
      <HoverCardTrigger asChild>
        <a
          href={pullRequests[0].url}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(event) => event.stopPropagation()}
          aria-label={`Show pull request approvals for PR ${pullRequests[0].id}`}
          className="inline-flex items-center rounded-md border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 text-emerald-500 transition-colors hover:border-emerald-500/60 hover:bg-emerald-500/15 hover:text-emerald-400"
        >
          <GitPullRequest className="h-3.5 w-3.5" />
        </a>
      </HoverCardTrigger>
      <HoverCardContent
        align="start"
        side="bottom"
        sideOffset={8}
        className="w-[22rem] overflow-hidden rounded-xl border border-emerald-500/20 bg-popover p-0 shadow-xl"
      >
        <div className="border-b border-border/60 bg-gradient-to-r from-emerald-500/10 via-transparent to-amber-500/10 px-3 py-2">
          <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
            <GitPullRequest className="h-3.5 w-3.5 text-emerald-500" />
            Pull Request Approvals
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {pullRequests.length === 1
              ? "1 linked pull request"
              : `${pullRequests.length} linked pull requests`}
          </p>
        </div>

        <div className="max-h-80 overflow-y-auto">
          {pullRequests.map((pullRequest) => {
            const approvalStatus = pullRequest.approvalStatus;
            const approvedCount = approvalStatus?.approvedBy.length ?? 0;
            const blockedCount = approvalStatus?.blockedBy.length ?? 0;
            const hasVotes =
              approvalStatus != null && approvedCount + blockedCount > 0;

            return (
              <article
                key={`${pullRequest.repositoryId}:${pullRequest.id}`}
                className="border-b border-border/60 px-3 py-2.5 last:border-b-0"
              >
                <a
                  href={pullRequest.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={(event) => event.stopPropagation()}
                  className="group/link flex items-start justify-between gap-2 text-left"
                >
                  <div className="min-w-0">
                    <p className="text-xs font-semibold leading-tight text-foreground">
                      PR #{pullRequest.id}
                    </p>
                    <p className="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">
                      {pullRequest.title?.trim() || "Pull request details"}
                    </p>
                  </div>
                  <ExternalLink className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground transition-colors group-hover/link:text-foreground" />
                </a>

                {approvalStatus ? (
                  <>
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      {approvedCount > 0 && (
                        <StatusTag
                          tone="approved"
                          icon={<CheckCircle2 className="h-3.5 w-3.5" />}
                          label={`${approvedCount} approved`}
                        />
                      )}
                      {blockedCount > 0 && (
                        <StatusTag
                          tone="blocked"
                          icon={<AlertCircle className="h-3.5 w-3.5" />}
                          label={`${blockedCount} blocking`}
                        />
                      )}
                    </div>

                    {hasVotes ? (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5">
                        {approvalStatus.approvedBy.map((reviewer) => (
                          <AzureAvatar
                            key={`approved:${pullRequest.id}:${reviewer.id}`}
                            className="size-7 border-2 border-emerald-500/80"
                            user={reviewer}
                          />
                        ))}
                        {approvalStatus.blockedBy.map((reviewer) => (
                          <AzureAvatar
                            key={`blocked:${pullRequest.id}:${reviewer.id}`}
                            className="size-7 border-2 border-red-500/80"
                            user={reviewer}
                          />
                        ))}
                      </div>
                    ) : (
                      <p className="mt-2 inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
                        <MinusCircle className="h-3.5 w-3.5" />
                        No approval or blocking votes yet
                      </p>
                    )}
                  </>
                ) : (
                  <p className="mt-2 inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
                    <AlertCircle className="h-3.5 w-3.5" />
                    Approval status unavailable
                  </p>
                )}
              </article>
            );
          })}
        </div>
      </HoverCardContent>
    </HoverCard>
  );
}

function StatusTag({
  icon,
  label,
  tone,
}: {
  icon: ReactNode;
  label: string;
  tone: "approved" | "blocked";
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
        tone === "approved" && "bg-emerald-500/15 text-emerald-500",
        tone === "blocked" && "bg-red-500/15 text-red-500",
      )}
    >
      {icon}
      {label}
    </span>
  );
}
