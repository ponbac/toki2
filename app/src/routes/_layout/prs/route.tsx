import { useSuspenseQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute, useNavigate } from "@tanstack/react-router";
import { DataTable } from "./-components/data-table";
import { pullRequestColumns } from "./-components/columns";
import { queries } from "@/lib/api/queries/queries";
import { z } from "zod";
import { useMemo, useRef } from "react";
import {
  SearchCode,
  UserIcon,
  ShieldAlertIcon,
  NotebookPenIcon,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { PullRequest } from "@/lib/api/queries/pullRequests";
import { Button } from "@/components/ui/button";
import { User } from "@/lib/api/queries/user";

const pullRequestsSearchSchema = z.object({
  searchString: z.string().optional().catch(""),
  filterAuthor: z.boolean().optional().catch(false),
  filterReviewer: z.boolean().optional().catch(false),
  filterBlocking: z.boolean().optional().catch(false),
});

export const Route = createFileRoute("/_layout/prs")({
  loader: ({ context }) => {
    context.queryClient.ensureQueryData(queries.me());
    context.queryClient.ensureQueryData(queries.cachedPullRequests());
  },
  shouldReload: false,
  validateSearch: pullRequestsSearchSchema,
  component: PrsComponent,
});

function PrsComponent() {
  const navigate = useNavigate();
  const { searchString, filterAuthor, filterReviewer, filterBlocking } =
    Route.useSearch();

  const { data: user } = useSuspenseQuery(queries.me());
  const { data: cachedPullRequests } = useSuspenseQuery({
    ...queries.cachedPullRequests(),
    refetchInterval: 60 * 1000,
  });

  const filteredData = useMemo(
    () =>
      filterPullRequests(
        cachedPullRequests ?? [],
        searchString ?? "",
        user,
        filterAuthor ?? false,
        filterReviewer ?? false,
        filterBlocking ?? false,
      ),
    [
      cachedPullRequests,
      searchString,
      user,
      filterAuthor,
      filterReviewer,
      filterBlocking,
    ],
  );

  return (
    <main className="flex w-full items-center justify-center p-8">
      <div className="flex w-[95%] max-w-[100rem] flex-col items-center justify-center gap-4">
        <TopBar />
        <DataTable
          data={filteredData}
          columns={pullRequestColumns(user)}
          onRowClick={(row) =>
            navigate({
              to: `/prs/$prId`,
              params: { prId: `${row.id}` },
              search: {
                searchString,
                filterAuthor,
                filterReviewer,
                filterBlocking,
              },
            })
          }
        />
      </div>
      <Outlet />
    </main>
  );
}

function TopBar() {
  const navigate = useNavigate({ from: Route.fullPath });
  const { searchString, filterAuthor, filterReviewer, filterBlocking } =
    Route.useSearch();

  const inputRef = useRef<HTMLInputElement>(null);

  const toggleFilter = (filter: "author" | "reviewer" | "blocking") => {
    navigate({
      search: (prev) => {
        const newSearch: Record<string, boolean | undefined> = {
          filterAuthor: undefined,
          filterReviewer: undefined,
          filterBlocking: undefined,
        };

        // TODO: this is ugly!
        if (
          prev[
            `filter${filter.charAt(0).toUpperCase() + filter.slice(1)}` as keyof typeof prev
          ] !== true
        ) {
          newSearch[
            `filter${filter.charAt(0).toUpperCase() + filter.slice(1)}` as keyof typeof newSearch
          ] = true;
        }

        return {
          ...prev,
          ...newSearch,
        };
      },
    });
  };

  return (
    <div className="flex w-full flex-col gap-2">
      <div>
        <h1 className="text-2xl font-bold">Pull requests</h1>
        <h2 className="text-muted-foreground">
          Open pull requests in your followed repositories are shown here.
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
            placeholder="Search pull requests..."
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
        <Button
          variant={filterAuthor ? "default" : "outline"}
          onClick={() => toggleFilter("author")}
          className="flex items-center gap-2"
        >
          <UserIcon className="size-4" />
          My PRs
        </Button>
        <Button
          variant={filterReviewer ? "default" : "outline"}
          onClick={() => toggleFilter("reviewer")}
          className="flex items-center gap-2"
        >
          <NotebookPenIcon className="size-4" />
          Reviews
        </Button>
        <Button
          variant={filterBlocking ? "default" : "outline"}
          onClick={() => toggleFilter("blocking")}
          className="flex items-center gap-2"
        >
          <ShieldAlertIcon className="size-4" />
          Blocking
        </Button>
      </div>
    </div>
  );
}

function filterPullRequests(
  data: Array<PullRequest>,
  searchString: string,
  user: User | undefined,
  filterAuthor: boolean,
  filterReviewer: boolean,
  filterBlocking: boolean,
) {
  let filteredData = data;

  if (filterAuthor && user) {
    filteredData = filteredData.filter(
      (pr) => pr.createdBy.uniqueName === user.email,
    );
  }

  if (filterReviewer && user) {
    filteredData = filteredData.filter((pr) =>
      pr.reviewers.some(
        (reviewer) =>
          reviewer.identity.uniqueName === user.email &&
          !pr.isDraft &&
          pr.createdBy.uniqueName !== user.email,
      ),
    );
  }

  if (filterBlocking && user) {
    filteredData = filteredData.filter((pr) =>
      pr.blockedBy.some(
        (blocker) => blocker.identity.uniqueName === user.email,
      ),
    );
  }

  if (!searchString) return filteredData;

  const lowerCaseSearchString = searchString.toLowerCase();
  return filteredData.filter(
    (pr) =>
      pr.title.toLowerCase().includes(lowerCaseSearchString) ||
      pr.repoName.toLowerCase().includes(lowerCaseSearchString) ||
      pr.createdBy.displayName.toLowerCase().includes(lowerCaseSearchString) ||
      pr.workItems.some((wi) => `#${wi.id}`.includes(searchString)) ||
      `!${pr.id}`.includes(searchString),
  );
}
