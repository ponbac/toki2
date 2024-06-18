/* prettier-ignore-start */

/* eslint-disable */

// @ts-nocheck

// noinspection JSUnusedGlobalSymbols

// This file is auto-generated by TanStack Router

// Import Routes

import { Route as rootRoute } from "./routes/__root"
import { Route as LoginImport } from "./routes/login"
import { Route as LayoutRouteImport } from "./routes/_layout/route"
import { Route as LayoutIndexImport } from "./routes/_layout/index"
import { Route as LayoutRepositoriesRouteImport } from "./routes/_layout/repositories/route"
import { Route as LayoutPrsRouteImport } from "./routes/_layout/prs/route"
import { Route as LayoutMilltimeRouteImport } from "./routes/_layout/milltime/route"
import { Route as LayoutRepositoriesIndexImport } from "./routes/_layout/repositories/index"
import { Route as LayoutPrsIndexImport } from "./routes/_layout/prs/index"
import { Route as LayoutRepositoriesAddRouteImport } from "./routes/_layout/repositories/add/route"
import { Route as LayoutPrsCommitsRouteImport } from "./routes/_layout/prs/commits/route"
import { Route as LayoutPrsPrIdRouteImport } from "./routes/_layout/prs/$prId/route"

// Create/Update Routes

const LoginRoute = LoginImport.update({
  path: "/login",
  getParentRoute: () => rootRoute,
} as any)

const LayoutRouteRoute = LayoutRouteImport.update({
  id: "/_layout",
  getParentRoute: () => rootRoute,
} as any)

const LayoutIndexRoute = LayoutIndexImport.update({
  path: "/",
  getParentRoute: () => LayoutRouteRoute,
} as any)

const LayoutRepositoriesRouteRoute = LayoutRepositoriesRouteImport.update({
  path: "/repositories",
  getParentRoute: () => LayoutRouteRoute,
} as any)

const LayoutPrsRouteRoute = LayoutPrsRouteImport.update({
  path: "/prs",
  getParentRoute: () => LayoutRouteRoute,
} as any)

const LayoutMilltimeRouteRoute = LayoutMilltimeRouteImport.update({
  path: "/milltime",
  getParentRoute: () => LayoutRouteRoute,
} as any)

const LayoutRepositoriesIndexRoute = LayoutRepositoriesIndexImport.update({
  path: "/",
  getParentRoute: () => LayoutRepositoriesRouteRoute,
} as any)

const LayoutPrsIndexRoute = LayoutPrsIndexImport.update({
  path: "/",
  getParentRoute: () => LayoutPrsRouteRoute,
} as any)

const LayoutRepositoriesAddRouteRoute = LayoutRepositoriesAddRouteImport.update(
  {
    path: "/add",
    getParentRoute: () => LayoutRepositoriesRouteRoute,
  } as any,
)

const LayoutPrsCommitsRouteRoute = LayoutPrsCommitsRouteImport.update({
  path: "/commits",
  getParentRoute: () => LayoutPrsRouteRoute,
} as any)

const LayoutPrsPrIdRouteRoute = LayoutPrsPrIdRouteImport.update({
  path: "/$prId",
  getParentRoute: () => LayoutPrsRouteRoute,
} as any)

// Populate the FileRoutesByPath interface

