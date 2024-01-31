import { queries } from "@/lib/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/prs/$prId")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(
      queries.cachedPullRequests({
        organization: "ex-change-part",
        project: "Quote Manager",
        repoName: "hexagon",
      }),
    ),
  component: PrComponent,
});

function PrComponent() {
  const { prId } = Route.useParams();

  const { data } = useSuspenseQuery({
    ...queries.cachedPullRequests({
      organization: "ex-change-part",
      project: "Quote Manager",
      repoName: "hexagon",
    }),
    select: (data) => data.find((pr) => pr.id === +prId),
  });
  console.log(data);

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4 p-8">
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </main>
  );
}
