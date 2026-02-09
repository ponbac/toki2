import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/milltime")({
  beforeLoad: () => {
    throw redirect({
      to: "/time-tracking",
    });
  },
});
