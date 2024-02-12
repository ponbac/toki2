import { AzureAvatar } from "@/components/azure-avatar";
import { PullRequest } from "@/lib/api/queries/pullRequests";
import { ColumnDef } from "@tanstack/react-table";
import dayjs from "dayjs";

export const pullRequestColumns: ColumnDef<PullRequest>[] = [
  {
    accessorKey: "id",
    header: "ID",
    cell: ({ row }) => `!${row.original.id}`,
  },
  {
    accessorKey: "repoName",
    header: "Repository",
  },
  {
    accessorKey: "title",
    header: "Title",
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
            <span>#{wi.id}</span>
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
