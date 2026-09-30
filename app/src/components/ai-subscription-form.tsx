import * as React from "react";
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
import {
  CURRENCIES,
  PLAN_SUGGESTIONS,
  parseDraft,
  type Draft,
  type SubscriptionEditor,
} from "@/lib/ai-subscriptions";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import {
  AI_PROVIDERS,
  type AiSubscriptionTerms,
} from "@/lib/api/queries/ai-usage";

/** The add/edit form for one AI subscription's terms. */
export function SubscriptionForm({
  editor,
  isSaving,
  onCancel,
  onSubmit,
}: {
  editor: SubscriptionEditor;
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
                  {AI_PROVIDER_LABELS[provider]}
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
