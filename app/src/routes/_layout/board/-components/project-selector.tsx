import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { WorkItemProject } from "@/lib/api/queries/workItems";
import { useNavigate } from "@tanstack/react-router";
import { Route } from "../route";

export function ProjectSelector({
  projects,
  selectedOrg,
  selectedProject,
}: {
  projects: WorkItemProject[];
  selectedOrg?: string;
  selectedProject?: string;
}) {
  const navigate = useNavigate({ from: Route.fullPath });

  const selectedIndex = projects.findIndex(
    (project) =>
      project.organization === selectedOrg && project.project === selectedProject,
  );
  const currentValue = selectedIndex >= 0 ? String(selectedIndex) : "";

  return (
    <Select
      value={currentValue}
      onValueChange={(value) => {
        const project = projects[Number(value)];
        if (!project) return;

        navigate({
          search: (prev) => ({
            ...prev,
            organization: project.organization,
            project: project.project,
            // Reset iteration when project changes
            iterationPath: undefined,
          }),
        });
      }}
    >
      <SelectTrigger className="w-[280px]">
        <SelectValue placeholder="Select a project..." />
      </SelectTrigger>
      <SelectContent>
        {projects.map((project, index) => {
          const key = `${project.organization}/${project.project}`;
          return (
            <SelectItem key={key} value={String(index)}>
              {key}
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
