import * as React from "react";
import { CheckCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import type { AiProviderAccount } from "@/lib/api/queries/ai-admin";
import { plural } from "../../-lib/format";
import {
  formatPeriod,
  userName,
  type SuggestionPick,
} from "../../-lib/provider-accounts";

/**
 * "Accept all verified": links every account without a disposition to its
 * verified top candidate, after a look at the list. Probable matches are left
 * for their rows.
 */
export function AcceptAllDialog({
  picks,
  accounts,
  names,
  disabled,
  onAccept,
}: {
  picks: readonly SuggestionPick[];
  accounts: ReadonlyMap<number, AiProviderAccount>;
  names: ReadonlyMap<number, string>;
  disabled: boolean;
  onAccept: (picks: readonly SuggestionPick[]) => void;
}) {
  const [open, setOpen] = React.useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="outline"
          disabled={disabled || picks.length === 0}
        >
          <CheckCheck className="size-4" aria-hidden />
          Accept all verified ({picks.length})
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            Accept {picks.length} verified{" "}
            {picks.length === 1 ? "match" : "matches"}
          </DialogTitle>
          <DialogDescription>
            Links each account to the Toki user its public profile name or email
            verifies. Each link starts on the account&apos;s first seat day and
            is ongoing, or ends with its last seat if every seat has ended.
            Probable matches aren&apos;t included: confirm them row by row.
          </DialogDescription>
        </DialogHeader>
        <ul className="flex max-h-72 flex-col gap-1 overflow-y-auto text-sm">
          {picks.map((pick) => {
            const account = accounts.get(pick.accountId);
            return (
              <li
                key={pick.accountId}
                className="flex flex-wrap items-baseline justify-between gap-x-3"
              >
                <span>
                  <span className="font-medium">
                    {account?.login ?? `Account ${pick.accountId}`}
                  </span>{" "}
                  <span aria-hidden>→</span>
                  <span className="sr-only">to</span>{" "}
                  {userName(pick.userId, names)}
                </span>
                {account?.defaultPeriod && (
                  <span className="text-xs text-muted-foreground tabular-nums">
                    {formatPeriod(account.defaultPeriod)}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            disabled={disabled}
            onClick={() => {
              onAccept(picks);
              setOpen(false);
            }}
          >
            Link {plural(picks.length, "account")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
