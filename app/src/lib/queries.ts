import { router } from "@/main";
import { queryOptions } from "@tanstack/react-query";

export const queries = {
  differs: () =>
    queryOptions({
      queryKey: ["differs"],
      queryFn: async () => {
        const response = await fetch("http://localhost:8000/differs", {
          credentials: "include",
        });
        console.log(response);

        if (response.redirected || response.status !== 200) {
          router.history.push(
            `/login?next=${router.history.location.pathname}`,
          );
          return Promise.reject("Unauthorized");
        }

        return response.json() as Promise<Differ[]>;
      },
      refetchInterval: 30 * 1000,
    }),
};

type Differ = {
  organization: string;
  project: string;
  repoName: string;
  status: "Running" | "Stopped";
  lastUpdated: string | null;
  refreshInterval: {
    secs: number;
    nanos: number;
  } | null;
};