declare module "@tanstack/react-router" {
  interface FileRoutesByPath {
    "/_layout": {
      id: "/_layout"
      path: ""
      fullPath: ""
      preLoaderRoute: typeof LayoutRouteImport
      parentRoute: typeof rootRoute
    }
    "/login": {
      id: "/login"
      path: "/login"
      fullPath: "/login"
      preLoaderRoute: typeof LoginImport
      parentRoute: typeof rootRoute
    }
    "/_layout/milltime": {
      id: "/_layout/milltime"
      path: "/milltime"
      fullPath: "/milltime"
      preLoaderRoute: typeof LayoutMilltimeRouteImport
      parentRoute: typeof LayoutRouteImport
    }
    "/_layout/prs": {
      id: "/_layout/prs"
      path: "/prs"
      fullPath: "/prs"
      preLoaderRoute: typeof LayoutPrsRouteImport
      parentRoute: typeof LayoutRouteImport
    }
    "/_layout/repositories": {
      id: "/_layout/repositories"
      path: "/repositories"
      fullPath: "/repositories"
      preLoaderRoute: typeof LayoutRepositoriesRouteImport
      parentRoute: typeof LayoutRouteImport
    }
    "/_layout/": {
      id: "/_layout/"
      path: "/"
      fullPath: "/"
      preLoaderRoute: typeof LayoutIndexImport
      parentRoute: typeof LayoutRouteImport
    }
    "/_layout/prs/$prId": {
      id: "/_layout/prs/$prId"
      path: "/$prId"
      fullPath: "/prs/$prId"
      preLoaderRoute: typeof LayoutPrsPrIdRouteImport
      parentRoute: typeof LayoutPrsRouteImport
    }
    "/_layout/prs/commits": {
      id: "/_layout/prs/commits"
      path: "/commits"
      fullPath: "/prs/commits"
      preLoaderRoute: typeof LayoutPrsCommitsRouteImport
      parentRoute: typeof LayoutPrsRouteImport
    }
    "/_layout/repositories/add": {
      id: "/_layout/repositories/add"
      path: "/add"
      fullPath: "/repositories/add"
      preLoaderRoute: typeof LayoutRepositoriesAddRouteImport
      parentRoute: typeof LayoutRepositoriesRouteImport
    }
    "/_layout/prs/": {
      id: "/_layout/prs/"
      path: "/"
      fullPath: "/prs/"
      preLoaderRoute: typeof LayoutPrsIndexImport
      parentRoute: typeof LayoutPrsRouteImport
    }
    "/_layout/repositories/": {
      id: "/_layout/repositories/"
      path: "/"
      fullPath: "/repositories/"
      preLoaderRoute: typeof LayoutRepositoriesIndexImport
      parentRoute: typeof LayoutRepositoriesRouteImport
    }
  }
}

// Create and export the route tree

export const routeTree = rootRoute.addChildren({
  LayoutRouteRoute: LayoutRouteRoute.addChildren({
    LayoutMilltimeRouteRoute,
    LayoutPrsRouteRoute: LayoutPrsRouteRoute.addChildren({
      LayoutPrsPrIdRouteRoute,
      LayoutPrsCommitsRouteRoute,
      LayoutPrsIndexRoute,
    }),
    LayoutRepositoriesRouteRoute: LayoutRepositoriesRouteRoute.addChildren({
      LayoutRepositoriesAddRouteRoute,
      LayoutRepositoriesIndexRoute,
    }),
    LayoutIndexRoute,
  }),
  LoginRoute,
})

/* prettier-ignore-end */

/* ROUTE_MANIFEST_START
{
  "routes": {
    "__root__": {
      "filePath": "__root.tsx",
      "children": [
        "/_layout",
        "/login"
      ]
    },
    "/_layout": {
      "filePath": "_layout/route.tsx",
      "children": [
        "/_layout/milltime",
        "/_layout/prs",
        "/_layout/repositories",
        "/_layout/"
      ]
    },
    "/login": {
      "filePath": "login.tsx"
    },
    "/_layout/milltime": {
      "filePath": "_layout/milltime/route.tsx",
      "parent": "/_layout"
    },
    "/_layout/prs": {
      "filePath": "_layout/prs/route.tsx",
      "parent": "/_layout",
      "children": [
        "/_layout/prs/$prId",
        "/_layout/prs/commits",
        "/_layout/prs/"
      ]
    },
    "/_layout/repositories": {
      "filePath": "_layout/repositories/route.tsx",
      "parent": "/_layout",
      "children": [
        "/_layout/repositories/add",
        "/_layout/repositories/"
      ]
    },
    "/_layout/": {
      "filePath": "_layout/index.tsx",
      "parent": "/_layout"
    },
    "/_layout/prs/$prId": {
      "filePath": "_layout/prs/$prId/route.tsx",
      "parent": "/_layout/prs"
    },
    "/_layout/prs/commits": {
      "filePath": "_layout/prs/commits/route.tsx",
      "parent": "/_layout/prs"
    },
    "/_layout/repositories/add": {
      "filePath": "_layout/repositories/add/route.tsx",
      "parent": "/_layout/repositories"
    },
    "/_layout/prs/": {
      "filePath": "_layout/prs/index.tsx",
      "parent": "/_layout/prs"
    },
    "/_layout/repositories/": {
      "filePath": "_layout/repositories/index.tsx",
      "parent": "/_layout/repositories"
    }
  }
}
ROUTE_MANIFEST_END */
