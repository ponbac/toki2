import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { queries } from "@/lib/api/queries/queries";
import { z } from "zod";
import { ProjectSelector } from "./-components/project-selector";
import { SprintSelector } from "./-components/sprint-selector";
import { BoardView } from "./-components/board-view";
import { Suspense, useEffect } from "react";
import { LoadingSpinner } from "@/components/loading-spinner";
import { useAtom } from "jotai";
import { lastViewedProjectAtom } from "./-lib/board-preferences";

const boardSearchSchema = z.object({
  organization: z.string().optional(),
  project: z.string().optional(),
  iterationPath: z.string().optional(),
  team: z.string().optional(),
});

export const Route = createFileRoute("/_layout/board")({
  validateSearch: boardSearchSchema,
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(queries.projects()),
      context.queryClient.ensureQueryData(queries.me()),
    ]);
  },
  component: BoardPage,
});

function BoardPage() {
  const { organization, project, iterationPath, team } = Route.useSearch();
  const { data: projects } = useSuspenseQuery(queries.projects());
  const navigate = useNavigate({ from: Route.fullPath });
  const [lastViewedProject, setLastViewedProject] = useAtom(
    lastViewedProjectAtom,
  );

  // Auto-select last viewed project (or first available) when no project in URL
  useEffect(() => {
    if (organization && project) return;
    if (projects.length === 0) return;

    const target =
      lastViewedProject &&
      projects.some(
        (p) =>
          p.organization === lastViewedProject.organization &&
          p.project === lastViewedProject.project,
      )
        ? lastViewedProject
        : projects[0];

    navigate({
      search: (prev) => ({
        ...prev,
        organization: target.organization,
        project: target.project,
        iterationPath: undefined,
      }),
      replace: true,
    });
  }, [organization, project, projects, lastViewedProject, navigate]);

  // Persist project selection
  useEffect(() => {
    if (organization && project) {
      setLastViewedProject({ organization, project });
    }
  }, [organization, project, setLastViewedProject]);

  return (
    <main className="flex w-full flex-col gap-4 p-4 md:p-8">
      <div className="mx-auto flex w-full max-w-[110rem] flex-col gap-4 md:w-[95%]">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold">Board</h1>
          <h2 className="text-muted-foreground">
            View work items for your projects organized by status.
          </h2>
        </div>

        {/* Selectors */}
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-4">
          <ProjectSelector
            projects={projects}
            selectedOrg={organization}
            selectedProject={project}
          />
          {organization && project && (
            <Suspense
              fallback={
                <div className="flex h-10 w-[280px] items-center justify-center rounded-md border border-input">
                  <LoadingSpinner className="size-4" />
                </div>
              }
            >
              <SprintSelector
                organization={organization}
                project={project}
                selectedIterationPath={iterationPath}
              />
            </Suspense>
          )}
        </div>
      </div>

      {/* Board content */}
      {!organization || !project ? (
        <div className="mx-auto flex h-[60vh] w-full max-w-[110rem] items-center justify-center md:w-[95%]">
          <p className="text-muted-foreground">
            {projects.length === 0
              ? "No projects available. Follow some repositories first."
              : "Select a project to view its board."}
          </p>
        </div>
      ) : (
        <Suspense
          fallback={
            <div className="flex h-[60vh] items-center justify-center">
              <LoadingSpinner className="size-8" />
            </div>
          }
        >
          <BoardView
            organization={organization}
            project={project}
            iterationPath={iterationPath}
            team={team}
          />
        </Suspense>
      )}
    </main>
  );
}
