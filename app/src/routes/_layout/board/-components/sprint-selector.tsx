import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { queries } from "@/lib/api/queries/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { Route } from "../route";

export function SprintSelector({
  organization,
  project,
  selectedIterationPath,
}: {
  organization: string;
  project: string;
  selectedIterationPath?: string;
}) {
  const navigate = useNavigate({ from: Route.fullPath });

  const { data: iterations } = useSuspenseQuery(
    queries.iterations(organization, project),
  );

  // Auto-select the current iteration on first load
  useEffect(() => {
    if (!selectedIterationPath && iterations.length > 0) {
      const current = iterations.find((i) => i.isCurrent);
      if (current) {
        navigate({
          search: (prev) => ({
            ...prev,
            iterationPath: current.path,
          }),
        });
      }
    }
  }, [iterations, selectedIterationPath, navigate]);

  return (
    <Select
      value={selectedIterationPath ?? ""}
      onValueChange={(value) => {
        navigate({
          search: (prev) => ({
            ...prev,
            iterationPath: value,
          }),
        });
      }}
    >
      <SelectTrigger className="w-[280px]">
        <SelectValue placeholder="Select a sprint..." />
      </SelectTrigger>
      <SelectContent>
        {iterations.map((iteration) => (
          <SelectItem key={iteration.id} value={iteration.path}>
            <div className="flex items-center gap-2">
              <span>{iteration.name}</span>
              {iteration.isCurrent && (
                <span className="rounded-full bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">
                  Current
                </span>
              )}
            </div>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
