import { useSuspenseQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute, useNavigate } from "@tanstack/react-router";
import { DataTable } from "./-components/data-table";
import { pullRequestColumns } from "./-components/columns";
import { queries } from "@/lib/api/queries/queries";
import { z } from "zod";
import { useMemo, useRef } from "react";
import { SearchCode } from "lucide-react";
import { Input } from "@/components/ui/input";
import { PullRequest } from "@/lib/api/queries/pullRequests";

const pullRequestsSearchSchema = z.object({
  searchString: z.string().optional().catch(""),
});

export const Route = createFileRoute("/_layout/prs")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.cachedPullRequests()),
  validateSearch: pullRequestsSearchSchema,
  component: PrsComponent,
});

function PrsComponent() {
  const navigate = useNavigate();
  const { searchString } = Route.useSearch();
  const { data } = useSuspenseQuery({
    ...queries.cachedPullRequests(),
    refetchInterval: 60 * 1000,
  });

  const filteredData = useMemo(
    () => filterPullRequests(data ?? [], searchString ?? ""),
    [data, searchString],
  );

  return (
    <main className="flex w-full items-center justify-center p-8">
      <div className="flex w-[85%] max-w-[95rem] flex-col items-center justify-center gap-4">
        <TopBar />
        <DataTable
          data={filteredData}
          columns={pullRequestColumns}
          onRowClick={(row) =>
            navigate({
              to: `/prs/$prId`,
              params: { prId: `${row.id}` },
              search: { searchString },
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
  const { searchString } = Route.useSearch();

  const inputRef = useRef<HTMLInputElement>(null);

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
      </div>
    </div>
  );
}

function filterPullRequests(data: Array<PullRequest>, searchString: string) {
  if (!searchString) return data;

  const lowerCaseSearchString = searchString.toLowerCase();
  return data.filter(
    (pr) =>
      pr.title.toLowerCase().includes(lowerCaseSearchString) ||
      pr.repoName.toLowerCase().includes(lowerCaseSearchString) ||
      pr.createdBy.displayName.toLowerCase().includes(lowerCaseSearchString) ||
      pr.workItems.some((wi) => `#${wi.id}`.includes(searchString)) ||
      `!${pr.id}`.includes(searchString),
  );
}
