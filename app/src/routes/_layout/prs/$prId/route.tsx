import { queries } from "@/lib/api/queries/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/prs/$prId")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.cachedPullRequests()),
  component: PrComponent,
});

function PrComponent() {
  const { prId } = Route.useParams();

  const { data } = useSuspenseQuery({
    ...queries.cachedPullRequests(),
    select: (data) => data.find((pr) => pr.id === +prId),
  });

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4 p-8">
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </main>
  );
}
