import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { mutations } from "@/lib/api/mutations/mutations";
import { queries } from "@/lib/api/queries/queries";
import type {
  AgentRunEvent,
  AgentRunRecord,
  AgentRunStatus,
} from "@/lib/api/queries/agentRuns";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  Bot,
  Braces,
  CheckCircle2,
  CheckCheck,
  CircleDot,
  ClipboardCheck,
  Code2,
  ExternalLink,
  FileSearch,
  FileText,
  ListChecks,
  Loader2,
  MessageSquareText,
  RotateCcw,
  Search,
  Send,
  Sparkles,
  Terminal,
  Trash2,
  Wrench,
  XCircle,
  Zap,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { toast } from "sonner";

const ACTIVE_STATUSES = new Set<AgentRunStatus>([
  "created",
  "provisioningSandbox",
  "checkingRepositoryAccess",
  "cloningRepository",
  "loadingWorkflow",
  "planning",
  "revisingPlan",
  "planApproved",
  "implementing",
  "verifying",
  "creatingDraftPr",
  "awaitingBackendPublish",
  "backendPublishing",
]);

type AgentActivityKind =
  | "orchestration"
  | "planning"
  | "approval"
  | "implementation"
  | "search"
  | "read"
  | "edit"
  | "setup"
  | "validation"
  | "repair"
  | "summary"
  | "decision"
  | "error"
  | "done"
  | "canceled"
  | "note";

type AgentActivityItem = {
  id: string;
  title: string;
  detail: string;
  kind: AgentActivityKind;
  phase: "prepare" | "plan" | "build" | "verify" | "publish" | "complete";
  createdAt?: string;
  source: "event" | "opencode";
  status?: AgentRunStatus;
};

type AgentWorkspaceTab = "plan" | "context" | "activity";

