import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const milltimeQueries = {
  listProjects: () =>
    queryOptions({
      queryKey: ["milltime", "projects"],
      queryFn: async () =>
        api
          .get("milltime/projects")
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          .json<Array<any>>(),
    }),
};
