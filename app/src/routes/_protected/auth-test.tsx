import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_protected/auth-test")({
  component: AuthTestComponent,
});

function AuthTestComponent() {
  return (
    <main className="flex h-screen items-center justify-center">
      <h1 className="text-4xl font-bold">You are authenticated!</h1>
    </main>
  );
}