export function AgentRunDrawer({
  runId,
  open,
  onOpenChange,
}: {
  runId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const enabled = open && runId !== null;
  const { data: run } = useQuery({
    ...queries.run(runId ?? "pending"),
    enabled,
  });
  const { data: events = [] } = useQuery({
    ...queries.events(runId ?? "pending"),
    enabled,
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="grid h-[min(1040px,calc(100vh-1.5rem))] max-w-[min(1480px,calc(100vw-1.5rem))] grid-rows-[auto_minmax(0,1fr)] overflow-hidden border-border/70 bg-background p-0 shadow-elevated-xl">
        <DialogHeader className="border-b border-border/60 bg-card/70 px-6 py-5 text-left">
          <div className="flex min-w-0 items-center gap-3 pr-10">
            <div className="flex size-10 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
              <Bot className="size-5" />
            </div>
            <div className="min-w-0">
              <DialogTitle className="truncate text-xl">
                Agent run review
              </DialogTitle>
              <DialogDescription className="truncate">
                {run
                  ? `${run.source.id}: ${run.source.title}`
                  : "Loading run..."}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <AgentRunWorkspace
          runId={runId}
          run={run}
          events={events}
          onClose={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}

function AgentRunWorkspace({
  runId,
  run,
  events,
  onClose,
}: {
  runId: string | null;
  run?: AgentRunRecord;
  events: AgentRunEvent[];
  onClose: () => void;
}) {
  const [feedback, setFeedback] = useState("");
  const [activeTab, setActiveTab] = useState<AgentWorkspaceTab>("plan");
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const { mutateAsync: sendFeedback, isPending: isSendingFeedback } =
    mutations.useSendAgentRunFeedback();
  const { mutateAsync: approvePlan, isPending: isApprovingPlan } =
    mutations.useApproveAgentRunPlan();
  const { mutateAsync: cancelRun, isPending: isCanceling } =
    mutations.useCancelAgentRun();
  const { mutateAsync: deleteRun, isPending: isDeleting } =
    mutations.useDeleteAgentRun();

  const canApprove = run?.status === "awaitingPlanFeedback";
  const canSendFeedback =
    run?.status === "awaitingPlanFeedback" ||
    run?.status === "planning" ||
    run?.status === "revisingPlan";
  const isActive = run ? ACTIVE_STATUSES.has(run.status) : false;
  const planMarkdown = run?.workpad.currentPlanMarkdown.trim();
  const activityItems = useMemo(
    () => buildAgentActivityItems(run, events),
    [run, events],
  );

  const handleSendFeedback = () => {
    if (!runId || feedback.trim().length === 0) {
      return;
    }

    void sendFeedback({ id: runId, message: feedback.trim() })
      .then(() => setFeedback(""))
      .catch(() => toast.error("Failed to send feedback."));
  };

  const handleApprovePlan = () => {
    if (!runId) {
      return;
    }

    setActiveTab("activity");
    void approvePlan(runId).catch(() => toast.error("Failed to approve plan."));
  };

  const handleCancelRun = () => {
    if (!runId) {
      return;
    }

    void cancelRun(runId).catch(() => toast.error("Failed to cancel run."));
  };

  const handleDeleteRun = () => {
    if (!runId) {
      return;
    }

    void deleteRun(runId)
      .then(() => {
        toast.success("Agent run removed.");
        setDeleteConfirmOpen(false);
        onClose();
      })
      .catch(() => toast.error("Failed to remove agent run."));
  };

  return (
    <>
      <div className="grid min-h-0 grid-cols-[minmax(0,1fr)_minmax(21rem,25rem)] overflow-hidden">
        <Tabs
          value={activeTab}
          onValueChange={(value) => setActiveTab(value as AgentWorkspaceTab)}
          className="grid min-h-0 min-w-0 grid-rows-[auto_1fr]"
        >
          <div className="flex items-center justify-between gap-3 border-b border-border/60 px-6 py-3">
            <TabsList className="h-9 bg-muted/70">
              <TabsTrigger value="plan">Plan</TabsTrigger>
              <TabsTrigger value="context">Context</TabsTrigger>
              <TabsTrigger value="activity">Activity</TabsTrigger>
            </TabsList>
            <RunStatusStrip run={run} isActive={isActive} />
          </div>
          <TabsContent value="plan" className="m-0 min-h-0">
            <ScrollArea className="h-full">
              <div className="grid gap-5 p-6">
                <Panel
                  title="Current plan"
                  meta={`v${run?.workpad.planVersion ?? 0}`}
                >
                  {planMarkdown ? (
                    <MarkdownBlock>{planMarkdown}</MarkdownBlock>
                  ) : (
                    <EmptyText>
                      The agent has not produced a plan yet.
                    </EmptyText>
                  )}
                </Panel>
                <FeedbackPanel
                  run={run}
                  feedback={feedback}
                  setFeedback={setFeedback}
                  canSendFeedback={canSendFeedback}
                  canApprove={canApprove}
                  isSendingFeedback={isSendingFeedback}
                  isApprovingPlan={isApprovingPlan}
                  onSendFeedback={handleSendFeedback}
                  onApprovePlan={handleApprovePlan}
                />
              </div>
            </ScrollArea>
          </TabsContent>
          <TabsContent value="context" className="m-0 min-h-0">
            <ScrollArea className="h-full">
              <div className="grid gap-5 p-6">
                <Panel title="Source brief" icon={FileText}>
                  {run?.source.markdown ? (
                    <MarkdownBlock>{run.source.markdown}</MarkdownBlock>
                  ) : (
                    <EmptyText>No source markdown was captured.</EmptyText>
                  )}
                </Panel>
                <WorkpadSection run={run} />
              </div>
            </ScrollArea>
          </TabsContent>
          <TabsContent value="activity" className="m-0 min-h-0">
            <ScrollArea className="h-full">
              <AgentFlowVariant
                run={run}
                items={activityItems}
                isActive={isActive}
              />
            </ScrollArea>
          </TabsContent>
        </Tabs>
        <aside className="grid min-h-0 min-w-0 grid-rows-[minmax(0,1fr)_auto] overflow-hidden border-l border-border/60 bg-muted/20">
          <ScrollArea className="min-h-0">
            <div className="grid min-w-0 gap-4 p-5">
              <RunSummary run={run} isActive={isActive} />
              <WorkpadSection run={run} compact />
            </div>
          </ScrollArea>
          <ActionFooter
            runId={runId}
            isActive={isActive}
            isCanceling={isCanceling}
            isDeleting={isDeleting}
            onCancelRun={handleCancelRun}
            onDeleteRun={() => setDeleteConfirmOpen(true)}
            onClose={onClose}
          />
        </aside>
      </div>
      <ConfirmAgentRunRemovalDialog
        open={deleteConfirmOpen}
        onOpenChange={setDeleteConfirmOpen}
        onConfirm={handleDeleteRun}
        isPending={isDeleting}
        run={run}
      />
    </>
  );
}

function ConfirmAgentRunRemovalDialog({
  open,
  onOpenChange,
  onConfirm,
  isPending,
  run,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  isPending: boolean;
  run?: AgentRunRecord;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md border-destructive/20 bg-background shadow-elevated-xl">
        <DialogHeader>
          <DialogTitle>Remove agent run?</DialogTitle>
          <DialogDescription>
            This hides the run from the board and deletes the stored run
            details.
          </DialogDescription>
        </DialogHeader>
        {run && (
          <div className="rounded-md border border-border/70 bg-muted/40 px-3 py-2">
            <p className="text-xs font-semibold uppercase text-muted-foreground">
              Work item
            </p>
            <p className="mt-1 break-words text-sm font-medium">
              {run.source.id}: {run.source.title}
            </p>
          </div>
        )}
        <DialogFooter>
          <Button
            type="button"
            variant="secondary"
            onClick={() => onOpenChange(false)}
            disabled={isPending}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={onConfirm}
            disabled={isPending}
            className="border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive"
          >
            {isPending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Trash2 className="size-4" />
            )}
            Remove run
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function RunStatusStrip({
  run,
  isActive,
}: {
  run?: AgentRunRecord;
  isActive: boolean;
}) {
  const draftPrUrl = run?.workpad.draftPrUrl
    ? normalizeAzureDevOpsPathUrl(run.workpad.draftPrUrl)
    : null;

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <span
        className={cn(
          "inline-flex items-center rounded-md border px-2.5 py-1 text-xs font-semibold",
          isActive
            ? "border-sky-300 bg-sky-100 text-sky-700 dark:border-sky-400/40 dark:bg-sky-500/10 dark:text-sky-300"
            : run?.status === "awaitingPlanFeedback"
              ? "border-amber-300 bg-amber-100 text-amber-800 dark:border-amber-400/40 dark:bg-amber-500/10 dark:text-amber-300"
              : "border-border bg-muted text-muted-foreground",
        )}
      >
        {isActive && <Loader2 className="mr-1.5 size-3 animate-spin" />}
        {formatStatus(run?.status ?? "loading")}
      </span>
      {draftPrUrl && (
        <a
          href={draftPrUrl}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 rounded-md border border-border/70 px-2.5 py-1 text-xs text-primary hover:bg-primary/10"
        >
          Draft PR
          <ExternalLink className="size-3" />
        </a>
      )}
      <span className="min-w-0 truncate text-xs text-muted-foreground">
        Updated {run ? new Date(run.updatedAt).toLocaleString() : "..."}
      </span>
    </div>
  );
}

function RunSummary({
  run,
  isActive,
}: {
  run?: AgentRunRecord;
  isActive: boolean;
}) {
  return (
    <div className="grid min-w-0 gap-3 overflow-hidden rounded-lg border border-border/70 bg-card/80 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase text-muted-foreground">
            Work item
          </p>
          <p className="mt-1 text-sm font-semibold">
            {run?.source.id ?? "..."}
          </p>
        </div>
        <div
          className={cn(
            "flex size-9 items-center justify-center rounded-md border",
            isActive
              ? "border-sky-300 bg-sky-100 text-sky-700 dark:border-sky-400/40 dark:bg-sky-500/10 dark:text-sky-300"
              : "border-border bg-muted text-muted-foreground",
          )}
        >
          {isActive ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Sparkles className="size-4" />
          )}
        </div>
      </div>
      <p className="line-clamp-3 min-w-0 break-words text-sm leading-relaxed text-foreground [overflow-wrap:anywhere]">
        {run?.source.title ?? "Loading run..."}
      </p>
      <div className="grid grid-cols-2 gap-2 text-xs">
        <Metric label="Plan" value={`v${run?.workpad.planVersion ?? 0}`} />
        <Metric label="Events" value={String(run?.events.length ?? 0)} />
        <Metric label="Model" value={run?.metadata?.model ?? "not recorded"} />
        <Metric
          label="Reasoning"
          value={run?.metadata?.reasoningLevel ?? "not recorded"}
        />
        <Metric
          label="Tokens"
          value={formatTokenUsage(run?.metadata?.tokenUsage)}
        />
        <Metric label="Notes" value={String(run?.workpad.notes.length ?? 0)} />
        <Metric
          label="Validation"
          value={String(run?.workpad.validationChecklist.length ?? 0)}
        />
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border border-border/60 bg-background/50 px-3 py-2">
      <p className="text-[11px] uppercase text-muted-foreground">{label}</p>
      <p className="mt-1 truncate font-mono text-sm text-foreground">{value}</p>
    </div>
  );
}

function FeedbackPanel({
  run,
  feedback,
  setFeedback,
  canSendFeedback,
  canApprove,
  isSendingFeedback,
  isApprovingPlan,
  onSendFeedback,
  onApprovePlan,
}: {
  run?: AgentRunRecord;
  feedback: string;
  setFeedback: (value: string) => void;
  canSendFeedback: boolean;
  canApprove: boolean;
  isSendingFeedback: boolean;
  isApprovingPlan: boolean;
  onSendFeedback: () => void;
  onApprovePlan: () => void;
}) {
  return (
    <Panel title="Feedback" icon={MessageSquareText}>
      <div className="grid gap-3">
        {run?.workpad.feedbackHistory.length ? (
          <div className="grid gap-2">
            {run.workpad.feedbackHistory.map((item) => (
              <div
                key={item.id}
                className="rounded-md border border-border/70 bg-background/50 p-3"
              >
                <MarkdownBlock compact>{item.message}</MarkdownBlock>
                <p className="mt-2 text-xs text-muted-foreground">
                  {item.actor.displayName} -{" "}
                  {new Date(item.createdAt).toLocaleString()}
                </p>
              </div>
            ))}
          </div>
        ) : (
          <EmptyText>No feedback yet.</EmptyText>
        )}
        <Textarea
          value={feedback}
          onChange={(event) => setFeedback(event.target.value)}
          disabled={!canSendFeedback || isSendingFeedback}
          placeholder="Feedback for the current plan."
          className="min-h-28 resize-none"
        />
        <div className="flex flex-wrap justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={
              feedback.trim().length === 0 ||
              !canSendFeedback ||
              isSendingFeedback
            }
            onClick={onSendFeedback}
          >
            {isSendingFeedback ? (
              <Loader2 className="mr-2 size-4 animate-spin" />
            ) : (
              <Send className="mr-2 size-4" />
            )}
            Send
          </Button>
          <Button
            type="button"
            disabled={!canApprove || isApprovingPlan}
            onClick={onApprovePlan}
          >
            {isApprovingPlan ? (
              <Loader2 className="mr-2 size-4 animate-spin" />
            ) : (
              <CheckCircle2 className="mr-2 size-4" />
            )}
            Implement plan
          </Button>
        </div>
      </div>
    </Panel>
  );
}

function WorkpadSection({
  run,
  compact = false,
}: {
  run?: AgentRunRecord;
  compact?: boolean;
}) {
  const sections = [
    {
      title: "Notes",
      icon: FileText,
      values: run?.workpad.notes ?? [],
      empty: "No notes.",
    },
    {
      title: "Validation",
      icon: CheckCircle2,
      values: run?.workpad.validationChecklist ?? [],
      empty: "No validation output yet.",
    },
    {
      title: "Risks",
      icon: AlertTriangle,
      values: run?.workpad.risksAndConfusions ?? [],
      empty: "No risks recorded.",
    },
    {
      title: "Acceptance",
      icon: CircleDot,
      values: run?.workpad.acceptanceCriteria ?? [],
      empty: "No acceptance criteria recorded.",
    },
  ];

  return (
    <div className={cn("grid min-w-0 gap-4", !compact && "md:grid-cols-2")}>
      {sections.map((section) => (
        <Panel key={section.title} title={section.title} icon={section.icon}>
          <WorkpadList values={section.values} empty={section.empty} />
        </Panel>
      ))}
      {run?.workpad.finalSummary && (
        <div className={cn(!compact && "md:col-span-2")}>
          <Panel title="Final summary" icon={Sparkles}>
            <MarkdownBlock>{run.workpad.finalSummary}</MarkdownBlock>
          </Panel>
        </div>
      )}
    </div>
  );
}

function AgentFlowVariant({
  run,
  items,
  isActive,
}: {
  run?: AgentRunRecord;
  items: AgentActivityItem[];
  isActive: boolean;
}) {
  const latest = items[0];
  const counts = countActivityKinds(items);
  const latestMarkerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    latestMarkerRef.current?.scrollIntoView({
      block: "start",
      behavior: "smooth",
    });
  }, [latest?.id]);

  return (
    <div className="grid min-h-full gap-5 bg-[linear-gradient(135deg,hsl(var(--background))_0%,hsl(var(--muted)/0.38)_48%,hsl(var(--background))_100%)] p-6">
      <div ref={latestMarkerRef} aria-hidden="true" />
      <section className="relative min-w-0 overflow-hidden rounded-lg border border-emerald-400/25 bg-zinc-950 text-zinc-50 shadow-elevated-xl">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_18%_12%,rgba(52,211,153,0.18),transparent_30%),radial-gradient(circle_at_88%_8%,rgba(251,191,36,0.13),transparent_26%),linear-gradient(90deg,rgba(255,255,255,0.045)_1px,transparent_1px),linear-gradient(rgba(255,255,255,0.035)_1px,transparent_1px)] bg-[size:auto,auto,36px_36px,36px_36px]" />
        <div className="relative grid gap-5 p-5">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="font-mono text-[11px] uppercase tracking-[0.24em] text-emerald-300">
                live agent trace
              </p>
              <h3 className="mt-2 break-words font-display text-2xl font-semibold leading-tight text-white">
                {latest?.title ?? "Waiting for agent activity"}
              </h3>
              {latest?.detail ? (
                <ActivityMarkdown inverted>{latest.detail}</ActivityMarkdown>
              ) : (
                <p className="mt-2 max-w-3xl break-words text-sm leading-relaxed text-zinc-300 [overflow-wrap:anywhere]">
                  Once the agent starts, file reads, searches, edits, setup,
                  validation, and repair attempts will appear here.
                </p>
              )}
            </div>
            <div className="grid min-w-40 gap-2 rounded-md border border-white/10 bg-white/[0.06] p-3">
              <span className="font-mono text-[11px] uppercase text-zinc-400">
                current state
              </span>
              <span className="inline-flex items-center gap-2 font-mono text-sm text-emerald-200">
                {isActive ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <CheckCircle2 className="size-4" />
                )}
                {formatStatus(run?.status ?? "loading")}
              </span>
            </div>
          </div>
          <div className="grid gap-3 sm:grid-cols-4">
            <ConsoleMetric label="Files read" value={counts.read} />
            <ConsoleMetric label="Searches" value={counts.search} />
            <ConsoleMetric label="Edits" value={counts.edit} />
            <ConsoleMetric label="Checks" value={counts.validation} />
          </div>
        </div>
      </section>

      {items.length === 0 ? (
        <Panel title="Agent focus" icon={Bot}>
          <EmptyText>No agent activity has been recorded yet.</EmptyText>
        </Panel>
      ) : (
        <div className="relative grid gap-3">
          {items.map((item, index) => {
            const visual = activityVisual(item);
            const Icon = visual.icon;
            const hasNextItem = index < items.length - 1;

            return (
              <article
                key={item.id}
                className={cn(
                  "relative grid grid-cols-[2.75rem_minmax(0,1fr)] gap-3",
                  index === 0 && "animate-in fade-in-0 slide-in-from-top-2",
                )}
              >
                {hasNextItem && (
                  <div
                    aria-hidden="true"
                    className="pointer-events-none absolute bottom-[-0.875rem] left-5 top-[2.75rem] w-px bg-gradient-to-b from-emerald-400/70 via-teal-400/55 to-emerald-400/70"
                  />
                )}
                <div
                  className={cn(
                    "z-10 mt-1 flex size-10 items-center justify-center rounded-md border bg-background shadow-sm",
                    visual.dotClass,
                  )}
                >
                  <Icon className="size-4" />
                </div>
                <div className="min-w-0 overflow-hidden rounded-lg border border-border/70 bg-card/95 p-4 shadow-sm dark:bg-card/85">
                  <div className="flex min-w-0 flex-wrap items-center gap-2">
                    <span
                      className={cn(
                        "rounded-sm border px-2 py-0.5 font-mono text-[10px] uppercase",
                        visual.badgeClass,
                      )}
                    >
                      {visual.label}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {item.createdAt
                        ? new Date(item.createdAt).toLocaleTimeString()
                        : item.source === "opencode"
                          ? "opencode"
                          : "event"}
                    </span>
                  </div>
                  <h4 className="mt-2 break-words text-sm font-semibold leading-snug">
                    {item.title}
                  </h4>
                  <ActivityMarkdown>{item.detail}</ActivityMarkdown>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
}

function ConsoleMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md border border-white/10 bg-white/[0.055] px-3 py-2">
      <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-zinc-500">
        {label}
      </p>
      <p className="mt-1 font-mono text-2xl font-semibold text-white">
        {value}
      </p>
    </div>
  );
}

function ActivityMarkdown({
  children,
  inverted = false,
}: {
  children: string;
  inverted?: boolean;
}) {
  return (
    <article
      className={cn(
        "prose prose-sm mt-1 min-w-0 max-w-none break-words [overflow-wrap:anywhere] dark:prose-invert prose-p:my-1 prose-p:break-words prose-p:[overflow-wrap:anywhere] prose-a:text-primary prose-a:underline prose-strong:text-foreground prose-code:break-words prose-code:text-foreground prose-code:[overflow-wrap:anywhere] prose-pre:max-w-full prose-pre:overflow-x-auto prose-pre:whitespace-pre-wrap prose-pre:rounded-md prose-pre:border prose-pre:border-border prose-pre:bg-secondary/70 prose-pre:text-foreground prose-ol:my-1 prose-ul:my-1 prose-li:my-0.5 prose-li:break-words prose-li:[overflow-wrap:anywhere] dark:prose-pre:bg-muted/70 dark:prose-pre:text-foreground",
        inverted
          ? "prose-p:text-zinc-300 prose-strong:text-white prose-code:text-emerald-200"
          : "prose-p:text-muted-foreground",
      )}
    >
      <ReactMarkdown>{children}</ReactMarkdown>
    </article>
  );
}

function buildAgentActivityItems(
  run: AgentRunRecord | undefined,
  events: AgentRunEvent[],
) {
  const eventItems = events.map((event) => activityItemFromEvent(event));
  const noteItems =
    run?.workpad.notes.map((note, index) =>
      activityItemFromNote(
        note,
        index,
        run.createdAt,
        run.updatedAt,
        run.workpad.notes.length,
      ),
    ) ?? [];

  return [...eventItems, ...noteItems].sort((a, b) => {
    const aTime = a.createdAt ? new Date(a.createdAt).getTime() : 0;
    const bTime = b.createdAt ? new Date(b.createdAt).getTime() : 0;

    if (aTime !== bTime) {
      return bTime - aTime;
    }

    return b.id.localeCompare(a.id);
  });
}

function activityItemFromEvent(event: AgentRunEvent): AgentActivityItem {
  return {
    id: `event-${event.id}`,
    title: titleForStatus(event.status),
    detail: event.message,
    kind: kindFromStatus(event.status),
    phase: phaseFromStatus(event.status),
    createdAt: event.createdAt,
    source: "event",
    status: event.status,
  };
}

function activityItemFromNote(
  note: string,
  index: number,
  runCreatedAt: string,
  runUpdatedAt: string,
  totalNotes: number,
): AgentActivityItem {
  const opencodePrefix = "OpenCode progress: ";
  const summaryPrefix = "OpenCode summary: ";
  const repairSummaryPrefix = "OpenCode repair summary: ";
  const isOpenCode = note.startsWith(opencodePrefix);
  const text = isOpenCode ? note.slice(opencodePrefix.length).trim() : note;
  const runStart = new Date(runCreatedAt).getTime();
  const runEnd = new Date(runUpdatedAt).getTime();
  const noteStep = Math.max(
    1,
    Math.floor((runEnd - runStart) / Math.max(totalNotes, 1)),
  );
  const createdAt = new Date(runStart + noteStep * (index + 1)).toISOString();

  if (note.startsWith(summaryPrefix) || note.startsWith(repairSummaryPrefix)) {
    return {
      id: `note-${index}`,
      title: note.startsWith(repairSummaryPrefix)
        ? "Repair summary"
        : "Implementation summary",
      detail: note
        .replace(summaryPrefix, "")
        .replace(repairSummaryPrefix, "")
        .trim(),
      kind: "summary",
      phase: note.startsWith(repairSummaryPrefix) ? "verify" : "build",
      createdAt,
      source: "opencode",
    };
  }

  if (text.startsWith("✱ Grep ")) {
    return {
      id: `note-${index}`,
      title: "Searched codebase",
      detail: cleanActivityDetail(text.replace(/^✱\s*/, "")),
      kind: "search",
      phase: "build",
      createdAt,
      source: "opencode",
    };
  }

  if (text.startsWith("→ Read ")) {
    return {
      id: `note-${index}`,
      title: "Read file",
      detail: cleanActivityDetail(text.replace(/^→\s*/, "")),
      kind: "read",
      phase: "build",
      createdAt,
      source: "opencode",
    };
  }

  if (/^M\s+\S/.test(text)) {
    return {
      id: `note-${index}`,
      title: "Changed file",
      detail: text.replace(/^M\s+/, ""),
      kind: "edit",
      phase: "build",
      createdAt,
      source: "opencode",
    };
  }

  if (text.startsWith("[ ] ")) {
    return {
      id: `note-${index}`,
      title: "Queued task",
      detail: text.replace(/^\[ \]\s*/, ""),
      kind: "decision",
      phase: "build",
      createdAt,
      source: "opencode",
    };
  }

  if (note.startsWith("Running setup command:")) {
    return {
      id: `note-${index}`,
      title: "Running setup",
      detail: note.replace("Running setup command:", "").trim(),
      kind: "setup",
      phase: "prepare",
      createdAt,
      source: "opencode",
    };
  }

  if (note.startsWith("Setup command")) {
    return {
      id: `note-${index}`,
      title: "Setup finished",
      detail: note,
      kind: "setup",
      phase: "prepare",
      createdAt,
      source: "opencode",
    };
  }

  if (note.startsWith("Running validation command:")) {
    return {
      id: `note-${index}`,
      title: "Running validation",
      detail: note.replace("Running validation command:", "").trim(),
      kind: "validation",
      phase: "verify",
      createdAt,
      source: "opencode",
    };
  }

  if (note.startsWith("Verification failed")) {
    return {
      id: `note-${index}`,
      title: "Validation failed",
      detail: note,
      kind: "repair",
      phase: "verify",
      createdAt,
      source: "opencode",
    };
  }

  if (text.startsWith("- Did not run validation")) {
    return {
      id: `note-${index}`,
      title: "Validation skipped",
      detail: text.replace(/^-\s*/, ""),
      kind: "validation",
      phase: "verify",
      createdAt,
      source: "opencode",
    };
  }

  if (note.includes("generated this plan")) {
    return {
      id: `note-${index}`,
      title: "Plan generated",
      detail: note,
      kind: "planning",
      phase: "plan",
      createdAt,
      source: "opencode",
    };
  }

  if (isOpenCode) {
    return {
      id: `note-${index}`,
      title: inferThinkingTitle(text),
      detail: text,
      kind: text.toLowerCase().includes("failure") ? "repair" : "note",
      phase: text.toLowerCase().includes("validation") ? "verify" : "build",
      createdAt,
      source: "opencode",
    };
  }

  return {
    id: `note-${index}`,
    title: note.startsWith("Run accepted")
      ? "Sandbox accepted run"
      : "Run note",
    detail: note,
    kind: note.startsWith("Run accepted") ? "orchestration" : "note",
    phase: "prepare",
    createdAt,
    source: "opencode",
  };
}

function activityVisual(item: AgentActivityItem): {
  icon: LucideIcon;
  label: string;
  dotClass: string;
  badgeClass: string;
  cardClass: string;
} {
  const map: Record<
    AgentActivityKind,
    {
      icon: LucideIcon;
      label: string;
      dotClass: string;
      badgeClass: string;
      cardClass: string;
    }
  > = {
    orchestration: {
      icon: Bot,
      label: "orchestrate",
      dotClass:
        "border-sky-300 bg-sky-100 text-sky-700 dark:border-sky-400/40 dark:bg-sky-500/10 dark:text-sky-300",
      badgeClass:
        "border-sky-300 bg-sky-100 text-sky-700 dark:border-sky-400/40 dark:bg-sky-500/10 dark:text-sky-300",
      cardClass: "border-sky-400/20",
    },
    planning: {
      icon: ClipboardCheck,
      label: "plan",
      dotClass:
        "border-amber-300 bg-amber-100 text-amber-800 dark:border-amber-400/40 dark:bg-amber-500/10 dark:text-amber-300",
      badgeClass:
        "border-amber-300 bg-amber-100 text-amber-800 dark:border-amber-400/40 dark:bg-amber-500/10 dark:text-amber-300",
      cardClass: "border-amber-400/20",
    },
    approval: {
      icon: CheckCircle2,
      label: "approval",
      dotClass:
        "border-lime-300 bg-lime-100 text-lime-800 dark:border-lime-400/40 dark:bg-lime-500/10 dark:text-lime-300",
      badgeClass:
        "border-lime-300 bg-lime-100 text-lime-800 dark:border-lime-400/40 dark:bg-lime-500/10 dark:text-lime-300",
      cardClass: "border-lime-400/20",
    },
    implementation: {
      icon: Code2,
      label: "implement",
      dotClass:
        "border-cyan-300 bg-cyan-100 text-cyan-800 dark:border-cyan-400/40 dark:bg-cyan-500/10 dark:text-cyan-300",
      badgeClass:
        "border-cyan-300 bg-cyan-100 text-cyan-800 dark:border-cyan-400/40 dark:bg-cyan-500/10 dark:text-cyan-300",
      cardClass: "border-cyan-400/20",
    },
    search: {
      icon: Search,
      label: "search",
      dotClass:
        "border-fuchsia-300 bg-fuchsia-100 text-fuchsia-800 dark:border-fuchsia-400/40 dark:bg-fuchsia-500/10 dark:text-fuchsia-300",
      badgeClass:
        "border-fuchsia-300 bg-fuchsia-100 text-fuchsia-800 dark:border-fuchsia-400/40 dark:bg-fuchsia-500/10 dark:text-fuchsia-300",
      cardClass: "border-fuchsia-400/20",
    },
    read: {
      icon: FileSearch,
      label: "read",
      dotClass:
        "border-blue-300 bg-blue-100 text-blue-800 dark:border-blue-400/40 dark:bg-blue-500/10 dark:text-blue-300",
      badgeClass:
        "border-blue-300 bg-blue-100 text-blue-800 dark:border-blue-400/40 dark:bg-blue-500/10 dark:text-blue-300",
      cardClass: "border-blue-400/20",
    },
    edit: {
      icon: Wrench,
      label: "edit",
      dotClass:
        "border-orange-300 bg-orange-100 text-orange-800 dark:border-orange-400/40 dark:bg-orange-500/10 dark:text-orange-300",
      badgeClass:
        "border-orange-300 bg-orange-100 text-orange-800 dark:border-orange-400/40 dark:bg-orange-500/10 dark:text-orange-300",
      cardClass: "border-orange-400/20",
    },
    setup: {
      icon: Terminal,
      label: "setup",
      dotClass:
        "border-stone-300 bg-stone-100 text-stone-700 dark:border-stone-400/40 dark:bg-stone-500/10 dark:text-stone-300",
      badgeClass:
        "border-stone-300 bg-stone-100 text-stone-700 dark:border-stone-400/40 dark:bg-stone-500/10 dark:text-stone-300",
      cardClass: "border-stone-400/20",
    },
    validation: {
      icon: ListChecks,
      label: "verify",
      dotClass:
        "border-teal-300 bg-teal-100 text-teal-800 dark:border-teal-400/40 dark:bg-teal-500/10 dark:text-teal-300",
      badgeClass:
        "border-teal-300 bg-teal-100 text-teal-800 dark:border-teal-400/40 dark:bg-teal-500/10 dark:text-teal-300",
      cardClass: "border-teal-400/20",
    },
    repair: {
      icon: RotateCcw,
      label: "repair",
      dotClass:
        "border-red-300 bg-red-100 text-red-800 dark:border-red-400/40 dark:bg-red-500/10 dark:text-red-300",
      badgeClass:
        "border-red-300 bg-red-100 text-red-800 dark:border-red-400/40 dark:bg-red-500/10 dark:text-red-300",
      cardClass: "border-red-400/20",
    },
    summary: {
      icon: Sparkles,
      label: "summary",
      dotClass:
        "border-violet-300 bg-violet-100 text-violet-800 dark:border-violet-400/40 dark:bg-violet-500/10 dark:text-violet-300",
      badgeClass:
        "border-violet-300 bg-violet-100 text-violet-800 dark:border-violet-400/40 dark:bg-violet-500/10 dark:text-violet-300",
      cardClass: "border-violet-400/20",
    },
    decision: {
      icon: Braces,
      label: "task",
      dotClass:
        "border-indigo-300 bg-indigo-100 text-indigo-800 dark:border-indigo-400/40 dark:bg-indigo-500/10 dark:text-indigo-300",
      badgeClass:
        "border-indigo-300 bg-indigo-100 text-indigo-800 dark:border-indigo-400/40 dark:bg-indigo-500/10 dark:text-indigo-300",
      cardClass: "border-indigo-400/20",
    },
    error: {
      icon: XCircle,
      label: "error",
      dotClass:
        "border-red-300 bg-red-100 text-red-800 dark:border-red-400/40 dark:bg-red-500/10 dark:text-red-300",
      badgeClass:
        "border-red-300 bg-red-100 text-red-800 dark:border-red-400/40 dark:bg-red-500/10 dark:text-red-300",
      cardClass: "border-red-400/20",
    },
    done: {
      icon: CheckCheck,
      label: "done",
      dotClass:
        "border-emerald-300 bg-emerald-100 text-emerald-800 dark:border-emerald-400/40 dark:bg-emerald-500/10 dark:text-emerald-300",
      badgeClass:
        "border-emerald-300 bg-emerald-100 text-emerald-800 dark:border-emerald-400/40 dark:bg-emerald-500/10 dark:text-emerald-300",
      cardClass: "border-emerald-400/20",
    },
    canceled: {
      icon: XCircle,
      label: "canceled",
      dotClass:
        "border-zinc-300 bg-zinc-100 text-zinc-700 dark:border-zinc-400/40 dark:bg-zinc-500/10 dark:text-zinc-300",
      badgeClass:
        "border-zinc-300 bg-zinc-100 text-zinc-700 dark:border-zinc-400/40 dark:bg-zinc-500/10 dark:text-zinc-300",
      cardClass: "border-zinc-400/20",
    },
    note: {
      icon: Zap,
      label: "note",
      dotClass: "border-primary/40 bg-primary/10 text-primary",
      badgeClass: "border-primary/40 bg-primary/10 text-primary",
      cardClass: "border-primary/20",
    },
  };

  return map[item.kind];
}

function countActivityKinds(items: AgentActivityItem[]) {
  return items.reduce(
    (counts, item) => {
      counts[item.kind] += 1;
      return counts;
    },
    {
      orchestration: 0,
      planning: 0,
      approval: 0,
      implementation: 0,
      search: 0,
      read: 0,
      edit: 0,
      setup: 0,
      validation: 0,
      repair: 0,
      summary: 0,
      decision: 0,
      error: 0,
      done: 0,
      canceled: 0,
      note: 0,
    } satisfies Record<AgentActivityKind, number>,
  );
}

function phaseFromStatus(status?: AgentRunStatus): AgentActivityItem["phase"] {
  switch (status) {
    case "created":
    case "provisioningSandbox":
    case "checkingRepositoryAccess":
    case "cloningRepository":
    case "loadingWorkflow":
      return "prepare";
    case "planning":
    case "awaitingPlanFeedback":
    case "revisingPlan":
      return "plan";
    case "planApproved":
    case "implementing":
      return "build";
    case "verifying":
      return "verify";
    case "creatingDraftPr":
    case "awaitingBackendPublish":
    case "backendPublishing":
    case "draftPrCreated":
      return "publish";
    default:
      return "complete";
  }
}

function kindFromStatus(status: AgentRunStatus): AgentActivityKind {
  switch (status) {
    case "created":
    case "provisioningSandbox":
    case "checkingRepositoryAccess":
    case "cloningRepository":
    case "loadingWorkflow":
      return "orchestration";
    case "planning":
    case "awaitingPlanFeedback":
    case "revisingPlan":
      return "planning";
    case "planApproved":
      return "approval";
    case "implementing":
      return "implementation";
    case "verifying":
    case "creatingDraftPr":
    case "awaitingBackendPublish":
    case "backendPublishing":
      return "validation";
    case "draftPrCreated":
    case "succeeded":
      return "done";
    case "failed":
      return "error";
    case "canceled":
      return "canceled";
  }
}

function titleForStatus(status: AgentRunStatus) {
  switch (status) {
    case "provisioningSandbox":
      return "Provisioning sandbox";
    case "checkingRepositoryAccess":
      return "Checking repository access";
    case "cloningRepository":
      return "Cloning repository";
    case "loadingWorkflow":
      return "Loading workflow policy";
    case "planning":
      return "Generating plan";
    case "awaitingPlanFeedback":
      return "Plan ready";
    case "revisingPlan":
      return "Revising plan";
    case "planApproved":
      return "Plan approved";
    case "implementing":
      return "Running implementation";
    case "verifying":
      return "Verifying changes";
    case "creatingDraftPr":
      return "Creating draft PR";
    case "awaitingBackendPublish":
      return "Waiting for publish";
    case "backendPublishing":
      return "Publishing branch";
    case "draftPrCreated":
      return "Draft PR created";
    case "succeeded":
      return "Run succeeded";
    case "failed":
      return "Run failed";
    case "canceled":
      return "Run canceled";
    default:
      return "Run created";
  }
}

function inferThinkingTitle(text: string) {
  const lower = text.toLowerCase();

  if (lower.includes("checking") || lower.includes("inspect")) {
    return "Inspecting code";
  }

  if (lower.includes("updating") || lower.includes("applying")) {
    return "Applying change";
  }

  if (lower.includes("found")) {
    return "Finding confirmed";
  }

  if (lower.includes("failure") || lower.includes("failed")) {
    return "Repair analysis";
  }

  return "Agent note";
}

function cleanActivityDetail(value: string) {
  return value.replace(/\s+\[offset=.*?\]$/, "").trim();
}

function ActionFooter({
  runId,
  isActive,
  isCanceling,
  isDeleting,
  onCancelRun,
  onDeleteRun,
  onClose,
}: {
  runId: string | null;
  isActive: boolean;
  isCanceling: boolean;
  isDeleting: boolean;
  onCancelRun: () => void;
  onDeleteRun: () => void;
  onClose: () => void;
}) {
  const isBusy = isCanceling || isDeleting;

  return (
    <div className="flex min-w-0 justify-between gap-2 border-t border-border bg-background/95 px-5 py-4">
      <div className="flex min-w-0 flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={!runId || !isActive || isBusy}
          onClick={onCancelRun}
        >
          {isCanceling && <Loader2 className="mr-2 size-4 animate-spin" />}
          Cancel run
        </Button>
        <Button
          type="button"
          variant="outline"
          size="icon"
          aria-label="Remove agent run"
          title="Remove agent run"
          className="border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive"
          disabled={!runId || isBusy}
          onClick={onDeleteRun}
        >
          {isDeleting ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Trash2 className="size-4" />
          )}
        </Button>
      </div>
      <Button type="button" variant="secondary" onClick={onClose}>
        Close
      </Button>
    </div>
  );
}

function Panel({
  title,
  meta,
  icon: Icon,
  children,
}: {
  title: string;
  meta?: string;
  icon?: LucideIcon;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 overflow-hidden rounded-lg border border-border/70 bg-card/70 shadow-sm">
      <div className="flex items-center justify-between gap-3 border-b border-border/60 px-4 py-3">
        <div className="flex min-w-0 items-center gap-2">
          {Icon && <Icon className="size-4 shrink-0 text-primary" />}
          <h3 className="truncate text-sm font-semibold">{title}</h3>
        </div>
        {meta && (
          <span className="shrink-0 text-xs text-muted-foreground">{meta}</span>
        )}
      </div>
      <div className="min-w-0 overflow-hidden p-4">{children}</div>
    </section>
  );
}

function MarkdownBlock({
  children,
  compact = false,
}: {
  children: string;
  compact?: boolean;
}) {
  return (
    <article
      className={cn(
        "prose min-w-0 max-w-none break-words [overflow-wrap:anywhere] dark:prose-invert prose-headings:scroll-m-20 prose-headings:font-semibold prose-p:my-2 prose-p:break-words prose-p:[overflow-wrap:anywhere] prose-a:text-primary prose-a:underline prose-blockquote:my-3 prose-blockquote:border-l prose-blockquote:border-border prose-blockquote:pl-3 prose-blockquote:text-muted-foreground prose-strong:text-foreground prose-code:break-words prose-code:text-foreground prose-code:[overflow-wrap:anywhere] prose-pre:max-w-full prose-pre:overflow-x-auto prose-pre:whitespace-pre-wrap prose-pre:rounded-md prose-pre:border prose-pre:border-border prose-pre:bg-secondary/70 prose-pre:text-foreground prose-ol:my-2 prose-ul:my-2 prose-li:my-0.5 prose-li:break-words prose-li:[overflow-wrap:anywhere] dark:prose-pre:bg-muted/70 dark:prose-pre:text-foreground",
        compact ? "prose-sm" : "prose-sm sm:prose-base",
      )}
    >
      <ReactMarkdown>{children}</ReactMarkdown>
    </article>
  );
}

function WorkpadList({ values, empty }: { values: string[]; empty: string }) {
  if (values.length === 0) {
    return <EmptyText>{empty}</EmptyText>;
  }

  return (
    <div className="grid min-w-0 gap-2">
      {values.map((value, index) => (
        <div
          key={`${value}-${index}`}
          className="min-w-0 overflow-hidden rounded-md border border-border/60 bg-background/50 px-3 py-2"
        >
          <MarkdownBlock compact>{value}</MarkdownBlock>
        </div>
      ))}
    </div>
  );
}

function EmptyText({ children }: { children: ReactNode }) {
  return <p className="text-sm text-muted-foreground">{children}</p>;
}

function formatStatus(status: string) {
  return status.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
}

function formatTokenUsage(
  tokenUsage: NonNullable<AgentRunRecord["metadata"]>["tokenUsage"],
) {
  if (!tokenUsage) {
    return "not reported";
  }

  if (tokenUsage.totalTokens !== undefined) {
    return tokenUsage.totalTokens.toLocaleString();
  }

  const parts = [
    tokenUsage.inputTokens !== undefined
      ? `${tokenUsage.inputTokens.toLocaleString()} in`
      : null,
    tokenUsage.outputTokens !== undefined
      ? `${tokenUsage.outputTokens.toLocaleString()} out`
      : null,
  ].filter((part): part is string => part !== null);

  return parts.length > 0 ? parts.join(" / ") : "not reported";
}

function normalizeAzureDevOpsPathUrl(value: string) {
  try {
    const url = new URL(value);

    if (url.hostname.toLowerCase() !== "dev.azure.com") {
      return value;
    }

    url.pathname = url.pathname
      .split("/")
      .map((segment) =>
        encodeURIComponent(decodeURIComponent(segment).replace(/\+/g, " ")),
      )
      .join("/");

    return url.toString();
  } catch {
    return value;
  }
}
