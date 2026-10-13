import * as React from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import {
  Ban,
  CircleCheck,
  CircleDashed,
  Info,
  UserRound,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import { Combobox } from "@/components/combobox";
import { Button } from "@/components/ui/button";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import { apiErrorMessage } from "@/lib/api/errors";
import { cn } from "@/lib/utils";
import { aiProjectMappingMutations } from "@/lib/api/mutations/ai-project-mappings";
import {
  aiUsageReportQueries,
  type AiUsageProjectKeyList,
} from "@/lib/api/queries/ai-usage-report";
import { QueryError } from "./query-state";
import { RepoKey } from "./repo-key";
import { useApiErrorMessage } from "../-lib/use-api-error-message";
import { formatLongDate } from "../-lib/format";

type ProjectKey = AiUsageProjectKeyList["projectKeys"][number];

/**
 * The user's repositories that count for no project, by what can be done
 * about each: the ones to map (or to mark as personal), the ones that cannot
 * be mapped here and why, and the ones marked personal, with a way to undo
 * that.
 */
export function RepositoriesPanel({
  query,
}: {
  query: UseQueryResult<AiUsageProjectKeyList, Error>;
}) {
  if (query.isPending) {
    return (
      <p className="text-sm text-muted-foreground" role="status">
        Loading repositories…
      </p>
    );
  }
  if (query.isError) {
    return (
      <QueryError
        message="Could not load your repositories."
        error={query.error}
        onRetry={() => void query.refetch()}
      />
    );
  }
  const configured = query.data.timeTrackingConfigured;
  const open = query.data.projectKeys.filter(
    (key) => key.status !== "mapped" && key.status !== "personal",
  );
  const mappable = open.filter((key) => key.mappable);
  const blocked = open.filter((key) => !key.mappable);
  const personal = query.data.projectKeys.filter(
    (key) => key.status === "personal",
  );

  if (open.length === 0 && personal.length === 0) {
    return (
      <div className="rounded-2xl border border-dashed border-border/80 bg-card/60 p-8 text-center">
        <p className="flex items-center justify-center gap-2 font-medium">
          <CircleCheck aria-hidden className="size-4 text-[#0ca30c]" />
          All of your usage counts for a project.
        </p>
        <p className="mt-1 text-sm text-muted-foreground">
          A repository shows up here when its usage has no time-tracking project
          yet.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <p className="max-w-3xl text-sm text-muted-foreground">
        Usage of these repositories counts for no time-tracking project. A
        repository without a mapping shows its name only to the developers who
        use it. Mapping it shows the name to admins and applies to
        everyone&apos;s usage of it, including past usage. One that is not
        client work can be marked {PERSONAL_LABEL} instead: your usage of it is
        left out of the bill, and nothing is mapped, so admins do not see its
        name.
      </p>
      {!configured && (
        <p className="flex max-w-3xl items-start gap-2 rounded-lg border border-border/70 bg-muted/40 px-3 py-2 text-xs">
          <Info aria-hidden className="mt-px size-3.5 shrink-0" />
          Time tracking is not configured on this Toki server, so no usage
          counts for a project and nothing can be mapped. Existing mappings
          apply again once it is configured. Mapping cannot fix this; whoever
          runs Toki has to configure time tracking.
        </p>
      )}
      <div className="grid items-start gap-5 lg:grid-cols-2">
        <div className="space-y-5">
          <KeyGroup
            icon={CircleDashed}
            title="To map"
            count={mappable.length}
            attention={mappable.length > 0}
            note={
              mappable.length > 0
                ? "Once a repository is mapped, only an admin can change it, so check the project first."
                : "No repository is waiting to be mapped."
            }
          >
            {mappable.map((key) => (
              <li key={key.projectKey} className="py-3 first:pt-0 last:pb-0">
                <KeyItem projectKey={key} configured={configured} />
              </li>
            ))}
          </KeyGroup>
          {blocked.length > 0 && (
            <KeyGroup
              icon={Ban}
              title="Cannot be mapped here"
              count={blocked.length}
            >
              {blocked.map((key) => (
                <li key={key.projectKey} className="py-3 first:pt-0 last:pb-0">
                  <KeyItem projectKey={key} configured={configured} />
                </li>
              ))}
            </KeyGroup>
          )}
        </div>
        {personal.length > 0 && (
          <KeyGroup
            icon={UserRound}
            title={PERSONAL_LABEL}
            count={personal.length}
            note="Your usage of these is left out of the bill. Their names are shown only to the developers who use them. If one of them maps a repository to a project, that applies to you too."
            columns
          >
            {personal.map((key) => (
              <li key={key.projectKey} className="break-inside-avoid">
                <PersonalKeyItem projectKey={key.projectKey} />
              </li>
            ))}
          </KeyGroup>
        )}
      </div>
    </div>
  );
}

/** One kind of repository: its mark, how many there are, and the list. */
function KeyGroup({
  icon: Icon,
  title,
  count,
  attention = false,
  note,
  columns = false,
  children,
}: {
  icon: LucideIcon;
  title: string;
  count: number;
  /** The group waits for the user: it takes the attention colour. */
  attention?: boolean;
  note?: string;
  /** Lay short rows out in two columns where there is room. */
  columns?: boolean;
  children: React.ReactNode;
}) {
  return (
    <section
      aria-label={title}
      className={cn(
        "rounded-2xl border bg-card p-5 shadow-sm",
        attention ? "border-primary/50" : "border-border/70",
      )}
    >
      <header className="flex items-center justify-between gap-3">
        <h2
          className={cn(
            "flex items-center gap-2 text-sm font-semibold",
            attention && "text-primary",
          )}
        >
          <Icon
            aria-hidden
            className={cn("size-4", !attention && "text-muted-foreground")}
          />
          {title}
        </h2>
        <span
          className={cn(
            "rounded-full px-2 py-0.5 text-[11px] font-semibold tabular-nums",
            attention
              ? "bg-primary/10 text-primary"
              : "bg-muted text-muted-foreground",
          )}
        >
          {count}
        </span>
      </header>
      {note && <p className="mt-1 text-xs text-muted-foreground">{note}</p>}
      {count > 0 && (
        <ul
          className={cn(
            "mt-3",
            columns ? "gap-x-6 sm:columns-2" : "divide-y divide-border/60",
          )}
        >
          {children}
        </ul>
      )}
    </section>
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
        <p className="min-w-0 font-mono text-sm">
          <RepoKey projectKey={projectKey.projectKey} className="max-w-full" />
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
    <div className="group/key flex items-center justify-between gap-2 rounded-md py-0.5">
      <p className="min-w-0 font-mono text-sm">
        <RepoKey projectKey={projectKey} className="max-w-full" />
      </p>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={unmark.isPending}
        aria-label={`Undo ${PERSONAL_LABEL} for ${projectKey}`}
        onClick={() => unmark.mutate({ projectKey })}
        className="h-7 shrink-0 px-2 text-xs text-muted-foreground"
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
