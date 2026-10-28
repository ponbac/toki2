import type { AiMeteredCheck } from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import { checkState } from "../../-lib/reconciliation";

/**
 * The last comparison of stored days with GitHub's month total, with exact
 * quantities. `subject` names whose days they are when the line stands
 * apart from its account, such as the org total.
 */
export function CheckLine({
  check,
  subject,
}: {
  check: AiMeteredCheck | null;
  subject?: string;
}) {
  const lead = subject !== undefined && (
    <span className="font-medium text-foreground">{subject}:</span>
  );
  if (check === null) {
    return (
      <p className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
        {lead}
        <span>Not compared with GitHub&apos;s month total yet.</span>
      </p>
    );
  }
  const state = checkState(check);
  const checkedAt = new Date(check.checkedAt).toLocaleString("en-GB");
  if (state === "failed") {
    return (
      <p className="flex flex-wrap items-center gap-1.5 text-xs">
        {lead}
        <StatusBadge tone="critical" label="Couldn't check with GitHub" />
        <span className="text-muted-foreground">
          {check.error} ({checkedAt})
        </span>
      </p>
    );
  }
  return (
    <p className="flex flex-wrap items-center gap-1.5 text-xs">
      {lead}
      {state === "equal" ? (
        <StatusBadge tone="good" label="Equals GitHub's month total" />
      ) : (
        <StatusBadge
          tone="critical"
          label="Differs from GitHub's month total"
        />
      )}
      <span className="text-muted-foreground">
        GitHub{" "}
        <span className="font-mono text-foreground">
          {check.payerGrossQuantity}
        </span>{" "}
        credits, stored days{" "}
        <span className="font-mono text-foreground">
          {check.storedGrossQuantity}
        </span>{" "}
        credits, checked {checkedAt}
      </span>
    </p>
  );
}
