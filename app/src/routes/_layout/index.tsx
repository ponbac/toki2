import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/")({
  loader: () => {
    const screenWidth = window.innerWidth;
    // Tailwind MD breakpoint
    if (screenWidth < 768) {
      throw redirect({
        to: "/milltime",
      });
    }

    throw redirect({
      to: "/prs",
    });
  },
});
