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
  PullRequest,
  Thread as PullRequestThread,
  User,
} from "@/lib/api/queries/pullRequests";
import { queries } from "@/lib/api/queries/queries";
import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import dayjs from "dayjs";
import {
  ClipboardCopy,
  CodeXmlIcon,
  MessageCircleCodeIcon,
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

export const Route = createFileRoute("/_layout/prs/$prId")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.cachedPullRequests()),
  component: PRDetailsDialog,
});

function PRDetailsDialog() {
  const { prId } = Route.useParams();
  const parentSearch = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });

  const { data: pr } = useSuspenseQuery({
    ...queries.cachedPullRequests(),
    select: (data) => data.find((pr) => pr.id === +prId),
  });

  const copyTimeReportTextToClipboard = (mode: "review" | "develop") => {
    let text = "";
    const workItem = pr?.workItems.at(0);
    if (!workItem) {
      text = `!${prId} - ${mode === "review" ? "[CR] " : ""}${pr?.title}`;
    } else {
      const parentWorkItem = workItem.parentId;
      text = `${parentWorkItem ? `#${parentWorkItem} ` : ""}#${workItem.id} - ${mode === "review" ? "[CR] " : ""}${workItem.title}`;
    }

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
          <Button
            autoFocus
            variant="outline"
            size="sm"
            className="flex gap-2"
            onClick={() => copyTimeReportTextToClipboard("review")}
          >
            <MessageCircleCodeIcon className="size-4" />
            Review
          </Button>
          <Button
            autoFocus
            variant="default"
            size="sm"
            className="flex gap-2"
            onClick={() => copyTimeReportTextToClipboard("develop")}
          >
            <CodeXmlIcon className="size-4" />
            Develop
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Header(props: { pullRequest: PullRequest }) {
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

function Threads(props: { pullRequest: PullRequest }) {
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
