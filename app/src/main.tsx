import "./index.css";
import React from "react";
import ReactDOM from "react-dom/client";
import { routeTree } from "./routeTree.gen";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { ThemeProvider } from "./hooks/useTheme";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30 * 1000,
    },
  },
});

export type RouterContext = {
  queryClient: QueryClient;
};

export const router = createRouter({
  routeTree,
  defaultNotFoundComponent: () => "404 Not Found",
  context: {
    queryClient,
  },
  defaultPreload: "intent",
  defaultPreloadStaleTime: 0,
});

// Register things for typesafety
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    navigator.serviceWorker
      .register("/service-worker.js")
      .then((registration) => {
        console.log("Service Worker registered:", registration);
      })
      .catch((error) => {
        console.error("Service Worker registration failed:", error);
      });
  });
}

// Migrate localStorage keys renamed in the milltime → time-tracking rename.
// Safe to remove once all users have loaded the app at least once after this change.
const LOCAL_STORAGE_KEY_MIGRATIONS: [string, string][] = [
  ["milltime-lastProject", "time-tracking-lastProject"],
  ["milltime-lastActivity", "time-tracking-lastActivity"],
  ["milltime-rememberLastProject", "time-tracking-rememberLastProject"],
  ["milltime-mergeSameDay", "time-tracking-mergeSameDay"],
  ["milltime-viewMode", "time-tracking-viewMode"],
  [
    "milltime-previous-week-alert-disabled",
    "time-tracking-previous-week-alert-disabled",
  ],
];

for (const [oldKey, newKey] of LOCAL_STORAGE_KEY_MIGRATIONS) {
  const value = localStorage.getItem(oldKey);
  if (value !== null) {
    localStorage.setItem(newKey, value);
    localStorage.removeItem(oldKey);
  }
}

ReactDOM.createRoot(document.getElementById("app")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
        <ReactQueryDevtools buttonPosition="top-right" />
      </QueryClientProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
