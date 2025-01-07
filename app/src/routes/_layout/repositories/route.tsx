import { buttonVariants } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { queries } from "@/lib/api/queries/queries";
import { cn, toRepoKeyString } from "@/lib/utils";
import { useSuspenseQuery } from "@tanstack/react-query";
import {
  Link,
  Outlet,
  createFileRoute,
  useNavigate,
} from "@tanstack/react-router";
import { Plus, SearchCode } from "lucide-react";
import { useMemo, useRef } from "react";
import { z } from "zod";
import { RepoCard } from "./-components/repo-card";

const repositoriesSearchSchema = z.object({
  searchString: z.string().optional().catch(""),
});

export const Route = createFileRoute("/_layout/repositories")({
  validateSearch: repositoriesSearchSchema,
  loader: ({ context }) => {
    context.queryClient.ensureQueryData(queries.me());
    context.queryClient.ensureQueryData(queries.differs());
  },
  component: RepositoriesComponent,
});

function RepositoriesComponent() {
  const { searchString } = Route.useSearch();

  const { data: isAdmin } = useSuspenseQuery({
    ...queries.me(),
    select: (data) => data.roles.includes("Admin"),
  });
  const { data, dataUpdatedAt } = useSuspenseQuery({
    ...queries.differs(),
    refetchInterval: 15 * 1000,
  });

  const filteredData = useMemo(
    () =>
      data.filter((differ) =>
        toRepoKeyString(differ)
          .toLowerCase()
          .includes(searchString?.toLowerCase() ?? ""),
      ),
    [data, searchString],
  );

  const followedRepos = useMemo(
    () => filteredData.filter((differ) => differ.followed),
    [filteredData],
  );
  const unfollowedRepos = useMemo(
    () => filteredData.filter((differ) => !differ.followed),
    [filteredData],
  );

  return (
    <main className="flex w-full items-center justify-center p-8">
      <div className="flex flex-col items-center justify-center gap-4">
        <TopBar />
        <div className="flex w-full flex-col gap-4">
          {/* Followed Repositories Section */}
          {followedRepos.length > 0 && (
            <div className="flex flex-col gap-2">
              <h2 className="text-xl font-semibold">Followed</h2>
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
                {followedRepos.length > 0 ? (
                  followedRepos.map((differ) => (
                    <RepoCard
                      key={`${toRepoKeyString(differ)}-${dataUpdatedAt}`}
                      differ={differ}
                      isAdmin={isAdmin}
                    />
                  ))
                ) : (
                  <Card className="w-[25rem] opacity-0" />
                )}
              </div>
            </div>
          )}

          {/* Unfollowed Repositories Section */}
          {unfollowedRepos.length > 0 && (
            <div className="flex flex-col gap-2">
              <h2 className="text-xl font-semibold">All repositories</h2>
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
                {unfollowedRepos.length > 0 ? (
                  unfollowedRepos.map((differ) => (
                    <RepoCard
                      key={`${toRepoKeyString(differ)}-${dataUpdatedAt}`}
                      differ={differ}
                      isAdmin={isAdmin}
                    />
                  ))
                ) : (
                  <Card className="w-[25rem] opacity-0" />
                )}
              </div>
            </div>
          )}
        </div>
      </div>
      <Outlet />
    </main>
  );
}

function TopBar() {
  const navigate = useNavigate({ from: Route.fullPath });
  const { searchString } = Route.useSearch();

  const inputRef = useRef<HTMLInputElement>(null);

  return (
    <div className="flex w-full flex-col gap-2">
      <div>
        <h1 className="text-2xl font-bold">Repositories</h1>
        <h2 className="text-muted-foreground">
          Follow the repositories you want to keep an eye on by clicking the
          heart.
        </h2>
      </div>
      <div className="flex gap-4">
        <div className="relative flex w-full items-center">
          <SearchCode
            onClick={() => inputRef.current?.focus()}
            className="absolute left-2 top-1/2 size-4 -translate-y-1/2 transform cursor-pointer"
          />
          <Input
            ref={inputRef}
            placeholder="Search connected repositories..."
            value={searchString ?? ""}
            onChange={(event) => {
              const value = event.target.value;
              navigate({
                search: (prev) => ({
                  ...prev,
                  searchString: value.length ? event.target.value : undefined,
                }),
              });
            }}
            className="pl-8"
          />
        </div>
        <Link
          to="/repositories/add"
          className={cn(buttonVariants({ variant: "outline" }), "gap-1")}
        >
          <Plus size="1.25rem" />
          Add repository
        </Link>
      </div>
    </div>
  );
}
