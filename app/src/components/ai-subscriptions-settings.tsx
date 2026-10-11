import * as React from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { AlertTriangle, CreditCard, Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { Separator } from "./ui/separator";
import { SubscriptionForm } from "./ai-subscription-form";
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
import { aiUsageMutations } from "@/lib/api/mutations/ai-usage";
import {
  aiUsageQueries,
  type AiSubscription,
  type AiSubscriptionList,
  type AiSubscriptionMismatch,
  type AiSubscriptionMismatchList,
} from "@/lib/api/queries/ai-usage";

/** Account-settings section for declaring AI subscriptions. */
export function AiSubscriptionsSettings() {
  const subscriptionsQuery = useQuery(aiUsageQueries.aiSubscriptions());
  const mismatchesQuery = useQuery(aiUsageQueries.aiSubscriptionMismatches());
  const [editor, setEditor] = React.useState<SubscriptionEditor | null>(null);

  const createSubscription = aiUsageMutations.useCreateAiSubscription({
    onSuccess: () => {
      toast.success("Subscription added");
      setEditor(null);
    },
    onError: apiErrorToast("Failed to add subscription"),
  });
  const updateSubscription = aiUsageMutations.useUpdateAiSubscription({
    onSuccess: () => {
      toast.success("Subscription updated");
      setEditor(null);
    },
    onError: apiErrorToast("Failed to update subscription"),
  });
  const deleteSubscription = aiUsageMutations.useDeleteAiSubscription({
    onSuccess: (_data, variables) => {
      setEditor((current) =>
        current?.mode === "update" &&
        current.subscriptionId === variables.subscriptionId
          ? null
          : current,
      );
      toast.success("Subscription deleted");
    },
    onError: apiErrorToast("Failed to delete subscription"),
  });

  const isSaving = createSubscription.isPending || updateSubscription.isPending;
  const isMutating = isSaving || deleteSubscription.isPending;

  return (
    <section className="space-y-3" aria-labelledby="ai-subscriptions-heading">
      <Separator />
      <div className="space-y-1">
        <h3
          id="ai-subscriptions-heading"
          className="flex items-center gap-2 text-sm font-medium"
        >
          <CreditCard className="size-4" />
          AI subscriptions
        </h3>
        <p className="text-xs text-muted-foreground">
          Declare the AI plans you pay a monthly fee for. On days without one, a
          provider&apos;s usage is billed at its estimated API cost in billing
          reports.
          {subscriptionsQuery.data &&
            ` Dates are calendar days in ${subscriptionsQuery.data.timeZone}.`}
        </p>
      </div>

      <MismatchWarning
        query={mismatchesQuery}
        declareDisabled={editor !== null}
        onDeclare={(mismatch) =>
          setEditor({
            mode: "create",
            initial: {
              ...emptyDraft(mismatch.provider, mismatch.firstDay),
              validTo: mismatch.lastUncoveredDay ?? "",
            },
          })
        }
      />

      <SubscriptionList
        query={subscriptionsQuery}
        disabled={isMutating}
        onEdit={(subscription) =>
          setEditor({
            mode: "update",
            subscriptionId: subscription.id,
            initial: draftFrom(subscription),
          })
        }
        onDelete={(subscription) => {
          if (
            window.confirm(
              `Delete ${subscription.plan}? Its days will count as API usage in billing reports again. To record that it ended, set an end date instead.`,
            )
          ) {
            deleteSubscription.mutate({ subscriptionId: subscription.id });
          }
        }}
      />

      {editor ? (
        <SubscriptionForm
          key={
            editor.mode === "update" ? `update-${editor.subscriptionId}` : "new"
          }
          editor={editor}
          isSaving={isSaving}
          onCancel={() => setEditor(null)}
          onSubmit={(terms) => {
            if (editor.mode === "create") {
              createSubscription.mutate({ terms });
            } else {
              updateSubscription.mutate({
                subscriptionId: editor.subscriptionId,
                terms,
              });
            }
          }}
        />
      ) : (
        <Button
          type="button"
          variant="outline"
          size="sm"
          // New subscriptions start today in the server's time zone.
          disabled={!subscriptionsQuery.data}
          onClick={() =>
            subscriptionsQuery.data &&
            setEditor({
              mode: "create",
              initial: emptyDraft("claude", subscriptionsQuery.data.today),
            })
          }
        >
          <Plus className="size-4" />
          Add subscription
        </Button>
      )}
    </section>
  );
}

function MismatchWarning({
  query,
  declareDisabled,
  onDeclare,
}: {
  query: UseQueryResult<AiSubscriptionMismatchList, Error>;
  declareDisabled: boolean;
  onDeclare: (mismatch: AiSubscriptionMismatch) => void;
}) {
  if (query.isPending) {
    return null;
  }

  if (query.isError) {
    return (
      <div className="flex items-center justify-between gap-3" role="alert">
        <p className="text-xs text-destructive">
          Could not check reported plans against your subscriptions.
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={async () => {
            await query.refetch();
          }}
        >
          Retry
        </Button>
      </div>
    );
  }

  const { mismatches } = query.data;
  if (mismatches.length === 0) {
    return null;
  }

  return (
    <Alert variant="warning" className="p-3 [&>svg]:left-3 [&>svg]:top-3">
      <AlertTriangle className="size-4" />
      <AlertTitle className="text-sm">Undeclared subscription?</AlertTitle>
      <AlertDescription className="space-y-2 text-xs text-foreground">
        {mismatches.map((mismatch) => (
          <div
            key={`${mismatch.provider}-${mismatch.firstDay}`}
            className="flex items-start justify-between gap-2"
          >
            <p>
              {AI_PROVIDER_LABELS[mismatch.provider]} usage indicates a
              subscription ({mismatch.plans.length === 1 ? "plan" : "plans"}{" "}
              {mismatch.plans.join(", ")}) {describeDays(mismatch)}, but no
              subscription is declared for these days, so their usage is billed
              at its estimated API cost in billing reports.
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 shrink-0 px-2 text-xs"
              disabled={declareDisabled}
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

function SubscriptionList({
  query,
  disabled,
  onEdit,
  onDelete,
}: {
  query: UseQueryResult<AiSubscriptionList, Error>;
  disabled: boolean;
  onEdit: (subscription: AiSubscription) => void;
  onDelete: (subscription: AiSubscription) => void;
}) {
  if (query.isPending) {
    return (
      <p className="text-xs text-muted-foreground" role="status">
        Loading subscriptions...
      </p>
    );
  }

  if (query.isError) {
    return (
      <div className="flex items-center justify-between gap-3" role="alert">
        <p className="text-xs text-destructive">
          Could not load subscriptions.
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={async () => {
            await query.refetch();
          }}
        >
          Retry
        </Button>
      </div>
    );
  }

  const { subscriptions, today } = query.data;
  if (subscriptions.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        No subscriptions declared yet.
      </p>
    );
  }

  return (
    <ul className="space-y-2">
      {subscriptions.map((subscription) => (
        <li
          key={subscription.id}
          className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-muted/30 px-3 py-2"
        >
          <div className="min-w-0">
            <p className="truncate text-sm font-medium">
              {subscription.plan}{" "}
              <span className="font-normal text-muted-foreground">
                · {AI_PROVIDER_LABELS[subscription.provider]}
              </span>
            </p>
            <p className="text-xs text-muted-foreground">
              {subscription.monthlyCost} {subscription.currency}/month ·{" "}
              {formatPeriod(subscription)} · {periodStatus(subscription, today)}
            </p>
          </div>
          <div className="flex shrink-0 gap-1">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-8"
              aria-label={`Edit ${subscription.plan}`}
              disabled={disabled}
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
              disabled={disabled}
              onClick={() => onDelete(subscription)}
            >
              <Trash2 className="size-4" />
            </Button>
          </div>
        </li>
      ))}
    </ul>
  );
}
