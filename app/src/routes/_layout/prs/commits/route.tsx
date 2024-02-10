import { queries } from "@/lib/api/queries/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/prs/commits")({
  component: CommitsComponent,
});

function CommitsComponent() {
  const { data } = useSuspenseQuery(
    queries.mostRecentCommits({
      organization: "ex-change-part",
      project: "Quote Manager",
      repoName: "hexagon",
    }),
  );

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4 p-8">
      <h1>Commits</h1>
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </main>
  );
}