import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { toast } from "sonner";
import { Skeleton } from "@/components/ui/skeleton";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import {
  aiAdminQueries,
  type AiAccountDisposition,
  type AiAccountDispositionTerms,
  type AiBillingDeveloper,
  type AiDirectoryRefresh,
  type AiProviderAccounts,
} from "@/lib/api/queries/ai-admin";
import { QueryError } from "../-components/query-error";
import { plural } from "../-lib/format";
import {
  acceptAllPicks,
  acceptedSummary,
  formatPeriod,
  ownerPhrase,
  periodPhrase,
  targetLabel,
  targetOptions,
  targetValue,
} from "../-lib/provider-accounts";
import { AcceptAllDialog } from "./-components/accept-all-dialog";
import { AccountTable, type AccountActions } from "./-components/account-table";
import { DirectoryStatus } from "./-components/directory-status";
import {
  DispositionEditor,
  EndDispositionDialog,
  type DispositionEditorState,
} from "./-components/disposition-editor";
import { NeedsALook } from "./-components/needs-a-look";

export const Route = createFileRoute("/_layout/admin/github-logins/")({
  component: GithubLoginsPage,
});

function GithubLoginsPage() {
  const accountsQuery = useQuery(aiAdminQueries.providerAccounts());
  const developersQuery = useQuery(aiAdminQueries.developers());

  return (
    <div className="flex flex-col gap-6">
      <p className="text-sm text-muted-foreground">
        GitHub accounts holding Copilot seats or credits in{" "}
        {accountsQuery.data?.directory.namespace || "the organization"}, and
        whose usage each one is: a Toki user&apos;s, or not a Toki user&apos;s
        (Spinit overhead). Dates are UTC calendar days, as GitHub bills, and
        both ends are included. From the Copilot billing cutover on, they decide
        whose project lines an account&apos;s credits bill to; days without one
        bill as Spinit&apos;s unlinked account line.
      </p>

      {developersQuery.isError && (
        <QueryError
          error={developersQuery.error}
          what="Toki users"
          onRetry={() => developersQuery.refetch()}
        />
      )}

      {accountsQuery.isPending ? (
        <div className="flex flex-col gap-4">
          <Skeleton className="h-14 w-full" />
          <div className="grid gap-4 md:grid-cols-2">
            <Skeleton className="h-36 w-full" />
            <Skeleton className="h-36 w-full" />
          </div>
          <Skeleton className="h-72 w-full" />
        </div>
      ) : accountsQuery.isError ? (
        <QueryError
          error={accountsQuery.error}
          what="GitHub accounts"
          onRetry={() => accountsQuery.refetch()}
        />
      ) : (
        <Accounts
          overview={accountsQuery.data}
          developers={developersQuery.data ?? []}
        />
      )}
    </div>
  );
}

