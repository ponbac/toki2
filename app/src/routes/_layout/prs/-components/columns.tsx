import { AzureAvatar } from "@/components/azure-avatar";
import { PRLink } from "@/components/pr-link";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ConditionalTooltip } from "@/components/ui/tooltip";
import { WorkItemLink } from "@/components/work-item-link";
import { ListPullRequest } from "@/lib/api/queries/pullRequests";
import { ColumnDef } from "@tanstack/react-table";
import dayjs from "dayjs";
import { CopySlashIcon, PickaxeIcon, UserXIcon, AlertTriangleIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { StatusIcon } from "./status-icon";

export function pullRequestColumns(): ColumnDef<ListPullRequest>[] {
  return [
    {
      accessorKey: "id",
      header: "ID",
      cell: ({ row }) => <PRLink data={row.original} />,
    },
    {
      accessorKey: "repoName",
      header: "Repository",
    },
    {
      accessorKey: "title",
      header: "Title",
      cell: ({ row }) => {
        const title = row.original.title;
        const isTruncated = (length: number) => title.length > length;

        const twoXLLimit = 75;
        const smallLimit = 45;

        return (
          <div className="flex flex-row items-center gap-2">
            <div className="hidden 2xl:block">
              {isTruncated(twoXLLimit) ? (
                <Tooltip>
                  <TooltipTrigger>
                    <span className="text-nowrap">
                      {title.slice(0, twoXLLimit).trimEnd()}...
                    </span>
                  </TooltipTrigger>
                  <TooltipContent>{title}</TooltipContent>
                </Tooltip>
              ) : (
                <span className="text-nowrap">{title}</span>
              )}
            </div>
            <div className="block 2xl:hidden">
              {isTruncated(smallLimit) ? (
                <Tooltip>
                  <TooltipTrigger>
                    <span className="text-nowrap">
                      {title.slice(0, smallLimit).trimEnd()}...
                    </span>
                  </TooltipTrigger>
                  <TooltipContent>{title}</TooltipContent>
                </Tooltip>
              ) : (
                <span className="text-nowrap">{title}</span>
              )}
            </div>
            {row.original.workItems.some((wi) => wi.priority === 1) && (
              <StatusIcon tooltip="Important: Priority 1 work item linked">
                <AlertTriangleIcon className="size-5 text-amber-500" />
              </StatusIcon>
            )}
            {row.original.isDraft && (
              <StatusIcon tooltip="Draft">
                <PickaxeIcon className="size-5 text-blue-400" />
              </StatusIcon>
            )}
            {row.original.mergeStatus === "conflicts" && (
              <StatusIcon tooltip="Merge conflicts">
                <CopySlashIcon className="size-5 text-red-400" />
              </StatusIcon>
            )}
          </div>
        );
      },
    },
    {
      accessorKey: "createdBy.displayName",
      header: "Author",
      cell: ({ row }) => {
        const LIMIT = 25;
        const displayName = row.original.createdBy.displayName;
        const displayText =
          displayName.length > LIMIT
            ? displayName.slice(0, LIMIT) + "..."
            : displayName;

        return (
          <div className="flex flex-row items-center justify-center gap-2 2xl:justify-start">
            <AzureAvatar user={row.original.createdBy} />
            <span className="hidden text-nowrap 2xl:block">
              <ConditionalTooltip
                condition={displayName.length > LIMIT}
                content={displayName}
              >
                {displayText}
              </ConditionalTooltip>
            </span>
          </div>
        );
      },
    },
    {
      accessorKey: "workItems",
      header: "Work Items",
      cell: ({ row }) => {
        const nWorkItemsToShow = 2;

        return (
          <div className="flex flex-row items-center gap-1">
            {row.original.workItems.length < 3 ? (
              row.original.workItems.map((wi) => (
                <WorkItemLink
                  key={wi.id}
                  tooltip={wi.title}
                  data={{
                    ...row.original,
                    id: wi.id,
                  }}
                />
              ))
            ) : (
              // map the first two work items, then ... with tooltip showing the rest
              <>
                {row.original.workItems.slice(0, nWorkItemsToShow).map((wi) => (
                  <WorkItemLink
                    key={wi.id}
                    tooltip={wi.title}
                    data={{
                      ...row.original,
                      id: wi.id,
                    }}
                  />
                ))}
                <Tooltip>
                  <TooltipTrigger>
                    <span className="text-nowrap hover:underline">
                      +{row.original.workItems.length - nWorkItemsToShow}
                    </span>
                  </TooltipTrigger>
                  <TooltipContent>
                    <div className="flex flex-col gap-1">
                      {row.original.workItems
                        .slice(nWorkItemsToShow)
                        .map((wi) => (
                          <WorkItemLink
                            key={wi.id}
                            text={wi.title}
                            data={{
                              ...row.original,
                              id: wi.id,
                            }}
                          />
                        ))}
                    </div>
                  </TooltipContent>
                </Tooltip>
              </>
            )}
          </div>
        );
      },
    },
    {
      accessorKey: "blockedBy",
      header: "Votes",
      cell: ({ row }) => {
        const blockedBy = row.original.blockedBy;
        const approvedBy = row.original.approvedBy;

        const waitingForYourReview = row.original.waitingForUserReview;
        const reviewRequired = row.original.reviewRequired;

        return (
          <div className="flex flex-row items-center gap-2">
            {approvedBy.map((reviewer) => (
              <AzureAvatar
                key={reviewer.identity.id}
                user={reviewer.identity}
                className="border-2 border-green-600"
              />
            ))}
            {blockedBy.map((reviewer) => (
              <AzureAvatar
                key={reviewer.identity.id}
                user={reviewer.identity}
                className="border-2 border-red-600"
              />
            ))}
            {waitingForYourReview && (
              <Tooltip>
                <TooltipTrigger>
                  <div className="flex size-5 items-center justify-center rounded-full">
                    <UserXIcon
                      className={cn(
                        "size-4",
                        reviewRequired
                          ? "text-red-600"
                          : "text-muted-foreground",
                      )}
                    />
                  </div>
                </TooltipTrigger>
                <TooltipContent>
                  <span>
                    {reviewRequired
                      ? "Waiting for your review (required)"
                      : "You are an optional reviewer"}
                  </span>
                </TooltipContent>
              </Tooltip>
            )}
          </div>
        );
      },
    },
    {
      header: "Created At",
      accessorFn: (row) => dayjs(row.createdAt),
      cell: ({ getValue }) => {
        const date = getValue() as dayjs.Dayjs;
        return (
          <span className="text-nowrap">
            <span className="hidden 2xl:inline">
              {date.format("YYYY-MM-DD HH:mm")}
            </span>
            <span className="2xl:hidden">{date.format("D/M HH:mm")}</span>
          </span>
        );
      },
    },
  ];
}
