import * as React from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { Link, createFileRoute, useLocation } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import {
  aiAdminQueries,
  type AiProjectMapping,
  type AiUnmappedProjectKey,
} from "@/lib/api/queries/ai-admin";
import { aiUsageReportQueries } from "@/lib/api/queries/ai-usage-report";
import {
  ProjectPicker,
  type ProjectsQuery,
} from "./-components/project-picker";
import { QueryError } from "./-components/query-error";
import {
  GITHUB_REPOSITORIES_ID,
  RepositoryMappings,
} from "./-components/repository-mappings";
import { StatusBadge } from "./-components/status-badge";
import { formatDateOf, shortDate } from "./-lib/format";

export const Route = createFileRoute("/_layout/admin/project-mappings")({
  component: ProjectMappingsPage,
});

function ProjectMappingsPage() {
  const mappingsQuery = useQuery(aiAdminQueries.projectMappings());
  const unmappedQuery = useQuery(aiAdminQueries.unmappedKeys());
  const projectsQuery = useQuery(aiUsageReportQueries.aiMappingProjects());
  const repositoryMappingsQuery = useQuery(aiAdminQueries.repositoryMappings());
  const hash = useLocation({ select: (location) => location.hash });
  // The sections above grow as their data arrives, so a link to the GitHub
  // repositories brings them into view once every section has loaded.
  const loaded =
    !mappingsQuery.isPending &&
    !unmappedQuery.isPending &&
    !repositoryMappingsQuery.isPending;
  React.useEffect(() => {
    if (loaded && hash === GITHUB_REPOSITORIES_ID) {
      document
        .getElementById(GITHUB_REPOSITORIES_ID)
        ?.scrollIntoView({ block: "start" });
    }
  }, [loaded, hash]);

  return (
    <div className="flex flex-col gap-8">
      <p className="text-sm text-muted-foreground">
        Map token-ledger project keys, such as{" "}
        <code className="text-foreground">github.com/org/repo</code>, to
        time-tracking projects. Mappings are team-wide and resolved whenever
        usage is read, so a change re-attributes everyone&apos;s usage of the
        key, in past months&apos; bills too. Usage of an unmapped, stale or
        unattributed key bills as Unassigned.
      </p>
      <p className="text-sm text-muted-foreground">
        A repository stays private to the developers who use it until one of
        them maps it from their AI usage page. Until then its name is not shown
        here or in billing, only its usage as Unassigned, and nobody else can
        map it. You can change or delete any mapping below. GitHub repositories
        are the exception: GitHub&apos;s billing names the repositories it bills
        code review to, so they are listed and mapped on their own,{" "}
        <Link
          to="/admin/project-mappings"
          hash={GITHUB_REPOSITORIES_ID}
          className="underline underline-offset-4 hover:text-foreground"
        >
          below
        </Link>
        .
      </p>

      {projectsQuery.isError && (
        <QueryError
          error={projectsQuery.error}
          what="the time-tracking projects to map to"
          onRetry={() => projectsQuery.refetch()}
        />
      )}

      <section
        aria-labelledby="unmapped-heading"
        className="flex flex-col gap-2"
      >
        <h2 id="unmapped-heading" className="text-lg font-semibold">
          Your unmapped keys
        </h2>
        <UnmappedKeys query={unmappedQuery} projects={projectsQuery} />
      </section>

      <section
        aria-labelledby="mappings-heading"
        className="flex flex-col gap-2"
      >
        <div className="flex flex-wrap items-end justify-between gap-2">
          <h2 id="mappings-heading" className="text-lg font-semibold">
            Mappings
          </h2>
        </div>
        <Mappings query={mappingsQuery} projects={projectsQuery} />
      </section>

      <RepositoryMappings
        query={repositoryMappingsQuery}
        projects={projectsQuery}
      />
    </div>
  );
}

