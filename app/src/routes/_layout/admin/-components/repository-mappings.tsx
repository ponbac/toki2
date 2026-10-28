import * as React from "react";
import type { UseQueryResult } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
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
import type {
  AiRepositoryMappingRow,
  AiRepositoryMappings,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { formatDateOf, monthLabel, shortDate } from "../-lib/format";
import {
  activeSuggestions,
  repositoryNeedsLook,
  repositoryStatus,
  suggestionTitle,
  unbilledCreditsReason,
} from "../-lib/repository-mappings";
import { creditHundredths, formatCreditNumber } from "../-lib/reconciliation";
import { ProjectPicker, type ProjectsQuery } from "./project-picker";
import { QueryError } from "./query-error";
import { StatusBadge } from "./status-badge";

/** The DOM id of the section, which links to it point at as a hash. */
export const GITHUB_REPOSITORIES_ID = "github-repositories";

const AMOUNT = "text-right tabular-nums whitespace-nowrap";

/**
 * The repositories GitHub billed Copilot code review to, with their credits
 * per UTC month, and the projects those credits bill to: suggested by mapped
 * project keys naming the repository, or picked by hand.
 */
export function RepositoryMappings({
  query,
  projects,
}: {
  query: UseQueryResult<AiRepositoryMappings, Error>;
  projects: ProjectsQuery;
}) {
  return (
    <section
      id={GITHUB_REPOSITORIES_ID}
      aria-labelledby="github-repositories-heading"
      className="flex scroll-mt-24 flex-col gap-2"
    >
      <h2 id="github-repositories-heading" className="text-lg font-semibold">
        GitHub repositories
      </h2>
      <p className="max-w-4xl text-sm text-muted-foreground">
        GitHub bills Copilot code review to the repository it reviewed. Map each
        repository to the project its reviews are for.{" "}
        {query.isSuccess && <CutoverNote billedFrom={query.data.billedFrom} />}{" "}
        Repository names come from GitHub&apos;s billing, which admins see;
        project keys stay private until a developer maps them, and suggestions
        only come from mapped keys. A repository without a project bills as
        Unassigned and holds back month close once its credits bill a line;
        greyed credits (before the cutover, or under one credit for the month)
        bill none. A mapping applies to every open month&apos;s bill; a closed
        month gets an adjustment on the next open month&apos;s bill.
      </p>
      <RepositoryTable query={query} projects={projects} />
    </section>
  );
}

/** When repository credits bill from, in a sentence. */
function CutoverNote({ billedFrom }: { billedFrom: string | null }) {
  return billedFrom === null ? (
    <>
      Nothing is billed from GitHub&apos;s credits yet: no cutover month is
      configured.
    </>
  ) : (
    <>
      Its credits then bill to that project at $0.01 a credit, per UTC month,
      from {monthLabel(billedFrom)}; earlier months are not billed from credits.
    </>
  );
}

function RepositoryTable({
  query,
  projects,
}: {
  query: UseQueryResult<AiRepositoryMappings, Error>;
  projects: ProjectsQuery;
}) {
  if (query.isPending) {
    return <Skeleton className="h-32 w-full" />;
  }
  if (query.isError) {
    return (
      <QueryError
        error={query.error}
        what="GitHub repositories"
        onRetry={() => query.refetch()}
      />
    );
  }
  if (query.data.repositories.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        GitHub has billed no code review to a repository yet.
      </p>
    );
  }

  return (
    <div className="rounded-xl border border-border/60 bg-card/60">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Repository</TableHead>
            {query.data.months.map((month) => (
              <TableHead key={month} className="text-right whitespace-nowrap">
                {monthLabel(month)}
                <span className="sr-only"> credits, UTC</span>
              </TableHead>
            ))}
            <TableHead>Project</TableHead>
            <TableHead className="w-96">Change</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {query.data.repositories.map((row) => (
            <RepositoryRow
              key={row.repository.toLowerCase()}
              row={row}
              billedFrom={query.data.billedFrom}
              projects={projects}
            />
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

function RepositoryRow({
  row,
  billedFrom,
  projects,
}: {
  row: AiRepositoryMappingRow;
  billedFrom: string | null;
  projects: ProjectsQuery;
}) {
  const [projectId, setProjectId] = React.useState("");
  const setMapping = aiAdminMutations.useSetRepositoryMapping({
    onSuccess: (saved) => {
      toast.success(
        `${saved.repository} now bills to ${saved.project.projectName}`,
      );
      setProjectId("");
    },
    onError: apiErrorToast("Failed to map the repository"),
  });
  const deleteMapping = aiAdminMutations.useDeleteRepositoryMapping({
    onSuccess: () => toast.success(`${row.repository} is unmapped`),
    onError: apiErrorToast("Failed to delete the mapping"),
  });
  const busy = setMapping.isPending || deleteMapping.isPending;
  const { mapping } = row;
  const status = repositoryStatus(row);
  const editor = mapping?.updatedBy?.fullName ?? mapping?.createdBy?.fullName;
  const suggestions = activeSuggestions(row.suggestions, projects.data);

  return (
    <TableRow className="align-top">
      <TableCell className="py-2.5">
        <div className="flex flex-col gap-0.5">
          <span className="font-mono text-xs break-all text-foreground">
            {row.repository}
          </span>
          <span className="text-xs text-muted-foreground">
            {row.lastBilledOn === null
              ? "no credits billed"
              : `last credits ${shortDate(row.lastBilledOn)}`}
          </span>
        </div>
      </TableCell>
      {row.months.map((monthCredits) => {
        const { month, credits } = monthCredits;
        const unbilled = unbilledCreditsReason(monthCredits, billedFrom);
        return (
          <TableCell
            key={month}
            title={unbilled ?? undefined}
            className={cn(
              AMOUNT,
              "py-2.5",
              (creditHundredths(credits) === 0 || unbilled !== null) &&
                "text-muted-foreground",
            )}
          >
            {creditHundredths(credits) === 0
              ? "—"
              : formatCreditNumber(credits)}
            {unbilled !== null && (
              <span className="sr-only">, {unbilled.toLowerCase()}</span>
            )}
          </TableCell>
        );
      })}
      <TableCell className="py-2.5">
        <div className="flex flex-col items-start gap-1">
          {mapping && <span>{mapping.project.projectName}</span>}
          <StatusBadge
            tone={
              status === "unmapped" && !repositoryNeedsLook(row)
                ? "neutral"
                : STATUS_TONES[status]
            }
            label={STATUS_LABELS[status]}
          />
          {mapping && (
            <span className="text-xs text-muted-foreground">
              {/* The date only, as for project-key mappings. */}
              {formatDateOf(mapping.updatedAt)}
              {editor && ` by ${editor}`}
            </span>
          )}
        </div>
      </TableCell>
      <TableCell className="py-2.5">
        <div className="flex flex-col gap-2">
          {suggestions.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {suggestions.map((suggestion) => (
                <Button
                  key={suggestion.project.projectId}
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-7 px-2 text-xs"
                  title={suggestionTitle(suggestion)}
                  disabled={busy}
                  onClick={() =>
                    setMapping.mutate({
                      repository: row.repository,
                      projectId: suggestion.project.projectId,
                    })
                  }
                >
                  Map to {suggestion.project.projectName}
                </Button>
              ))}
            </div>
          )}
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
                setMapping.mutate({ repository: row.repository, projectId })
              }
            >
              {mapping ? "Save" : "Map"}
            </Button>
            {mapping && (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-8 shrink-0"
                aria-label={`Delete the mapping of ${row.repository}`}
                disabled={busy}
                onClick={() => {
                  if (
                    window.confirm(
                      `Unmap ${row.repository}? Its code review credits will bill as Unassigned in open months; closed months get an adjustment on the next open month's bill.`,
                    )
                  ) {
                    deleteMapping.mutate({ repository: row.repository });
                  }
                }}
              >
                <Trash2 className="size-4" />
              </Button>
            )}
          </div>
        </div>
      </TableCell>
    </TableRow>
  );
}

const STATUS_LABELS = {
  resolves: "Resolves",
  stale: "Stale: bills as Unassigned",
  unconfigured: "Time tracking not configured: bills as Unassigned",
  unmapped: "Not mapped: bills as Unassigned",
} as const satisfies Record<ReturnType<typeof repositoryStatus>, string>;

const STATUS_TONES = {
  resolves: "good",
  stale: "critical",
  unconfigured: "warning",
  unmapped: "warning",
} as const satisfies Record<ReturnType<typeof repositoryStatus>, string>;
