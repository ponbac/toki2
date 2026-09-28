import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { AlertTriangle, Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { SubscriptionForm } from "@/components/ai-subscription-form";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  describeDays,
  draftFrom,
  emptyDraft,
  formatPeriod,
  periodStatus,
  type SubscriptionEditor,
} from "@/lib/ai-subscriptions";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import {
  aiAdminQueries,
  type AiBillingDeveloper,
} from "@/lib/api/queries/ai-admin";
import type {
  AiSubscription,
  AiSubscriptionMismatch,
} from "@/lib/api/queries/ai-usage";
import { QueryError } from "./-components/query-error";
import { StatusBadge } from "./-components/status-badge";
import { formatFee } from "./-lib/format";

export const Route = createFileRoute("/_layout/admin/ai-subscriptions")({
  component: AdminSubscriptionsPage,
});

/** An open editor, with the developer a new subscription is for. */
type AdminEditor = SubscriptionEditor & { userId: number | null };

const EVERYONE = "everyone";

function AdminSubscriptionsPage() {
  const subscriptionsQuery = useQuery(aiAdminQueries.subscriptions());
  const mismatchesQuery = useQuery(aiAdminQueries.subscriptionMismatches());
  const developersQuery = useQuery(aiAdminQueries.developers());
  const [editor, setEditor] = React.useState<AdminEditor | null>(null);
  const [filter, setFilter] = React.useState<string>(EVERYONE);

  const create = aiAdminMutations.useAdminCreateSubscription({
    onSuccess: () => {
      toast.success("Subscription added");
      setEditor(null);
    },
    onError: apiErrorToast("Failed to add the subscription"),
  });
  const update = aiAdminMutations.useAdminUpdateSubscription({
    onSuccess: () => {
      toast.success("Subscription updated");
      setEditor(null);
    },
    onError: apiErrorToast("Failed to update the subscription"),
  });
  const remove = aiAdminMutations.useAdminDeleteSubscription({
    onSuccess: (_data, { subscriptionId }) => {
      setEditor((current) =>
        current?.mode === "update" && current.subscriptionId === subscriptionId
          ? null
          : current,
      );
      toast.success("Subscription deleted");
    },
    onError: apiErrorToast("Failed to delete the subscription"),
  });
  const saving = create.isPending || update.isPending;
  const busy = saving || remove.isPending;

  const developers = developersQuery.data ?? [];
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const today = subscriptionsQuery.data?.today;

  const editorView = editor && (
    <EditorPanel
      editor={editor}
      developers={developers}
      saving={saving}
      onDeveloper={(userId) => setEditor({ ...editor, userId })}
      onCancel={() => setEditor(null)}
      onSubmit={(terms) => {
        if (editor.mode === "update") {
          update.mutate({ subscriptionId: editor.subscriptionId, terms });
        } else if (editor.userId !== null) {
          create.mutate({ userId: editor.userId, terms });
        } else {
          toast.error("Choose the developer the subscription is for.");
        }
      }}
    />
  );

  return (
    <div className="flex flex-col gap-6">
      <p className="text-sm text-muted-foreground">
        Everyone&apos;s declared AI subscriptions. A subscription&apos;s fee
        pays for its provider&apos;s usage on the days it covers; other days
        bill as API usage at their estimated cost.
        {subscriptionsQuery.data &&
          ` Dates are calendar days in ${subscriptionsQuery.data.timeZone}, and both ends are included.`}
      </p>

      {mismatchesQuery.isError ? (
        <QueryError
          error={mismatchesQuery.error}
          what="plan hints"
          onRetry={() => mismatchesQuery.refetch()}
        />
      ) : (
        mismatchesQuery.data &&
        mismatchesQuery.data.mismatches.length > 0 && (
          <Mismatches
            mismatches={mismatchesQuery.data.mismatches}
            names={names}
            disabled={editor !== null}
            onDeclare={(mismatch) =>
              setEditor({
                mode: "create",
                userId: mismatch.userId,
                initial: {
                  ...emptyDraft(mismatch.provider, mismatch.firstDay),
                  validTo: mismatch.lastUncoveredDay ?? "",
                },
              })
            }
          />
        )
      )}

      <div className="flex flex-wrap items-center justify-between gap-2">
        <Select value={filter} onValueChange={setFilter}>
          <SelectTrigger className="w-60" aria-label="Developer">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={EVERYONE}>Everyone</SelectItem>
            {developers.map((developer) => (
              <SelectItem
                key={developer.userId}
                value={String(developer.userId)}
              >
                {developer.fullName}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          variant="outline"
          disabled={!today || editor !== null}
          onClick={() =>
            today &&
            setEditor({
              mode: "create",
              userId: filter === EVERYONE ? null : Number(filter),
              initial: emptyDraft("claude", today),
            })
          }
        >
          <Plus className="size-4" />
          Add subscription
        </Button>
      </div>

      {editor?.mode === "create" && editorView}

      {subscriptionsQuery.isPending ? (
        <Skeleton className="h-40 w-full" />
      ) : subscriptionsQuery.isError ? (
        <QueryError
          error={subscriptionsQuery.error}
          what="subscriptions"
          onRetry={() => subscriptionsQuery.refetch()}
        />
      ) : (
        <SubscriptionTable
          subscriptions={subscriptionsQuery.data.subscriptions.filter(
            (subscription) =>
              filter === EVERYONE || subscription.userId === Number(filter),
          )}
          today={subscriptionsQuery.data.today}
          names={names}
          busy={busy}
          editing={editor?.mode === "update" ? editor.subscriptionId : null}
          editorView={editor?.mode === "update" ? editorView : null}
          onEdit={(subscription) =>
            setEditor({
              mode: "update",
              subscriptionId: subscription.id,
              userId: subscription.userId,
              initial: draftFrom(subscription),
            })
          }
          onDelete={(subscription) => {
            if (
              window.confirm(
                `Delete ${names.get(subscription.userId) ?? "this developer"}'s ${subscription.plan}? Its days bill as API usage again, in past months too. To record that it ended, set an end date instead.`,
              )
            ) {
              remove.mutate({ subscriptionId: subscription.id });
            }
          }}
        />
      )}
    </div>
  );
}

function Mismatches({
  mismatches,
  names,
  disabled,
  onDeclare,
}: {
  mismatches: readonly AiSubscriptionMismatch[];
  names: ReadonlyMap<number, string>;
  disabled: boolean;
  onDeclare: (mismatch: AiSubscriptionMismatch) => void;
}) {
  return (
    <Alert variant="warning" className="p-3 [&>svg]:left-3 [&>svg]:top-3">
      <AlertTriangle className="size-4" />
      <AlertTitle className="text-sm">Undeclared subscriptions?</AlertTitle>
      <AlertDescription className="space-y-2 text-sm text-foreground">
        {mismatches.map((mismatch) => (
          <div
            key={`${mismatch.userId}-${mismatch.provider}-${mismatch.firstDay}`}
            className="flex items-start justify-between gap-2"
          >
            <p>
              <span className="font-medium">
                {names.get(mismatch.userId) ?? `User ${mismatch.userId}`}
              </span>
              : {AI_PROVIDER_LABELS[mismatch.provider]} reported a paid plan (
              {mismatch.plans.join(", ")}) {describeDays(mismatch)}, but no
              subscription is declared for those days, so they bill as API
              usage.
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 shrink-0 px-2 text-xs"
              disabled={disabled}
              onClick={() => onDeclare(mismatch)}
            >
              Declare
            </Button>
          </div>
        ))}
      </AlertDescription>
    </Alert>
  );
}

function EditorPanel({
  editor,
  developers,
  saving,
  onDeveloper,
  onCancel,
  onSubmit,
}: {
  editor: AdminEditor;
  developers: readonly AiBillingDeveloper[];
  saving: boolean;
  onDeveloper: (userId: number) => void;
  onCancel: () => void;
  onSubmit: React.ComponentProps<typeof SubscriptionForm>["onSubmit"];
}) {
  const id = React.useId();
  return (
    <div className="flex max-w-xl flex-col gap-2">
      {editor.mode === "create" && (
        <div className="flex flex-col gap-1">
          <Label htmlFor={`${id}-developer`} className="text-xs">
            Developer
          </Label>
          <Select
            value={editor.userId === null ? "" : String(editor.userId)}
            onValueChange={(value) => onDeveloper(Number(value))}
          >
            <SelectTrigger id={`${id}-developer`}>
              <SelectValue placeholder="Choose a developer" />
            </SelectTrigger>
            <SelectContent>
              {developers.map((developer) => (
                <SelectItem
                  key={developer.userId}
                  value={String(developer.userId)}
                >
                  {developer.fullName}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}
      <SubscriptionForm
        key={
          editor.mode === "update" ? `update-${editor.subscriptionId}` : "new"
        }
        editor={editor}
        isSaving={saving}
        onCancel={onCancel}
        onSubmit={onSubmit}
      />
    </div>
  );
}

function SubscriptionTable({
  subscriptions,
  today,
  names,
  busy,
  editing,
  editorView,
  onEdit,
  onDelete,
}: {
  subscriptions: readonly AiSubscription[];
  today: string;
  names: ReadonlyMap<number, string>;
  busy: boolean;
  editing: number | null;
  editorView: React.ReactNode;
  onEdit: (subscription: AiSubscription) => void;
  onDelete: (subscription: AiSubscription) => void;
}) {
  if (subscriptions.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No subscriptions declared.
      </p>
    );
  }
  return (
    <div className="rounded-xl border border-border/60 bg-card/60">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Developer</TableHead>
            <TableHead>Plan</TableHead>
            <TableHead className="text-right">Monthly fee</TableHead>
            <TableHead>Period</TableHead>
            <TableHead>Status</TableHead>
            <TableHead className="w-24" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {subscriptions.map((subscription) => {
            const status = periodStatus(subscription, today);
            return (
              <React.Fragment key={subscription.id}>
                <TableRow>
                  <TableCell className="font-medium">
                    {names.get(subscription.userId) ??
                      `User ${subscription.userId}`}
                  </TableCell>
                  <TableCell>
                    <div>{subscription.plan}</div>
                    <div className="text-xs text-muted-foreground">
                      {AI_PROVIDER_LABELS[subscription.provider]}
                    </div>
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatFee(subscription.monthlyCost, subscription.currency)}
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {formatPeriod(subscription)}
                  </TableCell>
                  <TableCell>
                    <StatusBadge
                      tone={
                        status === "ended"
                          ? "neutral"
                          : status === "upcoming"
                            ? "neutral"
                            : "good"
                      }
                      label={status}
                    />
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-1">
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="size-8"
                        aria-label={`Edit ${subscription.plan}`}
                        disabled={busy || editing !== null}
                        onClick={() => onEdit(subscription)}
                      >
                        <Pencil className="size-4" />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="size-8"
                        aria-label={`Delete ${subscription.plan}`}
                        disabled={busy}
                        onClick={() => onDelete(subscription)}
                      >
                        <Trash2 className="size-4" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
                {editing === subscription.id && (
                  <TableRow className="hover:bg-transparent">
                    <TableCell colSpan={6}>{editorView}</TableCell>
                  </TableRow>
                )}
              </React.Fragment>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}
