import { createFileRoute } from "@tanstack/react-router";
import { queries } from "@/lib/api/queries/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { PRDetailsDialog } from "../-components/pr-details-dialog";

export const Route = createFileRoute("/_layout/prs/$prId")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.listPullRequests()),
  component: PRDetailsDialog,
});

function PRDetailsDialog() {
  const { prId } = Route.useParams();
  const parentSearch = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });

  const { data: pr } = useSuspenseQuery({
    ...queries.listPullRequests(),
    select: (data) => data.find((pr) => pr.id === +prId),
  });

  const { data: differs } = useSuspenseQuery(queries.differs());
  const { data: allProjects } = useSuspenseQuery(queries.listProjects());
  const [selectedProject, setSelectedProject] = React.useState<string | null>(null);
  const [selectedActivity, setSelectedActivity] = React.useState<string | null>(null);
  const [timeReportMode, setTimeReportMode] = React.useState<"review" | "develop" | null>(null);

  const differ = differs.find(d => d.repoName === pr?.repoName);
  const connectedProjects = allProjects.filter(p => differ?.milltimeProjectIds.includes(p.projectId));

  const { data: activities } = useSuspenseQuery({
    ...queries.listActivities(selectedProject ?? ""),
    enabled: !!selectedProject,
  });

  const { mutate: startTimer } = mutations.useStartTimer({
    onSuccess: () => {
      toast.success("Timer started successfully");
      setSelectedProject(null);
      setSelectedActivity(null);
      setTimeReportMode(null);
    },
  });

  const getTimeReportText = (mode: "review" | "develop") => {
    let text = "";
    const workItem = pr?.workItems.at(0);
    if (!workItem) {
      text = `!${prId} - ${mode === "review" ? "[CR] " : ""}${pr?.title}`;
    } else {
      const parentWorkItem = workItem.parentId;
      text = `${parentWorkItem ? `#${parentWorkItem} ` : ""}#${workItem.id} - ${mode === "review" ? "[CR] " : ""}${workItem.title}`;
    }
    return text;
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.info(
      <div className="flex flex-row items-center">
        <ClipboardCopy className="mr-2 inline-block" size="1.25rem" />
        <p className="text-pretty">
          Copied <span className="font-mono">{text}</span> to clipboard
        </p>
      </div>,
    );
  };

  const startTimeReport = () => {
    if (!timeReportMode || !selectedProject || !selectedActivity) return;

    const text = getTimeReportText(timeReportMode);
    copyToClipboard(text);

    const project = projects.find(p => p.projectId === selectedProject);
    if (!project) {
      toast.error("Project not found");
      return;
    }

    const activity = activities?.find(a => a.activity === selectedActivity);
    if (!activity) {
      toast.error("Activity not found");
      return;
    }

    startTimer({
      projectId: project.projectId,
      projectName: project.projectName,
      activity: activity.activity,
      activityName: activity.activityName,
      userNote: text,
      regDay: dayjs().format("YYYY-MM-DD"),
      weekNumber: dayjs().week(),
    });
  };

  if (!pr) {
    return null;
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          navigate({ to: "..", search: parentSearch });
        }
      }}
    >
      <DialogContent className="max-w-5xl">
        <Header pullRequest={pr} />
        <Threads pullRequest={pr} />
        <DialogFooter className="pt-2">
          <PRNotificationSettings pullRequest={pr} />
          <Popover open={timeReportMode === "review"} onOpenChange={(open) => {
            if (open) {
              setTimeReportMode("review");
            } else {
              setTimeReportMode(null);
              setSelectedProject(null);
              setSelectedActivity(null);
            }
          }}>
            <PopoverTrigger asChild>
              <Button
                autoFocus
                variant="outline"
                size="sm"
                className="flex gap-2"
              >
                <MessageCircleCodeIcon className="size-4" />
                Review
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-80">
              <div className="flex flex-col gap-4">
                <div className="flex flex-col gap-2">
                  <h4 className="font-medium">Select Project</h4>
                  <Select
                    value={selectedProject ?? undefined}
                    onValueChange={(value) => {
                      setSelectedProject(value);
                      setSelectedActivity(null);
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select a project" />
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
                {selectedProject && (
                  <div className="flex flex-col gap-2">
                    <h4 className="font-medium">Select Activity</h4>
                    <Select
                      value={selectedActivity ?? undefined}
                      onValueChange={setSelectedActivity}
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select an activity" />
                      </SelectTrigger>
                      <SelectContent>
                        {activities?.map((activity) => (
                          <SelectItem key={activity.activity} value={activity.activity}>
                            {activity.activityName}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                )}
                <Button
                  disabled={!selectedProject || !selectedActivity}
                  onClick={startTimeReport}
                >
                  Start Timer
                </Button>
              </div>
            </PopoverContent>
          </Popover>
          <Popover open={timeReportMode === "develop"} onOpenChange={(open) => {
            if (open) {
              setTimeReportMode("develop");
            } else {
              setTimeReportMode(null);
              setSelectedProject(null);
              setSelectedActivity(null);
            }
          }}>
            <PopoverTrigger asChild>
              <Button
                autoFocus
                variant="default"
                size="sm"
                className="flex gap-2"
              >
                <CodeXmlIcon className="size-4" />
                Develop
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-80">
              <div className="flex flex-col gap-4">
                <div className="flex flex-col gap-2">
                  <h4 className="font-medium">Select Project</h4>
                  <Select
                    value={selectedProject ?? undefined}
                    onValueChange={(value) => {
                      setSelectedProject(value);
                      setSelectedActivity(null);
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select a project" />
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
                {selectedProject && (
                  <div className="flex flex-col gap-2">
                    <h4 className="font-medium">Select Activity</h4>
                    <Select
                      value={selectedActivity ?? undefined}
                      onValueChange={setSelectedActivity}
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select an activity" />
                      </SelectTrigger>
                      <SelectContent>
                        {activities?.map((activity) => (
                          <SelectItem key={activity.activity} value={activity.activity}>
                            {activity.activityName}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                )}
                <Button
                  disabled={!selectedProject || !selectedActivity}
                  onClick={startTimeReport}
                >
                  Start Timer
                </Button>
              </div>
            </PopoverContent>
          </Popover>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Header(props: { pullRequest: ListPullRequest }) {
  const { createdBy, sourceBranch, targetBranch, title } = props.pullRequest;

  return (
    <DialogHeader>
      <DialogTitle className="flex flex-row items-center gap-2">
        <AzureAvatar user={createdBy} className="size-8" />
        <PRLink data={props.pullRequest}>
          <h1 className="text-xl font-semibold">{title}</h1>
        </PRLink>
      </DialogTitle>
      <DialogDescription>
        <BranchLink sourceBranch={sourceBranch} targetBranch={targetBranch} />
      </DialogDescription>
    </DialogHeader>
  );
}

function Threads(props: { pullRequest: ListPullRequest }) {
  const [showResolved, setShowResolved] = React.useState(false);

  const threads = props.pullRequest.threads;
  const allUsers = props.pullRequest.reviewers.map((r) => r.identity);

  const activeThreads = threads.filter((t) => t.status === "active");
  const resolvedThreads = threads.filter(
    (t) =>
      (t.status === "closed" || t.status === "fixed" || t.status === null) &&
      t.comments.at(0)?.commentType !== "system",
  );

  return (
    <ScrollArea className="max-h-[60vh] max-w-5xl">
      <div className="flex flex-col">
        {activeThreads.map((thread) => (
          <Thread key={thread.id} thread={thread} users={allUsers} />
        ))}
        {resolvedThreads.length > 0 && (
          <div className="flex w-full flex-col items-center pt-2">
            <Button
              variant="link"
              size="sm"
              className="flex w-full gap-2"
              onClick={() => setShowResolved(!showResolved)}
            >
              {showResolved ? "Hide" : "Show"} resolved threads{" "}
              {!showResolved ? `(${resolvedThreads.length} hidden)` : ""}
            </Button>
            {showResolved &&
              resolvedThreads.map((thread) => (
                <Thread key={thread.id} thread={thread} users={allUsers} />
              ))}
          </div>
        )}
      </div>
    </ScrollArea>
  );
}

function Thread(props: { thread: PullRequestThread; users: Array<User> }) {
  const nonDeletedComments = props.thread.comments
    .filter((c) => !c.isDeleted)
    .map((c) => ({
      ...c,
      content: replaceMentionsWithUsernames(c.content, props.users),
    }));

  const firstComment = nonDeletedComments.at(0);

  if (!firstComment) {
    return null;
  }

  return (
    <Accordion type="single" collapsible className="w-full">
      <AccordionItem value={firstComment.id.toString()}>
        <AccordionTrigger>
          <div className="flex max-w-[58rem] flex-col">
            <div className="flex flex-row items-center gap-2">
              <AzureAvatar
                user={firstComment.author}
                className="size-6"
                disableTooltip
              />
              <h1>
                {firstComment.author.displayName}{" "}
                <span className="text-sm text-muted-foreground">
                  {dayjs(firstComment.publishedAt).format("YYYY-MM-DD HH:mm")}
                </span>
              </h1>
            </div>
            <article className="prose-sm truncate text-left dark:prose-invert">
              <Markdown>{firstComment.content.split("\n").at(0)}</Markdown>
            </article>
          </div>
        </AccordionTrigger>
        <AccordionContent className="flex flex-col gap-4">
          {nonDeletedComments.map((comment) => (
            <div key={comment.id} className="flex flex-col gap-2">
              <div
                key={comment.id}
                className="flex flex-row items-center gap-2"
              >
                <AzureAvatar
                  user={comment.author}
                  className="size-6"
                  disableTooltip
                />
                <h1>
                  {comment.author.displayName}{" "}
                  <span className="text-sm text-muted-foreground">
                    {dayjs(comment.publishedAt).format("YYYY-MM-DD HH:mm")}
                  </span>
                </h1>
              </div>
              <article className="prose max-w-[80ch] dark:prose-invert">
                <Markdown>{comment.content}</Markdown>
              </article>
            </div>
          ))}
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

// @<23770AE1-E35F-613D-91B9-9BCC85CC5CE8> replace these mentions with display names
function replaceMentionsWithUsernames(text: string, users: Array<User>) {
  return text.replace(/@<([A-F0-9-]+)>/g, (match, userId) => {
    const user = users.find((u) => u.id.toUpperCase() === userId);
    return user ? `*@${user.displayName}*` : match;
  });
}
