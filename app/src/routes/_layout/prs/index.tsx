import { queries } from "@/lib/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

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

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4">
      <h1 className="text-4xl font-bold">PRS!</h1>
      <code>{JSON.stringify(data, null, 2)}</code>
    </main>
  );
}
