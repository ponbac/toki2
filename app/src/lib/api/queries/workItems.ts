import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export type WorkItemProject = {
  organization: string;
  project: string;
};

export type WorkItemPerson = {
  displayName: string;
  uniqueName: string | null;
  imageUrl: string | null;
};

export type WorkItemRef = {
  id: string;
  title: string | null;
};

export type PullRequestRef = {
  id: string;
  repositoryId: string;
  projectId: string;
  url: string;
};

export type BoardWorkItem = {
  id: string;
  title: string;
  boardState: "todo" | "inProgress" | "done";
  boardColumnId: string | null;
  boardColumnName: string | null;
  category: "userStory" | "bug" | "task" | "feature" | "epic" | string;
  stateName: string;
  priority: number | null;
  assignedTo: WorkItemPerson | null;
  createdBy: WorkItemPerson | null;
  description: string | null;
  acceptanceCriteria: string | null;
  iterationPath: string | null;
  areaPath: string | null;
  tags: string[];
  parent: WorkItemRef | null;
  related: WorkItemRef[];
  pullRequests?: PullRequestRef[];
  url: string;
  createdAt: string;
  changedAt: string;
};

export type BoardColumn = {
  id: string;
  name: string;
  order: number;
};

export type BoardResponse = {
  columns: BoardColumn[];
  items: BoardWorkItem[];
};

export type Iteration = {
  id: string;
  name: string;
  path: string;
  startDate: string | null;
  finishDate: string | null;
  isCurrent: boolean;
};

export type FormatForLlmResponse = {
  markdown: string;
  hasImages: boolean;
};

export const workItemsQueries = {
  baseKey: ["workItems"] as const,
  projects: () =>
    queryOptions({
      queryKey: [...workItemsQueries.baseKey, "projects"],
      queryFn: async () =>
        api.get("work-items/projects").json<Array<WorkItemProject>>(),
    }),
  iterations: (org: string, project: string) =>
    queryOptions({
      queryKey: [...workItemsQueries.baseKey, "iterations", org, project],
      queryFn: async () =>
        api
          .get("work-items/iterations", {
            searchParams: { organization: org, project },
          })
          .json<Array<Iteration>>(),
    }),
  board: (params: {
    organization: string;
    project: string;
    iterationPath?: string;
    team?: string;
  }) =>
    queryOptions({
      queryKey: [...workItemsQueries.baseKey, "board", params],
      refetchInterval: 60 * 1000,
      queryFn: async () =>
        api
          .get("work-items/board", {
            searchParams: Object.fromEntries(
              Object.entries(params).filter(([, v]) => v !== undefined),
            ),
          })
          .json<BoardResponse>(),
    }),
  formatForLlm: (params: {
    organization: string;
    project: string;
    workItemId: string;
  }) =>
    queryOptions({
      queryKey: [...workItemsQueries.baseKey, "formatForLlm", params],
      queryFn: async () =>
        api
          .get("work-items/format-for-llm", { searchParams: params })
          .json<FormatForLlmResponse>(),
    }),
};
