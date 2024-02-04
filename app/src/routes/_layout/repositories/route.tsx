import { Button, ButtonProps, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { mutations } from "@/lib/api/mutations/mutations";
import { Differ } from "@/lib/api/queries/differs";
import { queries } from "@/lib/api/queries/queries";
import { cn, toRepoKeyString } from "@/lib/utils";
import { useSuspenseQuery } from "@tanstack/react-query";
import {
  Link,
  Outlet,
  createFileRoute,
  useNavigate,
} from "@tanstack/react-router";
import dayjs from "dayjs";
import {
  Heart,
  PauseCircle,
  PlayCircle,
  Plus,
  SearchCode,
  Unplug,
} from "lucide-react";
import { useRef } from "react";
import { toast } from "sonner";
import { z } from "zod";

const repositoriesSearchSchema = z.object({
  searchString: z.string().optional().catch(""),
});

export const Route = createFileRoute("/_layout/repositories")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.differs()),
  validateSearch: (search) => repositoriesSearchSchema.parse(search),
  component: RepositoriesComponent,
});

function RepositoriesComponent() {
  const { searchString } = Route.useSearch();

  const { data, dataUpdatedAt } = useSuspenseQuery({
    ...queries.differs(),
    refetchInterval: 15 * 1000,
  });

  const { mutate: startDiffer, isPending: startingPending } =
    mutations.useStartDiffers();
  const { mutate: stopDiffer, isPending: stoppingPending } =
    mutations.useStopDiffers();
  const { mutate: followRepository } = mutations.useFollowRepository({
    onSuccess: (_, vars) => {
      toast.success(
        vars.follow
          ? `You are now following ${vars.repoName}.`
          : `You are no longer following ${vars.repoName}.`,
      );
    },
  });

  const filteredData = data.filter((differ) =>
    toRepoKeyString(differ)
      .toLowerCase()
      .includes(searchString?.toLowerCase() ?? ""),
  );

  return (
    <main className="flex w-full items-center justify-center">
      <div className="flex min-w-[77rem] flex-col items-center justify-center gap-4">
        <TopBar />
        <div className="grid grid-cols-3 gap-4">
          {filteredData.map((differ) => (
            <Card
              key={`${toRepoKeyString(differ)}-${dataUpdatedAt}`}
              className="flex w-[25rem] flex-col justify-between"
            >
              <CardHeader className="flex w-full flex-row items-start justify-between">
                <div className="flex flex-col gap-1">
                  <CardTitle>{differ.repoName}</CardTitle>
                  <CardDescription>{`${differ.organization}/${differ.project}`}</CardDescription>
                </div>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="group size-8"
                      onClick={() =>
                        followRepository({
                          ...differ,
                          follow: !differ.followed,
                        })
                      }
                    >
                      {differ.followed ? (
                        <Unplug size="1.25rem" />
                      ) : (
                        <Heart size="1.25rem" />
                      )}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {differ.followed
                      ? "Unfollow repository"
                      : "Follow repository"}
                  </TooltipContent>
                </Tooltip>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Status: <span className="font-semibold">{differ.status}</span>
                </CardDescription>
                <CardDescription>
                  Fetch Interval:{" "}
                  {differ.refreshInterval
                    ? `${differ.refreshInterval.secs} seconds`
                    : "None"}
                </CardDescription>
                <LastUpdated differ={differ} />
              </CardContent>
              <CardFooter className="flex flex-row-reverse gap-2">
                <FooterButton
                  disabled={differ.status === "Running" || startingPending}
                  onClick={() => startDiffer(differ)}
                >
                  <PlayCircle size="1.25rem" />
                  Start
                </FooterButton>
                <FooterButton
                  variant="outline"
                  disabled={differ.status === "Stopped" || stoppingPending}
                  onClick={() => stopDiffer(differ)}
                >
                  <PauseCircle size="1.25rem" />
                  Stop
                </FooterButton>
              </CardFooter>
            </Card>
          ))}
        </div>
      </div>
      <Outlet />
    </main>
  );
}

function TopBar() {
  const navigate = useNavigate({ from: Route.fullPath });
  const { searchString } = Route.useSearch();

  const inputRef = useRef<HTMLInputElement>(null);

  return (
    <div className="flex w-full flex-col gap-2">
      <div>
        <h1 className="text-2xl font-bold">Repositories</h1>
        <h2 className="text-muted-foreground">
          Follow the repositories you want to keep an eye on by clicking the
          heart.
        </h2>
      </div>
      <div className="flex gap-4">
        <div className="relative flex w-full items-center">
          <SearchCode
            onClick={() => inputRef.current?.focus()}
            className="absolute left-2 top-1/2 size-4 -translate-y-1/2 transform cursor-pointer"
          />
          <Input
            ref={inputRef}
            placeholder="Search connected repositories..."
            value={searchString ?? ""}
            onChange={(event) => {
              const value = event.target.value;
              navigate({
                search: (prev) => ({
                  ...prev,
                  searchString: value.length ? event.target.value : undefined,
                }),
              });
            }}
            className="pl-8"
          />
        </div>
        <Link
          to="/repositories/add"
          className={cn(buttonVariants({ variant: "outline" }), "gap-1")}
        >
          <Plus size="1.25rem" />
          Add repository
        </Link>
      </div>
    </div>
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