function Accounts({
  overview,
  developers,
}: {
  overview: AiProviderAccounts;
  developers: readonly AiBillingDeveloper[];
}) {
  const [editor, setEditor] = React.useState<DispositionEditorState | null>(
    null,
  );
  const [ending, setEnding] = React.useState<AiAccountDisposition | null>(null);
  const [open, setOpen] = React.useState<ReadonlySet<number>>(new Set());
  const [focus, setFocus] = React.useState<Readonly<{
    accountId: number;
  }> | null>(null);

  const { accounts, today } = overview;
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const logins = new Map(accounts.map((a) => [a.accountId, a.login]));
  const byId = new Map(accounts.map((a) => [a.accountId, a]));
  const activity = new Map(overview.activity.map((a) => [a.userId, a]));
  const picks = acceptAllPicks(accounts);
  const needingALook = accounts.filter((account) => account.needsLook);

  const refresh = aiAdminMutations.useRefreshProviderAccounts({
    onSuccess: (run) => reportRefresh(run),
    onError: apiErrorToast("Failed to read seats from GitHub"),
  });
  const create = aiAdminMutations.useCreateAccountDisposition({
    onSuccess: (disposition) => {
      toast.success(
        `${disposition.login} is ${ownerPhrase(disposition.target, names)} ${periodPhrase(disposition)}`,
      );
      setEditor(null);
    },
    onError: apiErrorToast("Failed to save the disposition"),
  });
  const update = aiAdminMutations.useUpdateAccountDisposition({
    onSuccess: (disposition) => {
      toast.success(
        `${disposition.login} is ${ownerPhrase(disposition.target, names)} ${periodPhrase(disposition)}`,
      );
      setEditor(null);
      setEnding(null);
    },
    onError: apiErrorToast("Failed to update the disposition"),
  });
  const remove = aiAdminMutations.useDeleteAccountDisposition({
    onSuccess: (_data, { dispositionId }) => {
      setEditor((current) =>
        current?.mode === "update" && current.dispositionId === dispositionId
          ? null
          : current,
      );
      toast.success("Disposition deleted");
    },
    onError: apiErrorToast("Failed to delete the disposition"),
  });
  const accept = aiAdminMutations.useAcceptAccountSuggestions({
    onSuccess: (accepted) => {
      const summary = acceptedSummary(accepted, logins);
      if (accepted.skipped.length > 0) {
        toast.warning(summary);
      } else {
        toast.success(summary);
      }
    },
    onError: apiErrorToast("Failed to accept the suggestions"),
  });
  const saving = create.isPending || update.isPending;
  const busy = saving || remove.isPending || accept.isPending;

  /** Opens an account's editor for a new disposition and brings it into view. */
  const openAccount = (accountId: number, userId: number | null) => {
    const account = byId.get(accountId);
    if (!account) {
      return;
    }
    if (account.defaultPeriod) {
      setEditor({
        mode: "create",
        accountId,
        initial: {
          target: userId === null ? "" : targetValue({ kind: "user", userId }),
          validFrom: account.defaultPeriod.validFrom,
          validTo: account.defaultPeriod.validTo ?? "",
        },
      });
    } else {
      setOpen((current) => new Set(current).add(accountId));
    }
    setFocus({ accountId });
  };

  const actions: AccountActions = {
    onSet: (account) => openAccount(account.accountId, null),
    onAccept: (account, suggestion) => {
      if (account.defaultPeriod) {
        create.mutate({
          accountId: account.accountId,
          terms: {
            target: { kind: "user", userId: suggestion.userId },
            validFrom: account.defaultPeriod.validFrom,
            validTo: account.defaultPeriod.validTo,
          },
        });
      }
    },
    onEdit: (account, disposition) =>
      setEditor({
        mode: "update",
        accountId: account.accountId,
        dispositionId: disposition.id,
        initial: {
          target: targetValue(disposition.target),
          validFrom: disposition.validFrom,
          validTo: disposition.validTo ?? "",
        },
      }),
    onEnd: setEnding,
    onDelete: (account, disposition) => {
      if (
        window.confirm(
          `Delete ${account.login}'s disposition (${targetLabel(disposition.target, names)}, ${formatPeriod(disposition)})? It is deleted as if it never existed. To record that the account stopped being theirs, set its last day instead.`,
        )
      ) {
        remove.mutate({ dispositionId: disposition.id });
      }
    },
  };

  const editedAccount = editor ? byId.get(editor.accountId) : undefined;
  const editorView = editor && editedAccount && (
    <DispositionEditor
      key={
        editor.mode === "update"
          ? `update-${editor.dispositionId}`
          : `create-${editor.accountId}-${editor.initial.target}`
      }
      editor={editor}
      login={editedAccount.login}
      options={targetOptions(editedAccount.suggestions, developers)}
      saving={saving}
      onCancel={() => setEditor(null)}
      onSubmit={(terms: AiAccountDispositionTerms) => {
        if (editor.mode === "update") {
          update.mutate({ dispositionId: editor.dispositionId, terms });
        } else {
          create.mutate({ accountId: editor.accountId, terms });
        }
      }}
    />
  );
  const endingAccount = ending ? byId.get(ending.accountId) : undefined;

  return (
    <>
      <DirectoryStatus
        directory={overview.directory}
        today={today}
        refreshing={refresh.isPending}
        onRefresh={() => refresh.mutate()}
      />

      {(accounts.length > 0 || overview.unlinkedUsers.length > 0) && (
        <NeedsALook
          accounts={needingALook}
          unlinkedUsers={overview.unlinkedUsers}
          reviewFrom={overview.reviewFrom}
          names={names}
          logins={logins}
          disabled={busy}
          onOpenAccount={openAccount}
        />
      )}

      {accounts.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border/70 px-4 py-8 text-center text-sm text-muted-foreground">
          {emptyMessage(overview)}
        </p>
      ) : (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-sm text-muted-foreground">
              {plural(accounts.length, "account")}
              {needingALook.length > 0 &&
                `, ${needingALook.length} needing a look`}
            </p>
            <AcceptAllDialog
              picks={picks}
              accounts={byId}
              names={names}
              disabled={busy || editor !== null}
              onAccept={(chosen) => accept.mutate({ picks: chosen })}
            />
          </div>
          <AccountTable
            accounts={accounts}
            names={names}
            activity={activity}
            today={today}
            reviewFrom={overview.reviewFrom}
            creditsMonth={overview.creditsMonth}
            open={open}
            onToggle={(accountId) =>
              setOpen((current) => {
                const next = new Set(current);
                if (!next.delete(accountId)) {
                  next.add(accountId);
                }
                return next;
              })
            }
            editingAccountId={editor?.accountId ?? null}
            editorView={editorView}
            busy={busy}
            focus={focus}
            actions={actions}
          />
        </div>
      )}

      <EndDispositionDialog
        ending={ending}
        login={endingAccount?.login ?? ending?.login ?? ""}
        owner={ending ? ownerPhrase(ending.target, names) : ""}
        today={today}
        saving={update.isPending}
        onCancel={() => setEnding(null)}
        onEnd={(disposition, lastDay) =>
          update.mutate({
            dispositionId: disposition.id,
            terms: {
              target: disposition.target,
              validFrom: disposition.validFrom,
              validTo: lastDay,
            },
          })
        }
      />
    </>
  );
}

