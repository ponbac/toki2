import { Button, ButtonProps } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { mutations } from "@/lib/api/mutations/mutations";
import { Differ } from "@/lib/api/queries/differs";
import { cn } from "@/lib/utils";
import { useNavigate } from "@tanstack/react-router";
import dayjs from "dayjs";
import {
  BellIcon,
  Heart,
  PauseCircle,
  PlayCircle,
  Trash,
  Unplug,
  Timer,
} from "lucide-react";
import { toast } from "sonner";
import { useSuspenseQuery } from "@tanstack/react-query";
import { queries } from "@/lib/api/queries/queries";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

export const RepoCard = (props: {
  differ: Differ;
  isAdmin: boolean | undefined;
}) => {
  const navigate = useNavigate();

  const { mutate: startDiffer } = mutations.useStartDiffers();
  const { mutate: stopDiffer } = mutations.useStopDiffers();
  const { mutate: followRepository } = mutations.useFollowRepository({
    onSuccess: (_, vars) => {
      toast.success(
        vars.follow
          ? `You are now following ${vars.repoName}.`
          : `You are no longer following ${vars.repoName}.`,
      );
    },
  });
  const { mutate: deleteRepository, isPending: isDeleting } =
    mutations.useDeleteRepository();

  const { data: projects } = useSuspenseQuery(queries.listProjects());
  const { mutate: updateMilltimeProject } = mutations.useUpdateMilltimeProject({
    onSuccess: () => {
      toast.success("Milltime project updated successfully");
    },
  });

  return (
    <Card
      className={cn(
        "flex h-56 w-[25rem] flex-col justify-between",
        props.differ.isInvalid && "border border-destructive",
        !props.isAdmin && "h-44",
      )}
    >
      <CardHeader className="flex w-full flex-row items-start justify-between">
        <div className="flex flex-col gap-1 overflow-hidden">
          <CardTitle className="truncate leading-6">
            {props.differ.repoName}
          </CardTitle>
          <CardDescription className="leading-4">{`${props.differ.organization}/${props.differ.project}`}</CardDescription>
        </div>
        <div className="flex items-center gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  disabled={!props.differ.followed}
                  onClick={() =>
                    navigate({
                      to: `/repositories/notifications/${props.differ.repoId}`,
                    })
                  }
                >
                  <BellIcon />
                </Button>
              </span>
            </TooltipTrigger>
            <TooltipContent>
              {props.differ.followed
                ? "Manage notifications"
                : "You can only manage notifications for followed repositories"}
            </TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="group size-8"
                onClick={() =>
                  followRepository({
                    ...props.differ,
                    follow: !props.differ.followed,
                  })
                }
              >
                {props.differ.followed ? (
                  <Unplug size="1.25rem" />
                ) : (
                  <Heart size="1.25rem" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {props.differ.followed
                ? "Unfollow repository"
                : "Follow repository"}
            </TooltipContent>
          </Tooltip>
        </div>
      </CardHeader>
      <CardContent>
        <CardDescription>
          Status:{" "}
          <span
            className={cn("font-semibold", {
              "text-destructive": props.differ.status === "Errored",
            })}
          >
            {props.differ.status}
          </span>
        </CardDescription>
        {props.differ.isInvalid ? (
          <CardDescription>
            Could not create an Azure DevOps connection. Add the repository to
            Toki again with a new PAT.
          </CardDescription>
        ) : (
          <>
            <CardDescription>
              Fetch Interval:{" "}
              {props.differ.refreshInterval
                ? `${props.differ.refreshInterval.secs} seconds`
                : "None"}
            </CardDescription>
            <LastUpdated differ={props.differ} />
            <div className="flex items-center gap-2 mt-2">
              <Timer className="size-4" />
              <Select
                value={props.differ.milltimeProjectId}
                onValueChange={(value) =>
                  updateMilltimeProject({
                    ...props.differ,
                    milltimeProjectId: value,
                  })
                }
              >
                <SelectTrigger className="w-[180px]">
                  <SelectValue placeholder="Select Milltime project" />
                </SelectTrigger>
                <SelectContent>
                  {projects.map((project) => (
                    <SelectItem key={project.projectId} value={project.projectId}>
                      {project.projectName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </>
        )}
      </CardContent>
      {props.isAdmin && (
        <CardFooter className="flex flex-row-reverse gap-2">
          <FooterButton
            disabled={
              props.differ.status === "Running" || props.differ.isInvalid
            }
            onClick={() => startDiffer(props.differ)}
          >
            <PlayCircle size="1.25rem" />
            Start
          </FooterButton>
          <FooterButton
            variant="outline"
            disabled={
              props.differ.status === "Stopped" || props.differ.isInvalid
            }
            onClick={() => stopDiffer(props.differ)}
          >
            <PauseCircle size="1.25rem" />
            Stop
          </FooterButton>
          <FooterButton
            variant="outline"
            onClick={() =>
              deleteRepository({
                organization: props.differ.organization,
                project: props.differ.project,
                repoName: props.differ.repoName,
              })
            }
            className="mr-auto transition-colors hover:text-destructive"
            disabled={props.differ.status === "Running" || isDeleting}
          >
            <Trash size="1.25rem" />
          </FooterButton>
        </CardFooter>
      )}
    </Card>
  );
};

function LastUpdated(props: { differ: Differ }) {
  const nMinutesAgo = props.differ.lastUpdated
    ? dayjs().diff(dayjs(props.differ.lastUpdated), "minute")
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
