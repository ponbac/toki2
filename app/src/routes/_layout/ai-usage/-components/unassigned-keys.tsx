import * as React from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { toast } from "sonner";
import { Combobox } from "@/components/combobox";
import { Button } from "@/components/ui/button";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import { apiErrorMessage } from "@/lib/api/errors";
import { aiProjectMappingMutations } from "@/lib/api/mutations/ai-project-mappings";
import {
  aiUsageReportQueries,
  type AiUsageProjectKeyList,
} from "@/lib/api/queries/ai-usage-report";
import { ChartCard } from "./chart-card";
import { QueryError } from "./query-state";
import { useApiErrorMessage } from "../-lib/use-api-error-message";
import { formatLongDate } from "../-lib/format";

type ProjectKey = AiUsageProjectKeyList["projectKeys"][number];

/**
 * Project keys whose usage is unassigned, with a way to map the ones the user
 * may map or to mark them as personal, and an explanation for the others.
 * The keys the user marked personal follow, with a way to undo that.
 */
export function UnassignedKeys({
  query,
  className,
  id,
}: {
  query: UseQueryResult<AiUsageProjectKeyList, Error>;
  className?: string;
  id?: string;
}) {
  const configured = query.data?.timeTrackingConfigured ?? true;
  const keys =
    query.data?.projectKeys.filter(
      (key) => key.status !== "mapped" && key.status !== "personal",
    ) ?? [];
  const personal =
    query.data?.projectKeys.filter((key) => key.status === "personal") ?? [];
  const mappable = keys.filter((key) => key.mappable);

  return (
    <ChartCard
      id={id}
      className={className}
      title="Unassigned repositories"
      description={`Usage of these project keys counts for no time-tracking project. A key without a mapping shows its name only to the developers who use it. Mapping it shows the name to admins and applies to everyone's usage of the key, including past usage. A repository that is not client work can be marked ${PERSONAL_LABEL} instead: your usage of it is left out of the bill, and its name stays with you.`}
    >
      {query.isPending ? (
        <p className="text-sm text-muted-foreground" role="status">
          Loading project keys…
        </p>
      ) : query.isError ? (
        <QueryError
          message="Could not load your project keys."
          error={query.error}
          onRetry={() => void query.refetch()}
        />
      ) : keys.length === 0 && personal.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          All of your usage counts for a project.
        </p>
      ) : (
        <div className="space-y-3">
          {keys.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No repository is waiting to be mapped.
            </p>
          )}
          {!configured && (
            <p className="rounded-lg border border-border/70 bg-muted/40 px-3 py-2 text-xs">
              Time tracking is not configured on this Toki server, so no usage
              counts for a project and no key can be mapped. Existing mappings
              apply again once it is configured. Mapping keys cannot fix this;
              whoever runs Toki has to configure time tracking.
            </p>
          )}
          {mappable.length > 0 && (
            <p className="text-xs text-muted-foreground">
              You can map a key that has no mapping yet. Once it is mapped, only
              an admin can change it, so check the project first.
            </p>
          )}
          <ul className="divide-y divide-border/60">
            {keys.map((key) => (
              <li key={key.projectKey} className="py-2.5 first:pt-0 last:pb-0">
                <KeyItem projectKey={key} configured={configured} />
              </li>
            ))}
          </ul>
          {personal.length > 0 && (
            <section
              aria-labelledby="personal-keys-heading"
              className="space-y-1.5 border-t border-border/60 pt-3"
            >
              <h3 id="personal-keys-heading" className="text-sm font-medium">
                {PERSONAL_LABEL}
              </h3>
              <p className="text-xs text-muted-foreground">
                Your usage of these is left out of the bill. Only you see their
                names.
              </p>
              <ul className="divide-y divide-border/60">
                {personal.map((key) => (
                  <li key={key.projectKey} className="py-2 last:pb-0">
                    <PersonalKeyItem projectKey={key.projectKey} />
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      )}
    </ChartCard>
  );
}

function KeyItem({
  projectKey,
  configured,
}: {
  projectKey: ProjectKey;
  configured: boolean;
}) {
  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3">
        <p
          className="min-w-0 truncate font-mono text-sm"
          title={projectKey.projectKey}
        >
          {projectKey.projectKey}
        </p>
        <p className="text-xs text-muted-foreground">
          Last used {formatLongDate(projectKey.lastUsedOn)}
        </p>
      </div>
      {projectKey.mappable ? (
        <MapKeyForm projectKey={projectKey.projectKey} />
      ) : (
        <p className="text-xs text-muted-foreground">
          {explanation(projectKey, configured)}
        </p>
      )}
      {canBePersonal(projectKey) && (
        <MarkPersonalButton projectKey={projectKey.projectKey} />
      )}
    </div>
  );
}

/**
 * Whether the user can mark the key as personal: it has no mapping, and it is
 * a key the server accepts, without surrounding whitespace. Unlike mapping,
 * this does not need time tracking.
 */
function canBePersonal(projectKey: ProjectKey): boolean {
  return (
    projectKey.status === "unmapped" &&
    projectKey.projectKey.trim() === projectKey.projectKey
  );
}

function MarkPersonalButton({ projectKey }: { projectKey: string }) {
  const mark = aiProjectMappingMutations.useMarkAiProjectKeyPersonal({
    onSuccess: () => {
      toast.success(`${projectKey} is now ${PERSONAL_LABEL}`, {
        description: "Your usage of it is left out of the bill.",
      });
    },
    onError: (error) => {
      void apiErrorMessage(error, "The server did not say why.").then(
        (reason) => {
          toast.error(`Could not mark ${projectKey}`, { description: reason });
        },
      );
    },
  });

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={mark.isPending}
      onClick={() => mark.mutate({ projectKey })}
    >
      {mark.isPending ? "Marking..." : `Mark as ${PERSONAL_LABEL}`}
    </Button>
  );
}

function PersonalKeyItem({ projectKey }: { projectKey: string }) {
  const unmark = aiProjectMappingMutations.useUnmarkAiProjectKeyPersonal({
    onSuccess: () => {
      toast.success(`${projectKey} is unassigned again`);
    },
    onError: (error) => {
      void apiErrorMessage(error, "The server did not say why.").then(
        (reason) => {
          toast.error(`Could not undo ${projectKey}`, { description: reason });
        },
      );
    },
  });

  return (
    <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
      <p className="min-w-0 truncate font-mono text-sm" title={projectKey}>
        {projectKey}
      </p>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={unmark.isPending}
        aria-label={`Undo ${PERSONAL_LABEL} for ${projectKey}`}
        onClick={() => unmark.mutate({ projectKey })}
      >
        {unmark.isPending ? "Undoing..." : "Undo"}
      </Button>
    </div>
  );
}

/** Why a key that cannot be mapped here stays unassigned, and what fixes it. */
function explanation(projectKey: ProjectKey, configured: boolean): string {
  switch (projectKey.status) {
    case "unattributed":
      return "Usage without a project name or Git remote. It cannot be mapped: configure a project name or a Git remote for it in token-ledger, and later syncs attribute it.";
    case "stale":
      return "Mapped to a project outside the configured time-tracking company. Only an admin can change a mapping, so ask an admin to map it again.";
    case "unconfigured":
      return "Mapped, but the mapping does not apply while time tracking is not configured.";
    case "unmapped":
      return configured
        ? "This key starts or ends with whitespace and cannot be mapped. Fix the project name in token-ledger."
        : "It can be mapped once time tracking is configured.";
    case "mapped":
    case "personal":
      return "";
  }
}

function MapKeyForm({ projectKey }: { projectKey: string }) {
  const [projectId, setProjectId] = React.useState("");
  const projects = useQuery(aiUsageReportQueries.aiMappingProjects());
  const projectsError = useApiErrorMessage(projects.error);
  const map = aiProjectMappingMutations.useMapAiProjectKey({
    onSuccess: (mapping) => {
      toast.success(
        `${mapping.projectKey} now counts for ${mapping.project.projectName}`,
      );
    },
    // The key list refreshes either way, so a key someone else mapped first
    // leaves this list; say why this attempt failed.
    onError: (error, vars) => {
      void apiErrorMessage(error, "The server did not say why.").then(
        (reason) => {
          toast.error(`Could not map ${vars.projectKey}`, {
            description: reason,
          });
        },
      );
    },
  });
  const items = React.useMemo(
    () =>
      (projects.data ?? []).map((project) => ({
        value: project.projectId,
        label: project.projectName,
      })),
    [projects.data],
  );

  if (projects.isError) {
    return (
      <p className="text-xs text-destructive">
        Projects cannot be listed right now
        {projectsError ? `: ${projectsError}` : "."}
      </p>
    );
  }

  return (
    <form
      className="flex flex-wrap items-center gap-2"
      aria-label={`Map ${projectKey}`}
      onSubmit={(event) => {
        event.preventDefault();
        const project = projects.data?.find(
          (candidate) => candidate.projectId === projectId,
        );
        if (
          project &&
          window.confirm(
            `Map ${projectKey} to ${project.projectName}? This applies to everyone's usage of the key, and only an admin can change it later.`,
          )
        ) {
          map.mutate({ projectKey, projectId });
        }
      }}
    >
      <div className="min-w-48 flex-1">
        <Combobox
          items={items}
          placeholder="Choose project"
          searchPlaceholder="Search projects..."
          emptyMessage="No project found."
          value={projectId}
          onChange={setProjectId}
          isLoading={projects.isPending}
          loadingMessage="Loading projects..."
          disabled={map.isPending}
        />
      </div>
      <Button
        type="submit"
        size="sm"
        disabled={projectId === "" || map.isPending}
      >
        {map.isPending ? "Mapping..." : "Map"}
      </Button>
    </form>
  );
}
