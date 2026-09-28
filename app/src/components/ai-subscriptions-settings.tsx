import * as React from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { AlertTriangle, CreditCard, Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./ui/select";
import { Separator } from "./ui/separator";
import { apiErrorToast } from "@/lib/api/errors";
import { aiUsageMutations } from "@/lib/api/mutations/ai-usage";
import {
  AI_PROVIDERS,
  MAX_PLAN_CODE_POINTS,
  aiUsageQueries,
  codePointLength,
  type AiProvider,
  type AiSubscription,
  type AiSubscriptionList,
  type AiSubscriptionMismatch,
  type AiSubscriptionMismatchList,
  type AiSubscriptionTerms,
} from "@/lib/api/queries/ai-usage";

const PROVIDER_LABELS: Record<AiProvider, string> = {
  codex: "Codex",
  claude: "Claude Code",
  grok: "Grok Build",
  copilot: "Copilot",
};

/** Suggested plan names. Plans are free text, so any name is accepted. */
const PLAN_SUGGESTIONS: Record<AiProvider, readonly string[]> = {
  codex: ["ChatGPT Plus", "ChatGPT Pro"],
  claude: ["Claude Pro", "Claude Max 5x", "Claude Max 20x"],
  grok: [],
  copilot: ["Copilot Business"],
};

const CURRENCIES: readonly string[] = ["SEK", "USD", "EUR"];

/** Form state as typed; the cost may use a decimal comma. */
type Draft = Readonly<{
  provider: AiProvider;
  plan: string;
  monthlyCost: string;
  currency: string;
  validFrom: string;
  validTo: string;
}>;

type Editor =
  | Readonly<{ mode: "create"; initial: Draft }>
  | Readonly<{ mode: "update"; subscriptionId: number; initial: Draft }>;

type ParsedDraft =
  | Readonly<{ ok: true; terms: AiSubscriptionTerms }>
  | Readonly<{ ok: false; problem: string }>;

/** Account-settings section for declaring AI subscriptions. */
export function AiSubscriptionsSettings() {
  const subscriptionsQuery = useQuery(aiUsageQueries.aiSubscriptions());
  const mismatchesQuery = useQuery(aiUsageQueries.aiSubscriptionMismatches());
  const [editor, setEditor] = React.useState<Editor | null>(null);

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
              {PROVIDER_LABELS[mismatch.provider]} usage indicates a
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
                · {PROVIDER_LABELS[subscription.provider]}
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

function SubscriptionForm({
  editor,
  isSaving,
  onCancel,
  onSubmit,
}: {
  editor: Editor;
  isSaving: boolean;
  onCancel: () => void;
  onSubmit: (terms: AiSubscriptionTerms) => void;
}) {
  const [draft, setDraft] = React.useState<Draft>(editor.initial);
  const [submitAttempted, setSubmitAttempted] = React.useState(false);
  const id = React.useId();
  const parsed = parseDraft(draft);
  const currencies = CURRENCIES.includes(draft.currency)
    ? CURRENCIES
    : [...CURRENCIES, draft.currency];
  const update = (patch: Partial<Draft>) =>
    setDraft((current) => ({ ...current, ...patch }));

  return (
    <form
      className="space-y-3 rounded-md border border-border/60 bg-muted/30 p-3"
      aria-label={
        editor.mode === "create" ? "Add subscription" : "Edit subscription"
      }
      onSubmit={(event) => {
        event.preventDefault();
        setSubmitAttempted(true);
        if (parsed.ok && !isSaving) {
          onSubmit(parsed.terms);
        }
      }}
    >
      <div className="grid grid-cols-2 gap-2">
        <div className="space-y-1">
          <Label htmlFor={`${id}-provider`} className="text-xs">
            Provider
          </Label>
          <Select
            value={draft.provider}
            onValueChange={(value) => {
              const provider = AI_PROVIDERS.find((known) => known === value);
              if (provider) {
                update({ provider });
              }
            }}
          >
            <SelectTrigger id={`${id}-provider`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {AI_PROVIDERS.map((provider) => (
                <SelectItem key={provider} value={provider}>
                  {PROVIDER_LABELS[provider]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-plan`} className="text-xs">
            Plan
          </Label>
          <Input
            id={`${id}-plan`}
            list={`${id}-plans`}
            value={draft.plan}
            placeholder={PLAN_SUGGESTIONS[draft.provider][0] ?? "Plan name"}
            onChange={(event) => update({ plan: event.target.value })}
          />
          <datalist id={`${id}-plans`}>
            {PLAN_SUGGESTIONS[draft.provider].map((plan) => (
              <option key={plan} value={plan} />
            ))}
          </datalist>
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-cost`} className="text-xs">
            Monthly cost
          </Label>
          <Input
            id={`${id}-cost`}
            inputMode="decimal"
            value={draft.monthlyCost}
            placeholder="0.00"
            onChange={(event) => update({ monthlyCost: event.target.value })}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-currency`} className="text-xs">
            Currency
          </Label>
          <Select
            value={draft.currency}
            onValueChange={(currency) => update({ currency })}
          >
            <SelectTrigger id={`${id}-currency`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {currencies.map((currency) => (
                <SelectItem key={currency} value={currency}>
                  {currency}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-from`} className="text-xs">
            From
          </Label>
          <Input
            id={`${id}-from`}
            type="date"
            value={draft.validFrom}
            onChange={(event) => update({ validFrom: event.target.value })}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-to`} className="text-xs">
            To (optional)
          </Label>
          <Input
            id={`${id}-to`}
            type="date"
            value={draft.validTo}
            min={draft.validFrom || undefined}
            onChange={(event) => update({ validTo: event.target.value })}
          />
        </div>
      </div>
      <p className="text-xs text-muted-foreground">
        Both dates are included. Leave the end empty while the subscription is
        ongoing.
      </p>
      {submitAttempted && !parsed.ok && (
        <p className="text-xs text-destructive" role="alert">
          {parsed.problem}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={isSaving}
        >
          Cancel
        </Button>
        <Button type="submit" size="sm" disabled={isSaving}>
          {isSaving ? "Saving..." : editor.mode === "create" ? "Add" : "Save"}
        </Button>
      </div>
    </form>
  );
}

function emptyDraft(provider: AiProvider, validFrom: string): Draft {
  return {
    provider,
    plan: "",
    monthlyCost: "",
    currency: "SEK",
    validFrom,
    validTo: "",
  };
}

function draftFrom(subscription: AiSubscription): Draft {
  return {
    provider: subscription.provider,
    plan: subscription.plan,
    monthlyCost: subscription.monthlyCost,
    currency: subscription.currency,
    validFrom: subscription.validFrom,
    validTo: subscription.validTo ?? "",
  };
}

function parseDraft(draft: Draft): ParsedDraft {
  const plan = draft.plan.trim();
  if (!plan) {
    return { ok: false, problem: "Enter a plan, such as Claude Max 5x." };
  }
  if (codePointLength(plan) > MAX_PLAN_CODE_POINTS) {
    return {
      ok: false,
      problem: `Shorten the plan to at most ${MAX_PLAN_CODE_POINTS} characters.`,
    };
  }

  const monthlyCost = draft.monthlyCost.replace(/\s/g, "").replace(",", ".");
  if (!/^\d{1,10}(\.\d{1,2})?$/.test(monthlyCost)) {
    return {
      ok: false,
      problem:
        "Enter the monthly cost as an amount with at most two decimals, such as 1100 or 19.99.",
    };
  }

  if (!draft.validFrom) {
    return { ok: false, problem: "Enter the first day it covers." };
  }
  if (draft.validTo && draft.validTo < draft.validFrom) {
    return { ok: false, problem: "The end must not be before the start." };
  }

  return {
    ok: true,
    terms: {
      provider: draft.provider,
      plan,
      monthlyCost,
      currency: draft.currency,
      validFrom: draft.validFrom,
      validTo: draft.validTo || null,
    },
  };
}

function formatPeriod(subscription: AiSubscription): string {
  return subscription.validTo
    ? `${subscription.validFrom} – ${subscription.validTo}`
    : `from ${subscription.validFrom}`;
}

/** Which days a mismatch covers, in words. */
function describeDays(mismatch: AiSubscriptionMismatch): string {
  if (mismatch.firstDay === mismatch.lastDay) {
    return `on ${mismatch.firstDay}`;
  }

  const days = `${mismatch.days} ${mismatch.days === 1 ? "day" : "days"}`;
  return `on ${days} from ${mismatch.firstDay} to ${mismatch.lastDay}`;
}

/**
 * Status on the server's `today`. Local dates compare correctly as
 * `YYYY-MM-DD` strings.
 */
function periodStatus(subscription: AiSubscription, today: string): string {
  if (subscription.validFrom > today) {
    return "upcoming";
  }
  if (subscription.validTo !== null && subscription.validTo < today) {
    return "ended";
  }
  return subscription.validTo === null ? "ongoing" : "active";
}
