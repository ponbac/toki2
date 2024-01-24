import { useMutation, useQuery } from "@tanstack/react-query";

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

type RepoKey = {
  organization: string;
  project: string;
  repoName: string;
};

const hexagonRepoKey: RepoKey = {
  organization: "ex-change-part",
  project: "Quote Manager",
  repoName: "hexagon",
};

export function Home() {
  const { data: differs, refetch } = useQuery({
    queryKey: ["differs"],
    queryFn: () =>
      fetch("http://localhost:8000/differs").then((res) =>
        res.json(),
      ) as Promise<Differ[]>,
    refetchInterval: 30 * 1000,
  });

  const { data: cachedPullRequests } = useQuery({
    queryKey: ["cachedPullRequests"],
    queryFn: () =>
      fetch(
        `http://localhost:8000/pull-requests/cached?organization=${hexagonRepoKey.organization}&project=${hexagonRepoKey.project}&repoName=${hexagonRepoKey.repoName}`,
      ).then((res) => res.json()),
    refetchInterval: 30 * 1000,
  });

  const { mutate: startDiffer } = useMutation({
    mutationFn: (repoKey: RepoKey) =>
      fetch("http://localhost:8000/differs/start", {
        method: "POST",
        body: JSON.stringify(repoKey),
        headers: {
          "Content-Type": "application/json",
        },
      }),
    onSuccess: () => {
      refetch();
    },
  });

  const { mutate: stopDiffer } = useMutation({
    mutationFn: (repoKey: RepoKey) =>
      fetch("http://localhost:8000/differs/stop", {
        method: "POST",
        body: JSON.stringify(repoKey),
        headers: {
          "Content-Type": "application/json",
        },
      }),
    onSuccess: () => {
      refetch();
    },
  });

  return (
    <div className="">
      <h3>Welcome Home!</h3>
      <div className="flex flex-col gap-4">
        {differs?.map((differ) => (
          <div key={differ.repoName}>
            <div>{differ.organization}</div>
            <div>{differ.project}</div>
            <div>{differ.repoName}</div>
            <div>{differ.status}</div>
            <div>
              Latest fetch:{" "}
              {differ.lastUpdated
                ? Intl.DateTimeFormat("sv-SE", {
                    year: "numeric",
                    month: "numeric",
                    day: "numeric",
                    hour: "numeric",
                    minute: "numeric",
                    second: "numeric",
                  }).format(new Date(differ.lastUpdated))
                : "Never"}
            </div>
            {differ.status === "Running" && (
              <div>{differ.refreshInterval?.secs} seconds refresh</div>
            )}
          </div>
        ))}
        <button
          className="w-36 rounded bg-blue-500 px-4 py-2 font-bold text-white hover:bg-blue-700"
          onClick={() => startDiffer(hexagonRepoKey)}
        >
          Start
        </button>
        <button
          className="w-36 rounded bg-orange-500 px-4 py-2 font-bold text-white hover:bg-orange-700"
          onClick={() => stopDiffer(hexagonRepoKey)}
        >
          Stop
        </button>
        {cachedPullRequests?.length > 0 && (
          <pre>{JSON.stringify(cachedPullRequests, null, 2)}</pre>
        )}
      </div>
    </div>
  );
}
