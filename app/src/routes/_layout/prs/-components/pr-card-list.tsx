import { AzureAvatar } from "@/components/azure-avatar";
import { ListPullRequest } from "@/lib/api/queries/pullRequests";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import { CopySlashIcon, PickaxeIcon } from "lucide-react";

dayjs.extend(relativeTime);

export function PrCardList(props: {
  data: ListPullRequest[];
  onCardClick: (pr: ListPullRequest) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      {props.data.map((pr) => {
        return (
          <button
            type="button"
            key={pr.id}
            onClick={() => props.onCardClick(pr)}
            className="flex flex-col gap-1.5 overflow-hidden rounded-lg border border-border/50 bg-card/50 p-3 text-left transition-colors hover:bg-card active:bg-card/80"
          >
            {/* Row 1: Avatar + Title + Status icons */}
            <div className="flex items-start gap-2">
              <AzureAvatar user={pr.createdBy} className="mt-0.5 size-[26px] shrink-0" />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {pr.title}
              </span>
              <div className="flex shrink-0 items-center gap-1">
                {pr.isDraft && <PickaxeIcon className="size-4 text-blue-400" />}
                {pr.mergeStatus === "conflicts" && (
                  <CopySlashIcon className="size-4 text-red-400" />
                )}
              </div>
            </div>

            {/* Row 2: Repo + Time */}
            <div className="flex items-center justify-between gap-2 pl-8">
              <span className="truncate text-xs text-muted-foreground">
                {pr.repoName}
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">
                {dayjs(pr.createdAt).fromNow()}
              </span>
            </div>

            {/* Row 3: Reviewer votes (if any) */}
            {(pr.approvedBy.length > 0 || pr.blockedBy.length > 0) && (
              <div className="flex items-center gap-1 pl-8">
                {pr.approvedBy.map((r) => (
                  <AzureAvatar
                    key={r.identity.id}
                    user={r.identity}
                    className="size-[22px] border-2 border-green-600"
                  />
                ))}
                {pr.blockedBy.map((r) => (
                  <AzureAvatar
                    key={r.identity.id}
                    user={r.identity}
                    className="size-[22px] border-2 border-red-600"
                  />
                ))}
              </div>
            )}
          </button>
        );
      })}
    </div>
  );
}
