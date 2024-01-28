import { queries } from "@/lib/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_protected/auth-test")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.differs()),
  component: AuthTestComponent,
});

function AuthTestComponent() {
  const { data } = useSuspenseQuery(queries.differs());

  return (
    <main className="flex h-screen items-center justify-center">
      <h1 className="text-4xl font-bold">You are authenticated!</h1>
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </main>
  );
}
