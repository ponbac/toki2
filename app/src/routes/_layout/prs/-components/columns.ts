import { ColumnDef } from "@tanstack/react-table";
import dayjs from "dayjs";

export type TablePullRequest = {
  id: string;
  title: string;
  createdAt: string;
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
    header: "Created At",
    accessorFn: (row) => dayjs(row.createdAt).format("YYYY-MM-DD HH:mm"),
  },
];
