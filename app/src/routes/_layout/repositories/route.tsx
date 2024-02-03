import { Button } from "@/components/ui/button";
import { mutations } from "@/lib/api/mutations/mutations";
import { queries } from "@/lib/api/queries/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/repositories")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.differs()),
  component: RepositoriesComponent,
});

function RepositoriesComponent() {
  const { data } = useSuspenseQuery(queries.differs());

  const { mutate: startDiffer } = mutations.useStartDiffers();
  const { mutate: stopDiffer } = mutations.useStopDiffers();

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4">
      <h1 className="text-4xl font-bold">You are authenticated!</h1>
      <Button
        onClick={() => {
          startDiffer({
            organization: "ex-change-part",
            project: "Quote Manager",
            repoName: "hexagon",
          });
        }}
      >
        <span className="mr-2">Start differ</span>
        <span>🚀</span>
      </Button>
      <Button
        onClick={() => {
          stopDiffer({
            organization: "ex-change-part",
            project: "Quote Manager",
            repoName: "hexagon",
          });
        }}
      >
        <span className="mr-2">Stop differ</span>
        <span>🛑</span>
      </Button>
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </main>
  );
}
