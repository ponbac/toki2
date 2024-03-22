import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/")({
  loader: () =>
    redirect({
      to: "/prs",
    }),
});
