import * as React from "react";
import { toast } from "sonner";
import { apiErrorToast } from "@/lib/api/errors";
import { userMutations } from "@/lib/api/mutations/user";
import type {
  ApiTokenCapabilities,
  CreatedApiToken,
} from "@/lib/api/queries/user";

/** State for a one-time token secret that must survive dialog close/reopen. */
export type ApiTokenIssuance = Readonly<{
  issued: CreatedApiToken | null;
  isPending: boolean;
  issue: (name: string, capabilities: ApiTokenCapabilities) => void;
  dismiss: () => void;
}>;

/** Owns API-token issuance outside the conditionally mounted dialog content. */
export function useApiTokenIssuance(): ApiTokenIssuance {
  const [issued, setIssued] = React.useState<CreatedApiToken | null>(null);
  const issuedRef = React.useRef<CreatedApiToken | null>(null);
  const issuanceBlockedRef = React.useRef(false);
  const showCreateError = React.useMemo(
    () => apiErrorToast("Failed to create token"),
    [],
  );
  const createToken = userMutations.useCreateApiToken({
    onSuccess: (created) => {
      issuedRef.current = created;
      setIssued(created);
      toast.success("Token created");
    },
    onError: (error) => {
      issuanceBlockedRef.current = false;
      showCreateError(error);
    },
  });

  const issue = React.useCallback(
    (name: string, capabilities: ApiTokenCapabilities) => {
      if (issuanceBlockedRef.current || issuedRef.current) {
        return;
      }

      issuanceBlockedRef.current = true;
      createToken.mutate({ name, capabilities });
    },
    [createToken],
  );

  const dismiss = React.useCallback(() => {
    issuedRef.current = null;
    issuanceBlockedRef.current = false;
    setIssued(null);
    createToken.reset();
  }, [createToken]);

  return {
    issued,
    isPending: createToken.isPending,
    issue,
    dismiss,
  };
}
