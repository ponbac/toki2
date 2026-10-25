import type { UseQueryResult } from "@tanstack/react-query";
import { Combobox } from "@/components/combobox";
import type { AiUsageProject } from "@/lib/api/queries/ai-usage-report";

/** The active time-tracking projects a mapping can point at. */
export type ProjectsQuery = UseQueryResult<readonly AiUsageProject[], Error>;

/** A searchable choice of an active time-tracking project, by id. */
export function ProjectPicker({
  projects,
  value,
  onChange,
  disabled,
}: {
  projects: ProjectsQuery;
  value: string;
  onChange: (projectId: string) => void;
  disabled?: boolean;
}) {
  return (
    <Combobox
      items={(projects.data ?? []).map((project) => ({
        value: project.projectId,
        label: project.projectName,
      }))}
      placeholder="Choose project"
      searchPlaceholder="Search projects..."
      emptyMessage="No active project matches."
      value={value}
      onChange={onChange}
      disabled={disabled || !projects.data}
      isLoading={projects.isPending}
      loadingMessage="Loading projects..."
    />
  );
}