function UnmappedKeys({
  query,
  projects,
}: {
  query: UseQueryResult<readonly AiUnmappedProjectKey[], Error>;
  projects: ProjectsQuery;
}) {
  if (query.isPending) {
    return <Skeleton className="h-24 w-full" />;
  }
  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="unmapped keys"
        onRetry={() => query.refetch()}
      />
    );
  }
  if (query.data.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        Every project key in your own usage is mapped.
      </p>
    );
  }

  return (
    <div className="rounded-xl border border-border/60 bg-card/60">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Project key</TableHead>
            <TableHead>Last used</TableHead>
            <TableHead className="w-80">Map to</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {query.data.map((key) => (
            <UnmappedKeyRow
              key={key.projectKey}
              unmapped={key}
              projects={projects}
            />
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

function UnmappedKeyRow({
  unmapped,
  projects,
}: {
  unmapped: AiUnmappedProjectKey;
  projects: ProjectsQuery;
}) {
  const [projectId, setProjectId] = React.useState("");
  const setMapping = aiAdminMutations.useSetProjectMapping({
    onSuccess: (mapping) =>
      toast.success(
        `${mapping.projectKey} mapped to ${mapping.project.projectName}`,
      ),
    onError: apiErrorToast("Failed to map the key"),
  });

  return (
    <TableRow>
      <TableCell className="break-all font-mono text-xs">
        {unmapped.projectKey}
      </TableCell>
      <TableCell className="tabular-nums">
        {shortDate(unmapped.lastUsedOn)}
      </TableCell>
      <TableCell>
        {unmapped.mappable ? (
          <div className="flex items-center gap-2">
            <ProjectPicker
              projects={projects}
              value={projectId}
              onChange={setProjectId}
              disabled={setMapping.isPending}
            />
            <Button
              type="button"
              size="sm"
              disabled={!projectId || setMapping.isPending}
              onClick={() =>
                setMapping.mutate({
                  projectKey: unmapped.projectKey,
                  projectId,
                })
              }
            >
              Map
            </Button>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            {unmappableReason(unmapped.projectKey)}
          </p>
        )}
      </TableCell>
    </TableRow>
  );
}

/** Why the server marked a key as not mappable. */
function unmappableReason(projectKey: string): string {
  if (projectKey === "unattributed") {
    return "Usage without project metadata cannot be mapped and stays Unassigned. Configure a project name or Git remote in token-ledger.";
  }
  if (projectKey.trim() !== projectKey) {
    return "A key with surrounding spaces cannot be mapped. Fix the project name in token-ledger.";
  }
  return "Nothing can be mapped until time tracking is configured; the usage stays Unassigned.";
}

function Mappings({
  query,
  projects,
}: {
  query: UseQueryResult<readonly AiProjectMapping[], Error>;
  projects: ProjectsQuery;
}) {
  const [search, setSearch] = React.useState("");

  if (query.isPending) {
    return <Skeleton className="h-40 w-full" />;
  }
  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="mappings"
        onRetry={() => query.refetch()}
      />
    );
  }
  if (query.data.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">No key is mapped yet.</p>
    );
  }

  const needle = search.trim().toLowerCase();
  const mappings = query.data.filter(
    (mapping) =>
      !needle ||
      mapping.projectKey.toLowerCase().includes(needle) ||
      mapping.project.projectName.toLowerCase().includes(needle),
  );

  return (
    <div className="flex flex-col gap-2">
      <Input
        className="max-w-sm"
        placeholder="Search keys and projects..."
        aria-label="Search mappings"
        value={search}
        onChange={(event) => setSearch(event.target.value)}
      />
      <div className="rounded-xl border border-border/60 bg-card/60">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Project key</TableHead>
              <TableHead>Project</TableHead>
              <TableHead>State</TableHead>
              <TableHead>Last changed</TableHead>
              <TableHead className="w-96">Change</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {mappings.map((mapping) => (
              <MappingRow
                key={mapping.projectKey}
                mapping={mapping}
                projects={projects}
              />
            ))}
          </TableBody>
        </Table>
        {mappings.length === 0 && (
          <p className="p-4 text-sm text-muted-foreground">
            No mapping matches.
          </p>
        )}
      </div>
    </div>
  );
}

function MappingRow({
  mapping,
  projects,
}: {
  mapping: AiProjectMapping;
  projects: ProjectsQuery;
}) {
  const [projectId, setProjectId] = React.useState("");
  const setMapping = aiAdminMutations.useSetProjectMapping({
    onSuccess: (saved) => {
      toast.success(
        `${saved.projectKey} now maps to ${saved.project.projectName}`,
      );
      setProjectId("");
    },
    onError: apiErrorToast("Failed to change the mapping"),
  });
  const deleteMapping = aiAdminMutations.useDeleteProjectMapping({
    onSuccess: () => toast.success(`${mapping.projectKey} is unmapped`),
    onError: apiErrorToast("Failed to delete the mapping"),
  });
  const busy = setMapping.isPending || deleteMapping.isPending;
  const editor = mapping.updatedBy?.fullName ?? mapping.createdBy?.fullName;

  return (
    <TableRow>
      <TableCell className="break-all font-mono text-xs">
        {mapping.projectKey}
      </TableCell>
      <TableCell>{mapping.project.projectName}</TableCell>
      <TableCell>
        <div className="flex flex-wrap gap-1">
          {mapping.status === "resolves" ? (
            <StatusBadge tone="good" label="Resolves" />
          ) : mapping.status === "stale" ? (
            <StatusBadge tone="critical" label="Stale: bills as Unassigned" />
          ) : (
            <StatusBadge
              tone="warning"
              label="Time tracking not configured: bills as Unassigned"
            />
          )}
          {!mapping.inUse && <StatusBadge tone="neutral" label="Not in use" />}
        </div>
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {/* The date only: when someone edits is not the admin's business. */}
        {formatDateOf(mapping.updatedAt)}
        {editor && <div>by {editor}</div>}
      </TableCell>
      <TableCell>
        <div className="flex items-center gap-2">
          <ProjectPicker
            projects={projects}
            value={projectId}
            onChange={setProjectId}
            disabled={busy}
          />
          <Button
            type="button"
            size="sm"
            disabled={!projectId || busy}
            onClick={() =>
              setMapping.mutate({ projectKey: mapping.projectKey, projectId })
            }
          >
            Save
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-8 shrink-0"
            aria-label={`Delete the mapping of ${mapping.projectKey}`}
            disabled={busy}
            onClick={() => {
              if (
                window.confirm(
                  `Unmap ${mapping.projectKey}? Its usage will bill as Unassigned, in past months too.`,
                )
              ) {
                deleteMapping.mutate({ projectKey: mapping.projectKey });
              }
            }}
          >
            <Trash2 className="size-4" />
          </Button>
        </div>
      </TableCell>
    </TableRow>
  );
}
