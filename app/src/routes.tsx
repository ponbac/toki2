import { QueryClient } from "@tanstack/react-query";
import { Outlet, Route, rootRouteWithContext } from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/router-devtools";
import { NavMenu } from "./components/nav-menu";
import { Home } from "./pages/home/home";
import { About } from "./pages/about/about";
import { NotFound } from "./pages/not-found/not-found";
import { Login } from "./pages/login/login";
import { Editor } from "./pages/editor/editor";

const rootRoute = rootRouteWithContext<{
  queryClient: QueryClient;
}>()({
  component: () => {
    const isAuthenticated = localStorage.getItem("isAuthenticated");

    return (
      <div className="flex min-h-screen w-full flex-col bg-background">
        {isAuthenticated ? (
          <>
            <NavMenu />
            <main className="h-full p-8">
              <Outlet />
            </main>
            <TanStackRouterDevtools position="bottom-left" />
          </>
        ) : (
          <Login />
        )}
      </div>
    );
  },
});

const indexRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => <Home />,
});

const notFoundRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "*",
  component: () => <NotFound />,
});

const editorRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/editor",
  component: () => <Editor />,
});

const aboutRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/about",
  component: () => <About />,
});

export const routeTree = rootRoute.addChildren([
  indexRoute,
  notFoundRoute,
  editorRoute,
  aboutRoute,
]);
