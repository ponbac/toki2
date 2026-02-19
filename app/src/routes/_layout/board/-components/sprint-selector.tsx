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
import {
  isCurrentIteration,
  resolveEffectiveIterationPath,
} from "../-lib/iterations";

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
  const effectiveIterationPath = resolveEffectiveIterationPath(
    iterations,
    selectedIterationPath,
  );

  // Keep URL sprint selection valid, defaulting to current sprint when missing/invalid.
  useEffect(() => {
    if (effectiveIterationPath === selectedIterationPath) return;

    navigate({
      search: (prev) => ({
        ...prev,
        iterationPath: effectiveIterationPath,
      }),
      replace: true,
    });
  }, [effectiveIterationPath, selectedIterationPath, navigate]);

  const nowMsForRender = Date.now();

  return (
    <Select
      value={effectiveIterationPath ?? ""}
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
        {iterations.map((iteration) => {
          const current = isCurrentIteration(iteration, nowMsForRender);
          return (
            <SelectItem key={iteration.id} value={iteration.path}>
              {iteration.name}
              {current ? " (Current)" : ""}
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
