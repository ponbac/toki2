import { Button, ButtonProps } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { mutations } from "@/lib/api/mutations/mutations";
import { Differ } from "@/lib/api/queries/differs";
import { queries } from "@/lib/api/queries/queries";
import { cn, toRepoKeyString } from "@/lib/utils";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import dayjs from "dayjs";
import { PauseCircle, PlayCircle } from "lucide-react";

export const Route = createFileRoute("/_layout/repositories")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.differs()),
  component: RepositoriesComponent,
});

function RepositoriesComponent() {
  const { data, dataUpdatedAt } = useSuspenseQuery({
    ...queries.differs(),
    refetchInterval: 15 * 1000,
  });

  const { mutate: startDiffer, isPending: isStarting } =
    mutations.useStartDiffers();
  const { mutate: stopDiffer, isPending: isStopping } =
    mutations.useStopDiffers();

  return (
    <main className="flex min-h-screen w-full flex-col items-center justify-center gap-4">
      <div className="grid grid-cols-3 gap-4">
        {data.map((differ) => (
          <Card
            key={`${toRepoKeyString(differ)}-${dataUpdatedAt}`}
            className="flex min-w-[25rem] flex-col justify-between"
          >
            <CardHeader>
              <CardTitle>{differ.repoName}</CardTitle>
              <CardDescription>{`${differ.organization}/${differ.project}`}</CardDescription>
            </CardHeader>
            <CardContent>
              <CardDescription>
                Status: <span className="font-semibold">{differ.status}</span>
              </CardDescription>
              {differ.status === "Running" && (
                <CardDescription>
                  Fetch Interval:{" "}
                  {differ.refreshInterval
                    ? `${differ.refreshInterval.secs} seconds`
                    : "None"}
                </CardDescription>
              )}
              <LastUpdated differ={differ} />
            </CardContent>
            <CardFooter className="flex flex-row-reverse gap-2">
              <FooterButton
                disabled={differ.status === "Running" || isStarting}
                onClick={() => startDiffer(differ)}
              >
                <PlayCircle size="1.25rem" />
                Start
              </FooterButton>
              <FooterButton
                variant="outline"
                disabled={differ.status === "Stopped" || isStopping}
                onClick={() => stopDiffer(differ)}
              >
                <PauseCircle size="1.25rem" />
                Stop
              </FooterButton>
            </CardFooter>
          </Card>
        ))}
      </div>
    </main>
  );
}

function LastUpdated({ differ }: { differ: Differ }) {
  const nMinutesAgo = differ.lastUpdated
    ? dayjs().diff(dayjs(differ.lastUpdated), "minute")
    : undefined;

  return (
    <CardDescription>
      Updated:{" "}
      {nMinutesAgo === undefined
        ? "Never"
        : nMinutesAgo < 1
          ? "Just now"
          : nMinutesAgo === 1
            ? "1 minute ago"
            : `${nMinutesAgo} minutes ago`}
    </CardDescription>
  );
}

function FooterButton({ className, ...rest }: Omit<ButtonProps, "size">) {
  return (
    <Button
      size="sm"
      className={cn(
        "w-18 flex items-center gap-1.5 transition-colors",
        className,
      )}
      {...rest}
    />
  );
}
