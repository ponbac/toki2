import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { queries } from "@/lib/queries";
import { useMutation, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/auth-test")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.differs()),
  component: AuthTestComponent,
});

type RepoKey = {
  organization: string;
  project: string;
  repoName: string;
};

function AuthTestComponent() {
  const { data, refetch } = useSuspenseQuery(queries.differs());

  const { mutate: startDiffer } = useMutation({
    mutationFn: (repoKey: RepoKey) =>
      api.post("differs/start", {
        json: repoKey,
      }),
    onSuccess: () => {
      refetch();
    },
  });

  const { mutate: stopDiffer } = useMutation({
    mutationFn: (repoKey: RepoKey) =>
      api.post("differs/stop", {
        json: repoKey,
      }),
    onSuccess: () => {
      refetch();
    },
  });

  return (
    <main className="flex w-full flex-col items-center justify-center gap-4">
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
