import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import {
  parseAiMappingProjectList,
  parseAiUsageMachineList,
  parseAiUsageProjectKeyList,
  parseAiUsageReport,
  parseAiUsageSessionList,
  type AiSessionSort,
  type AiUsageReportParams,
} from "../contracts/ai-usage-report";

export type {
  AiCost,
  AiCoverageStatus,
  AiProjectAttribution,
  AiProjectMapping,
  AiSessionSort,
  AiTokenCounts,
  AiUsageMachine,
  AiUsageMachineList,
  AiUsageProject,
  AiUsageProjectKeyList,
  AiUsageReport,
  AiUsageReportParams,
  AiUsageSession,
  AiUsageSessionList,
  AiUsageTotals,
} from "../contracts/ai-usage-report";

/** Query keys of the current user's own usage; mappings change all of them. */
export const aiUsageReportQueryKeys = {
  all: ["ai-usage", "personal"] as const,
  report: (params: AiUsageReportParams) =>
    ["ai-usage", "personal", "report", params] as const,
  sessions: (
    params: AiUsageReportParams,
    sort: AiSessionSort,
    pageSize: number,
  ) => ["ai-usage", "personal", "sessions", params, sort, pageSize] as const,
  machines: (range: AiUsageRange) =>
    ["ai-usage", "personal", "machines", range] as const,
  projectKeys: ["ai-usage", "personal", "project-keys"] as const,
  mappingProjects: ["ai-usage", "project-mappings", "projects"] as const,
};

/**
 * Local dates whose usage each machine reports; absent bounds default on the
 * server to the current month, as for a report.
 */
export type AiUsageRange = Readonly<{ from?: string; to?: string }>;

/** The query string of a report or session listing, without absent values. */
function searchParams(
  params: AiUsageReportParams,
  extra: Readonly<Record<string, string>> = {},
): URLSearchParams {
  const search = new URLSearchParams();
  const entries: ReadonlyArray<readonly [string, string | undefined]> = [
    ["from", params.from],
    ["to", params.to],
    ["provider", params.provider],
    ["model", params.model],
    ["projectId", params.projectId],
    ["unassigned", params.unassigned ? "true" : undefined],
    ["machineId", params.machineId],
  ];
  for (const [name, value] of entries) {
    if (value !== undefined) {
      search.set(name, value);
    }
  }
  for (const [name, value] of Object.entries(extra)) {
    search.set(name, value);
  }
  return search;
}

/** Query definitions for the current user's own AI usage. */
export const aiUsageReportQueries = {
  aiUsageReport: (params: AiUsageReportParams) =>
    queryOptions({
      queryKey: aiUsageReportQueryKeys.report(params),
      queryFn: async () =>
        parseAiUsageReport(
          await api
            .get("ai-usage/report", { searchParams: searchParams(params) })
            .json<unknown>(),
        ),
    }),
  /**
   * The sessions of a report's range and filters, `pageSize` at a time: each
   * page continues after the previous page's cursor, so loading more never
   * fetches the earlier pages again.
   */
  aiUsageSessions: (
    params: AiUsageReportParams,
    sort: AiSessionSort,
    pageSize: number,
  ) =>
    infiniteQueryOptions({
      queryKey: aiUsageReportQueryKeys.sessions(params, sort, pageSize),
      // The first page has no cursor to continue after: "".
      queryFn: async ({ pageParam }) =>
        parseAiUsageSessionList(
          await api
            .get("ai-usage/sessions", {
              searchParams: searchParams(
                params,
                pageParam === ""
                  ? { sort, limit: String(pageSize) }
                  : { sort, limit: String(pageSize), after: pageParam },
              ),
            })
            .json<unknown>(),
        ),
      initialPageParam: "",
      getNextPageParam: (page) => page.nextCursor ?? undefined,
    }),
  /** The user's machines, with all their usage in `range`. */
  aiUsageMachines: (range: AiUsageRange) =>
    queryOptions({
      queryKey: aiUsageReportQueryKeys.machines(range),
      queryFn: async () =>
        parseAiUsageMachineList(
          await api
            .get("ai-usage/machines", {
              searchParams: searchParams({ from: range.from, to: range.to }),
            })
            .json<unknown>(),
        ),
    }),
  aiUsageProjectKeys: () =>
    queryOptions({
      queryKey: aiUsageReportQueryKeys.projectKeys,
      queryFn: async () =>
        parseAiUsageProjectKeyList(
          await api.get("ai-usage/project-keys").json<unknown>(),
        ),
    }),
  /** Active time-tracking projects a project key can be mapped to. */
  aiMappingProjects: () =>
    queryOptions({
      queryKey: aiUsageReportQueryKeys.mappingProjects,
      queryFn: async () =>
        parseAiMappingProjectList(
          await api.get("ai-usage/project-mappings/projects").json<unknown>(),
        ),
      staleTime: 5 * 60 * 1000,
    }),
};
