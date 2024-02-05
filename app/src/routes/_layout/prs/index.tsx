import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { DataTable } from "./-components/data-table";
import { pullRequestColumns } from "./-components/columns";
import { queries } from "@/lib/api/queries/queries";

export const Route = createFileRoute("/_layout/prs/")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.cachedPullRequests()),
  component: PrsComponent,
});

function PrsComponent() {
  const { data } = useSuspenseQuery(queries.cachedPullRequests());

  const navigate = useNavigate();

  return (
    <main className="flex w-full items-center justify-center py-8">
      <div className="flex min-w-[77rem] flex-col items-center justify-center gap-4">
        <div className="w-full">
          <h1 className="text-2xl font-bold">Pull requests</h1>
          <h2 className="text-muted-foreground">
            Open pull requests in your followed repositories is shown here.
          </h2>
        </div>
        <DataTable
          data={data}
          columns={pullRequestColumns}
          onRowClick={(row) =>
            navigate({
              to: `/prs/$prId`,
              params: { prId: row.id },
            })
          }
        />
      </div>
    </main>
  );
}
