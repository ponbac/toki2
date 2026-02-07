import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const searchQueries = {
  baseKey: ["search"],
  search: (query: string, limit?: number) => {
    const trimmedQuery = query.trim();
    return queryOptions({
      queryKey: [...searchQueries.baseKey, trimmedQuery, limit],
      queryFn: async () => {
        const params = new URLSearchParams({ q: trimmedQuery });
        if (limit) {
          params.set("limit", limit.toString());
        }
        return api.get(`search?${params}`).json<Array<SearchResult>>();
      },
      enabled: trimmedQuery.length >= 2,
    });
  },
};

export type SearchSource = "Pr" | "WorkItem";

export type SearchResult = {
  id: number;
  sourceType: SearchSource;
  sourceId: string;
  externalId: number;
  title: string;
  description: string | null;
  status: string;
  priority: number | null;
  itemType: string | null;
  authorName: string | null;
  url: string;
  createdAt: string;
  updatedAt: string;
  score: number;
};
