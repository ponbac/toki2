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
import type { Iteration } from "@/lib/api/queries/workItems";

function getEffectiveFinishTimeMs(finishDate: string): number {
  const finish = new Date(finishDate);
  const isMidnightUtc =
    finish.getUTCHours() === 0 &&
    finish.getUTCMinutes() === 0 &&
    finish.getUTCSeconds() === 0 &&
    finish.getUTCMilliseconds() === 0;

  // ADO often stores finish dates at midnight; treat those as end-of-day inclusive.
  return isMidnightUtc ? finish.getTime() + 24 * 60 * 60 * 1000 - 1 : finish.getTime();
}

function isCurrentIteration(iteration: Iteration, nowMs: number): boolean {
  if (iteration.isCurrent) return true;

  // Match backend semantics: date-range fallback is only valid when both dates exist.
  if (!iteration.startDate || !iteration.finishDate) return false;

  const startMs = new Date(iteration.startDate).getTime();
  const finishMs = getEffectiveFinishTimeMs(iteration.finishDate);

  return nowMs >= startMs && nowMs <= finishMs;
}

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

  // Keep URL sprint selection valid, defaulting to current sprint when missing/invalid.
  useEffect(() => {
    if (iterations.length === 0) return;
    const nowMs = Date.now();

    const selectedExists = selectedIterationPath
      ? iterations.some((iteration) => iteration.path === selectedIterationPath)
      : false;

    if (selectedExists) return;

    const currentIteration = iterations.find((iteration) =>
      isCurrentIteration(iteration, nowMs),
    );
    if (currentIteration) {
      navigate({
        search: (prev) => ({
          ...prev,
          iterationPath: currentIteration.path,
        }),
        replace: true,
      });
      return;
    }

    if (selectedIterationPath) {
      navigate({
        search: (prev) => ({
          ...prev,
          iterationPath: undefined,
        }),
        replace: true,
      });
    }
  }, [iterations, selectedIterationPath, navigate]);

  const nowMsForRender = Date.now();

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
