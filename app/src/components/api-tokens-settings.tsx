import * as React from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { ClipboardCopy, KeyRound, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { Separator } from "./ui/separator";
import type { ApiTokenIssuance } from "@/hooks/use-api-token-issuance";
import { API_URL } from "@/lib/api/api";
import { apiErrorToast } from "@/lib/api/errors";
import { userMutations } from "@/lib/api/mutations/user";
import {
  userQueries,
  type ApiToken,
  type CreatedApiToken,
} from "@/lib/api/queries/user";

/** Account-settings section for issuing and revoking API tokens. */
export function ApiTokensSettings({
  issuance,
}: {
  issuance: ApiTokenIssuance;
}) {
  const [name, setName] = React.useState("");
  const tokensQuery = useQuery(userQueries.apiTokens());
  const revokeToken = userMutations.useRevokeApiToken({
    onSuccess: (_data, variables) => {
      if (issuance.issued?.id === variables.tokenId) {
        issuance.dismiss();
      }
      toast.success("Token revoked");
    },
    onError: apiErrorToast("Failed to revoke token"),
  });

  React.useEffect(() => {
    if (issuance.issued) {
      setName("");
    }
  }, [issuance.issued]);

  const creationDisabled =
    !name.trim() || issuance.isPending || issuance.issued !== null;

  return (
    <section className="space-y-3" aria-labelledby="api-tokens-heading">
      <Separator />
      <div className="space-y-1">
        <h3
          id="api-tokens-heading"
          className="flex items-center gap-2 text-sm font-medium"
        >
          <KeyRound className="size-4" />
          API tokens
        </h3>
        <p className="text-xs text-muted-foreground">
          Create an API token. The secret is shown only once.
        </p>
      </div>

      {issuance.issued && (
        <IssuedTokenCard
          token={issuance.issued}
          dismissDisabled={issuance.isPending}
          onDismiss={issuance.dismiss}
        />
      )}

      <form
        className="flex items-end gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          const trimmed = name.trim();
          if (!creationDisabled) {
            issuance.issue(trimmed);
          }
        }}
      >
        <div className="min-w-0 flex-1 space-y-1">
          <Label htmlFor="api-token-name" className="text-xs">
            Name
          </Label>
          <Input
            id="api-token-name"
            value={name}
            maxLength={64}
            onChange={(event) => setName(event.target.value)}
            placeholder="Token name"
            disabled={issuance.isPending || issuance.issued !== null}
          />
        </div>
        <Button type="submit" size="sm" disabled={creationDisabled}>
          {issuance.isPending ? "Creating..." : "Create"}
        </Button>
      </form>

      {issuance.issued && (
        <p className="text-xs text-muted-foreground" role="status">
          Save or dismiss the current secret before creating another token.
        </p>
      )}

      <TokenList
        query={tokensQuery}
        revokingTokenId={revokeToken.variables?.tokenId}
        revokePending={revokeToken.isPending}
        onRevoke={(tokenId, label) => {
          if (
            window.confirm(
              `Revoke ${label}? Clients using it will stop working.`,
            )
          ) {
            revokeToken.mutate({ tokenId });
          }
        }}
      />
    </section>
  );
}

function TokenList({
  query,
  revokingTokenId,
  revokePending,
  onRevoke,
}: {
  query: UseQueryResult<readonly ApiToken[], Error>;
  revokingTokenId: number | undefined;
  revokePending: boolean;
  onRevoke: (tokenId: number, label: string) => void;
}) {
  if (query.isPending) {
    return (
      <p className="text-xs text-muted-foreground" role="status">
        Loading tokens...
      </p>
    );
  }

  if (query.isError) {
    return (
      <div className="flex items-center justify-between gap-3" role="alert">
        <p className="text-xs text-destructive">Could not load tokens.</p>
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

  if (query.data.length === 0) {
    return <p className="text-xs text-muted-foreground">No tokens yet.</p>;
  }

  return (
    <ul className="space-y-2">
      {query.data.map((token) => {
        const label = `${token.name} (${token.prefix}…)`;
        const isRevoking = revokePending && revokingTokenId === token.id;
        return (
          <li
            key={token.id}
            className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-muted/30 px-3 py-2"
          >
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{token.name}</p>
              <p className="font-mono text-xs text-muted-foreground">
                {token.prefix}…
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-8 shrink-0"
              aria-label={`Revoke ${label}`}
              disabled={revokePending}
              onClick={() => onRevoke(token.id, label)}
            >
              <Trash2 className="size-4" />
              {isRevoking && <span className="sr-only">Revoking</span>}
            </Button>
          </li>
        );
      })}
    </ul>
  );
}

function IssuedTokenCard({
  token,
  dismissDisabled,
  onDismiss,
}: {
  token: CreatedApiToken;
  dismissDisabled: boolean;
  onDismiss: () => void;
}) {
  const copyButtonRef = React.useRef<HTMLButtonElement>(null);
  const credentials = [
    `api_url=${API_URL}`,
    `token=${token.token}`,
    `app_url=${window.location.origin}`,
  ].join("\n");

  React.useEffect(() => {
    copyButtonRef.current?.focus();
  }, []);

  return (
    <div className="space-y-2 rounded-md border border-primary/30 bg-primary/5 p-3">
      <p className="text-xs text-muted-foreground">
        Save this to{" "}
        <span className="font-mono text-foreground">
          ~/.config/toki/credentials
        </span>{" "}
        with mode 600. It will not be shown again after dismissal.
      </p>
      <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-background/80 p-2 font-mono text-[11px] leading-5">
        {credentials}
      </pre>
      <div className="flex gap-2">
        <Button
          ref={copyButtonRef}
          type="button"
          variant="outline"
          size="sm"
          className="flex-1"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(credentials);
              toast.success("Credentials copied");
            } catch {
              toast.error("Failed to copy credentials");
            }
          }}
        >
          <ClipboardCopy className="size-4" />
          Copy credentials
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onDismiss}
          disabled={dismissDisabled}
        >
          Dismiss
        </Button>
      </div>
    </div>
  );
}
