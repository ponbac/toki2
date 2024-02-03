import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { DataTable } from "./-components/data-table";
import { pullRequestColumns } from "./-components/columns";
import { queries } from "@/lib/api/queries/queries";

export const Route = createFileRoute("/_layout/prs/")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(
      queries.cachedPullRequests({
        organization: "ex-change-part",
        project: "Quote Manager",
        repoName: "hexagon",
      }),
    ),
  component: PrsComponent,
});

function PrsComponent() {
  const { data } = useSuspenseQuery(
    queries.cachedPullRequests({
      organization: "ex-change-part",
      project: "Quote Manager",
      repoName: "hexagon",
    }),
  );

  const navigate = useNavigate();

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4">
      <h1 className="text-4xl font-bold">PRS!</h1>
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
    </main>
  );
}
