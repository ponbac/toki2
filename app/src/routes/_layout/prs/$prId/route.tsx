import { AzureAvatar } from "@/components/azure-avatar";
import BranchLink from "@/components/branch-link";
import { PRLink } from "@/components/pr-link";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
import { z } from "zod";

const pullRequestsSearchSchema = z.object({
  searchString: z.string().optional().catch(""),
});

export const Route = createFileRoute("/_layout/prs/$prId")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(queries.cachedPullRequests()),
  validateSearch: pullRequestsSearchSchema,
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
        <Threads threads={pr.threads} />
        <DialogFooter>
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

function Threads(props: { threads: Array<PullRequestThread> }) {
  return (
    <div className="flex max-h-[60vh] flex-col gap-4 overflow-auto">
      {props.threads
        .filter((t) => t.status !== null)
        .map((thread) => (
          <Thread key={thread.id} thread={thread} />
        ))}
    </div>
  );
}

function Thread(props: { thread: PullRequestThread }) {
  return (
    <Card className="flex flex-col gap-2 p-2">
      {props.thread.comments.map((comment) => (
        <div key={comment.id} className="flex flex-row gap-2">
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
      ))}
    </Card>
  );
}
