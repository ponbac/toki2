import { AzureAvatar } from "@/components/azure-avatar";
import { PRLink } from "@/components/pr-link";
import { WorkItemLink } from "@/components/work-item-link";
import { PullRequest } from "@/lib/api/queries/pullRequests";
import { ColumnDef } from "@tanstack/react-table";
import dayjs from "dayjs";

export const pullRequestColumns: ColumnDef<PullRequest>[] = [
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
        <span>
          {initialChars.trimEnd()}
          {initialChars.length < row.original.title.length ? "..." : ""}
        </span>
      );
    },
  },
  {
    accessorKey: "createdBy.displayName",
    header: "Author",
    cell: ({ row }) => {
      return (
        <div className="flex flex-row items-center gap-2">
          <AzureAvatar user={row.original.createdBy} />
          <span>{row.original.createdBy.displayName}</span>
        </div>
      );
    },
  },
  {
    accessorKey: "workItems",
    header: "Work Items",
    cell: ({ row }) => {
      return (
        <div className="flex flex-row items-center gap-2">
          {row.original.workItems.map((wi) => (
            <WorkItemLink
              data={{
                ...row.original,
                id: wi.id,
              }}
            />
          ))}
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
        .filter((reviewer) => reviewer.vote === "Approved")
        .filter((r) => !blockedBy.find((b) => b.identity.id === r.identity.id));

      return (
        <div className="flex flex-row items-center gap-2">
          {approvedBy.map((reviewer) => (
            <AzureAvatar
              user={reviewer.identity}
              className="border-2 border-green-600"
            />
          ))}
          {blockedBy.map((reviewer) => (
            <AzureAvatar
              user={reviewer.identity}
              className="border-2 border-red-600"
            />
          ))}
        </div>
      );
    },
  },
  {
    header: "Created At",
    accessorFn: (row) => dayjs(row.createdAt).format("YYYY-MM-DD HH:mm"),
  },
];
