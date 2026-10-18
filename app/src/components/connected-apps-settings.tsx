import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import { PlugZap, Unplug } from "lucide-react";
import { toast } from "sonner";
import { Button } from "./ui/button";
import { Separator } from "./ui/separator";
import { apiErrorToast } from "@/lib/api/errors";
import { userMutations } from "@/lib/api/mutations/user";
import { userQueries, type OAuthGrant } from "@/lib/api/queries/user";

dayjs.extend(relativeTime);

/** Account-settings section for listing and disconnecting MCP OAuth apps. */
export function ConnectedAppsSettings() {
  const grantsQuery = useQuery(userQueries.oauthGrants());
  const revokeGrant = userMutations.useRevokeOAuthGrant({
    onSuccess: () => {
      toast.success("App disconnected");
    },
    onError: apiErrorToast("Failed to disconnect app"),
  });

  return (
    <section className="space-y-3" aria-labelledby="connected-apps-heading">
      <Separator />
      <div className="space-y-1">
        <h3
          id="connected-apps-heading"
          className="flex items-center gap-2 text-sm font-medium"
        >
          <PlugZap className="size-4" />
          Connected apps
        </h3>
        <p className="text-xs text-muted-foreground">
          Apps you connected to Toki over MCP, such as Claude or ChatGPT.
          Disconnect one to revoke its access.
        </p>
      </div>

      <GrantList
        query={grantsQuery}
        revokingGrantId={revokeGrant.variables?.grantId}
        revokePending={revokeGrant.isPending}
        onRevoke={(grantId, name) => {
          if (
            window.confirm(`Disconnect ${name}? It will have to connect again.`)
          ) {
            revokeGrant.mutate({ grantId });
          }
        }}
      />
    </section>
  );
}

function GrantList({
  query,
  revokingGrantId,
  revokePending,
  onRevoke,
}: {
  query: UseQueryResult<readonly OAuthGrant[], Error>;
  revokingGrantId: number | undefined;
  revokePending: boolean;
  onRevoke: (grantId: number, name: string) => void;
}) {
  if (query.isPending) {
    return (
      <p className="text-xs text-muted-foreground" role="status">
        Loading connected apps...
      </p>
    );
  }

  if (query.isError) {
    return (
      <div className="flex items-center justify-between gap-3" role="alert">
        <p className="text-xs text-destructive">
          Could not load connected apps.
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

  if (query.data.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">No connected apps yet.</p>
    );
  }

  return (
    <ul className="space-y-2">
      {query.data.map((grant) => {
        const isRevoking = revokePending && revokingGrantId === grant.id;
        return (
          <li
            key={grant.id}
            className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-muted/30 px-3 py-2"
          >
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{grant.clientName}</p>
              <p className="text-xs break-words text-muted-foreground">
                {grant.clientHost ?? (
                  <>
                    <span className="font-medium">Unverified</span>
                    {" · "}
                    {grant.redirectDestination}
                  </>
                )}
                {" · "}
                connected {dayjs(grant.createdAt).format("MMM D, YYYY")}, last
                active {dayjs(grant.lastUsedAt).fromNow()}
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-8 shrink-0"
              aria-label={`Disconnect ${grant.clientName}`}
              disabled={revokePending}
              onClick={() => onRevoke(grant.id, grant.clientName)}
            >
              <Unplug className="size-4" />
              {isRevoking && <span className="sr-only">Disconnecting</span>}
            </Button>
          </li>
        );
      })}
    </ul>
  );
}
