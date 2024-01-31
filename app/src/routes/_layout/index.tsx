import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/")({
  component: IndexComponent,
});

function IndexComponent() {
  return <div>Index!</div>;
}
