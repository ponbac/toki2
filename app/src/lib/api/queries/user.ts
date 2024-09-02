import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const userQueries = {
  me: () =>
    queryOptions({
      queryKey: ["me"],
      queryFn: () => api.get("me").json<User>(),
    }),
};

export type User = {
  id: number;
  email: string;
  fullName: string;
  picture: string;
  accessToken: string;
};
