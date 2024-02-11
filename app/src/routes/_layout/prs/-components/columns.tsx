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
    header: "Created At",
    accessorFn: (row) => dayjs(row.createdAt).format("YYYY-MM-DD HH:mm"),
  },
];