/** Tells what a refresh read, and warns when it couldn't end seats. */
function reportRefresh(run: AiDirectoryRefresh) {
  const changes = [
    run.seatsStarted > 0 && `${plural(run.seatsStarted, "new seat")}`,
    run.seatsEnded > 0 && `${plural(run.seatsEnded, "ended seat")}`,
  ].filter(Boolean);
  const profiles =
    run.profilesFailed > 0
      ? `; ${plural(run.profilesFailed, "profile")} couldn't be read`
      : "";
  const summary = `Read ${plural(run.seatsListed, "seat")} from GitHub${
    changes.length > 0 ? ` (${changes.join(", ")})` : ""
  }${profiles}.`;
  if (run.complete) {
    toast.success(summary);
  } else {
    toast.warning(
      `${summary} GitHub reported ${run.seatsReported ?? "more"} seats, so none was ended.`,
    );
  }
}

/** Why there are no accounts yet, and what would bring them. */
function emptyMessage(overview: AiProviderAccounts): string {
  const { directory } = overview;
  switch (directory.status) {
    case "ready":
      return directory.lastSuccessAt
        ? `GitHub lists no Copilot seats in ${directory.namespace || "the organization"}.`
        : "No accounts yet. Refresh to read the Copilot seats from GitHub.";
    case "disabled":
      return "No accounts yet. Connect GitHub to read the Copilot seats.";
    case "misconfigured":
      return "No accounts yet. Fix the GitHub settings to read the Copilot seats.";
  }
}
