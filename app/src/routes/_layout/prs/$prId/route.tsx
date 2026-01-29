import { AzureAvatar } from "@/components/azure-avatar";
import BranchLink from "@/components/branch-link";
import { PRLink } from "@/components/pr-link";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  ListPullRequest,
  Thread as PullRequestThread,
  User,
} from "@/lib/api/queries/pullRequests";
import { queries } from "@/lib/api/queries/queries";
import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import dayjs from "dayjs";
import {
  ClipboardCopy,
  CodeXmlIcon,
  MessageCircleCodeIcon,
  TimerIcon,
} from "lucide-react";
import { toast } from "sonner";
import Markdown from "react-markdown";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import React from "react";
import { PRNotificationSettings } from "../-components/pr-notification-settings";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { milltimeMutations } from "@/lib/api/mutations/milltime";

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

  // Timer state and mutations
  const { data: timerResponse, isSuccess: timerQuerySuccess } = useQuery({
    ...milltimeQueries.getTimer(),
    retry: false,
  });
  const timer = timerResponse?.timer;

  const { mutateAsync: startStandaloneTimer } =
    milltimeMutations.useStartStandaloneTimer();
  const { mutateAsync: editStandaloneTimer } =
    milltimeMutations.useEditStandaloneTimer();

  const buildTimeReportText = (mode: "review" | "develop") => {
    const workItem = pr?.workItems.at(0);
    if (!workItem) {
      return `!${prId} - ${mode === "review" ? "[CR] " : ""}${pr?.title}`;
    }
    const parentWorkItem = workItem.parentId;
    return `${parentWorkItem ? `#${parentWorkItem} ` : ""}#${workItem.id} - ${mode === "review" ? "[CR] " : ""}${workItem.title}`;
  };

  const handleTimeReportClick = async (mode: "review" | "develop") => {
    const text = buildTimeReportText(mode);

    // Always copy to clipboard
    navigator.clipboard.writeText(text);
    toast.info(
      <div className="flex flex-row items-center">
        <ClipboardCopy className="mr-2 inline-block" size="1.25rem" />
        <p className="text-pretty">
          Copied <span className="font-mono">{text}</span> to clipboard
        </p>
      </div>,
    );

    // Only proceed with timer operations if query succeeded
    if (!timerQuerySuccess) return;

    try {
      if (timer) {
        // Update existing timer note
        await editStandaloneTimer({ userNote: text });
        toast.success(
          <div className="flex flex-row items-center">
            <TimerIcon className="mr-2 inline-block" size="1.25rem" />
            Timer note updated
          </div>,
        );
      } else {
        // No active timer - start a new timer
        await startStandaloneTimer({ userNote: text });
        toast.success(
          <div className="flex flex-row items-center">
            <TimerIcon className="mr-2 inline-block" size="1.25rem" />
            Timer started
          </div>,
        );
      }
    } catch {
      // Silently fail - clipboard copy already succeeded
    }
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
          <Button
            autoFocus
            variant="outline"
            size="sm"
            className="flex gap-2"
            onClick={() => {
              handleTimeReportClick("review");
              navigate({ to: "..", search: parentSearch });
            }}
          >
            <MessageCircleCodeIcon className="size-4" />
            Review
          </Button>
          <Button
            autoFocus
            variant="default"
            size="sm"
            className="flex gap-2"
            onClick={() => {
              handleTimeReportClick("develop");
              navigate({ to: "..", search: parentSearch });
            }}
          >
            <CodeXmlIcon className="size-4" />
            Develop
          </Button>
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
      content: mdFormatMentions(c.content),
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

// @<Pontus Backman> -> ***@Pontus Backman***
function mdFormatMentions(text: string) {
  return text.replace(/@<([^>]+)>/g, "***@$1***");
}
