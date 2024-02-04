import { ColumnDef } from "@tanstack/react-table";
import dayjs from "dayjs";

export type TablePullRequest = {
  id: string;
  title: string;
  createdAt: string;
  createdBy: {
    displayName: string;
    avatarUrl: string;
  };
};

export const pullRequestColumns: ColumnDef<TablePullRequest>[] = [
  {
    accessorKey: "id",
    header: "ID",
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
        <div className="flex flex-row items-center justify-between gap-2">
          <span>{row.original.createdBy.displayName}</span>
          <img
            src={row.original.createdBy.avatarUrl}
            alt={row.original.createdBy.displayName}
            className="h-6 w-6 rounded-full"
          />
        </div>
      );
    },
  },
  {
    header: "Created At",
    accessorFn: (row) => dayjs(row.createdAt).format("YYYY-MM-DD HH:mm"),
  },
];
