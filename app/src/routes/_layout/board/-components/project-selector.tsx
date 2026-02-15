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

  const currentValue =
    selectedOrg && selectedProject
      ? `${selectedOrg}/${selectedProject}`
      : undefined;

  return (
    <Select
      value={currentValue ?? ""}
      onValueChange={(value) => {
        const [org, project] = value.split("/");
        navigate({
          search: (prev) => ({
            ...prev,
            organization: org,
            project,
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
        {projects.map((p) => {
          const key = `${p.organization}/${p.project}`;
          return (
            <SelectItem key={key} value={key}>
              {key}
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
