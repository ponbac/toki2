import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_protected")({
  beforeLoad: async ({ location }) => {
    if (!isAuthenticated()) {
      throw redirect({
        to: "/login",
        search: {
          next: location.href,
        },
      });
    }
  },
});

function isAuthenticated() {
  return true;
}
