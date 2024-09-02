import { AzureAvatar } from "@/components/azure-avatar";
import { PRLink } from "@/components/pr-link";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { WorkItemLink } from "@/components/work-item-link";
import { PullRequest } from "@/lib/api/queries/pullRequests";
import { ColumnDef } from "@tanstack/react-table";
import dayjs from "dayjs";
import { ReactNode } from "react";
import {
  CopySlashIcon,
  MessageCircleQuestionIcon,
  PickaxeIcon,
  UserXIcon,
} from "lucide-react";
import { User } from "@/lib/api/queries/user";
import { cn } from "@/lib/utils";

export function pullRequestColumns(
  user: User | undefined,
): ColumnDef<PullRequest>[] {
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
        const initialChars = row.original.title.slice(0, 60);
        return (
          <div className="flex flex-row items-center gap-2">
            <span className="text-nowrap">
              {initialChars.trimEnd()}
              {initialChars.length < row.original.title.length ? "..." : ""}
            </span>
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
        return (
          <div className="flex flex-row items-center justify-center gap-2 xl:justify-start">
            <AzureAvatar user={row.original.createdBy} />
            <span className="hidden text-nowrap xl:block">
              {row.original.createdBy.displayName}
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
        const approvedBy = row.original.reviewers
          .filter(
            (reviewer) =>
              reviewer.vote === "Approved" ||
              reviewer.vote === "ApprovedWithSuggestions",
          )
          .filter(
            (r) => !blockedBy.find((b) => b.identity.id === r.identity.id),
          );

        const waitingForYourReview = row.original.reviewers.find(
          (reviewer) =>
            reviewer.identity.uniqueName === user?.email &&
            reviewer.vote === "NoResponse" &&
            !row.original.isDraft &&
            row.original.createdBy.uniqueName !== user?.email &&
            !blockedBy.find((b) => b.identity.id === reviewer.identity.id),
        );

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
                        waitingForYourReview.isRequired
                          ? "text-red-600"
                          : "text-muted-foreground",
                      )}
                    />
                  </div>
                </TooltipTrigger>
                <TooltipContent>
                  <span>
                    {waitingForYourReview.isRequired
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
      accessorFn: (row) => dayjs(row.createdAt).format("YYYY-MM-DD HH:mm"),
      cell: ({ getValue }) => (
        <span className="text-nowrap">{getValue() as ReactNode}</span>
      ),
    },
  ];
}

function StatusIcon(props: { tooltip: string; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger>{props.children}</TooltipTrigger>
      <TooltipContent>
        <span>{props.tooltip}</span>
      </TooltipContent>
    </Tooltip>
  );
}
