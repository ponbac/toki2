import * as React from "react";
import { SubscriptionDateInput } from "@/components/ai-subscription-form";
import { Combobox } from "@/components/combobox";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import type {
  AiAccountDisposition,
  AiAccountDispositionTerms,
} from "@/lib/api/queries/ai-admin";
import {
  defaultEndDay,
  endDayProblem,
  parseDispositionDraft,
  periodPhrase,
  type DispositionDraft,
  type TargetOption,
} from "../../-lib/provider-accounts";

/** What the inline editor edits: a new disposition of an account, or one of
 * its dispositions. */
export type DispositionEditorState =
  | Readonly<{ mode: "create"; accountId: number; initial: DispositionDraft }>
  | Readonly<{
      mode: "update";
      accountId: number;
      dispositionId: number;
      initial: DispositionDraft;
    }>;

/** The inline form for a disposition's target and UTC days. */
export function DispositionEditor({
  editor,
  login,
  options,
  saving,
  onCancel,
  onSubmit,
}: {
  editor: DispositionEditorState;
  login: string;
  /** The target picker's items, suggestions first. */
  options: readonly Readonly<TargetOption>[];
  saving: boolean;
  onCancel: () => void;
  onSubmit: (terms: AiAccountDispositionTerms) => void;
}) {
  const [draft, setDraft] = React.useState<DispositionDraft>(editor.initial);
  const [submitAttempted, setSubmitAttempted] = React.useState(false);
  const id = React.useId();
  const parsed = parseDispositionDraft(draft);
  const update = (patch: Partial<DispositionDraft>) =>
    setDraft((current) => ({ ...current, ...patch }));

  return (
    <form
      className="flex max-w-2xl flex-col gap-3 rounded-md border border-border/60 bg-muted/30 p-3"
      aria-label={
        editor.mode === "create"
          ? `Set a disposition for ${login}`
          : `Edit ${login}'s disposition`
      }
      onSubmit={(event) => {
        event.preventDefault();
        setSubmitAttempted(true);
        if (parsed.ok && !saving) {
          onSubmit(parsed.terms);
        }
      }}
    >
      <div className="grid gap-2 sm:grid-cols-2">
        <div className="space-y-1 sm:col-span-2">
          <Label htmlFor={`${id}-target`} className="text-xs">
            Whose usage {login}&apos;s is
          </Label>
          <Combobox
            id={`${id}-target`}
            items={[...options]}
            value={draft.target}
            onChange={(target) => update({ target })}
            placeholder="Choose a Toki user, or not a Toki user"
            searchPlaceholder="Search by name or email..."
            emptyMessage="No Toki user matches"
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-from`} className="text-xs">
            First day
          </Label>
          <SubscriptionDateInput
            id={`${id}-from`}
            label="First day"
            value={draft.validFrom}
            onChange={(validFrom) => update({ validFrom })}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor={`${id}-to`} className="text-xs">
            Last day (optional)
          </Label>
          <SubscriptionDateInput
            id={`${id}-to`}
            label="Last day"
            value={draft.validTo}
            min={draft.validFrom}
            onChange={(validTo) => update({ validTo })}
          />
        </div>
      </div>
      <p className="text-xs text-muted-foreground">
        UTC calendar days as YYYY-MM-DD, as GitHub bills, and both are included.
        Leave the last day empty while it&apos;s ongoing.
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
          disabled={saving}
        >
          Cancel
        </Button>
        <Button type="submit" size="sm" disabled={saving}>
          {saving ? "Saving..." : editor.mode === "create" ? "Set" : "Save"}
        </Button>
      </div>
    </form>
  );
}

/**
 * Ends a disposition: asks only for its last day, today by default and never
 * before its first day.
 */
export function EndDispositionDialog({
  ending,
  login,
  owner,
  today,
  saving,
  onCancel,
  onEnd,
}: {
  /** The disposition to end; the dialog is closed while null. */
  ending: AiAccountDisposition | null;
  login: string;
  /** Whose the account's usage is, such as `Lisa Nord's`. */
  owner: string;
  /** Today's UTC date, `YYYY-MM-DD`. */
  today: string;
  saving: boolean;
  onCancel: () => void;
  onEnd: (disposition: AiAccountDisposition, lastDay: string) => void;
}) {
  return (
    <Dialog
      open={ending !== null}
      onOpenChange={(open) => {
        if (!open) {
          onCancel();
        }
      }}
    >
      <DialogContent className="sm:max-w-md">
        {ending && (
          <EndForm
            key={ending.id}
            disposition={ending}
            login={login}
            owner={owner}
            today={today}
            saving={saving}
            onCancel={onCancel}
            onEnd={(lastDay) => onEnd(ending, lastDay)}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function EndForm({
  disposition,
  login,
  owner,
  today,
  saving,
  onCancel,
  onEnd,
}: {
  disposition: AiAccountDisposition;
  login: string;
  owner: string;
  today: string;
  saving: boolean;
  onCancel: () => void;
  onEnd: (lastDay: string) => void;
}) {
  const [lastDay, setLastDay] = React.useState(() =>
    defaultEndDay(today, disposition.validFrom),
  );
  const [submitAttempted, setSubmitAttempted] = React.useState(false);
  const id = React.useId();
  const problem = endDayProblem(lastDay, disposition.validFrom);

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        setSubmitAttempted(true);
        if (problem === null && !saving) {
          onEnd(lastDay);
        }
      }}
    >
      <DialogHeader>
        <DialogTitle>End {login}&apos;s disposition</DialogTitle>
        <DialogDescription>
          {login} is {owner} {periodPhrase(disposition)}. Choose the last day it
          is, a UTC calendar day that is included. To hand the account to
          someone else, end this one, then set a new disposition from the next
          day.
        </DialogDescription>
      </DialogHeader>
      <div className="space-y-1">
        <Label htmlFor={`${id}-last`} className="text-xs">
          Last day
        </Label>
        <SubscriptionDateInput
          id={`${id}-last`}
          label="Last day"
          value={lastDay}
          min={disposition.validFrom}
          onChange={setLastDay}
        />
      </div>
      {submitAttempted && problem && (
        <p className="text-xs text-destructive" role="alert">
          {problem}
        </p>
      )}
      <DialogFooter>
        <Button
          type="button"
          variant="ghost"
          onClick={onCancel}
          disabled={saving}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={saving}>
          {saving ? "Saving..." : "End"}
        </Button>
      </DialogFooter>
    </form>
  );
}
