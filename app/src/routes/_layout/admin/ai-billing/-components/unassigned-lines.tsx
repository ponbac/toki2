import { Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import type {
  AiBillingDeveloper,
  AiBillingLine,
} from "@/lib/api/queries/ai-admin";
import {
  formatConverted,
  groupLines,
  unassignedLines,
} from "../../-lib/billing";
import { plural } from "../../-lib/format";

/**
 * Whose usage bills as Unassigned and how much, with the way to the project
 * mappings that would give it a project. Says so when nothing is unassigned.
 */
export function UnassignedLines({
  month,
  billingCurrency,
  pendingRates,
  lines,
  developers,
}: {
  month: string;
  billingCurrency: string;
  pendingRates: readonly string[];
  lines: readonly AiBillingLine[];
  developers: readonly AiBillingDeveloper[];
}) {
  const groups = groupLines(unassignedLines(lines), developers, "developer");

  return (
    <div className="flex flex-col items-start gap-3">
      <p className="text-sm text-muted-foreground">
        {groups.length === 0
          ? "Every billed line is charged to a project. Usage of a repository nobody has mapped would bill as Unassigned."
          : "Usage whose project key is unmapped, stale or unattributed bills as Unassigned, and no project can be invoiced for it. Map its keys and the bill updates."}
      </p>
      {groups.length > 0 && (
        <dl className="grid w-full grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-1 text-sm">
          {groups.map((group) => (
            <div key={group.key} className="contents">
              <dt className="min-w-0 truncate">
                {group.userId !== undefined ? (
                  <Link
                    to="/admin/ai-billing/$userId"
                    params={{ userId: String(group.userId) }}
                    search={{ month }}
                    className="underline-offset-4 hover:underline"
                  >
                    {group.label}
                  </Link>
                ) : (
                  group.label
                )}
                <span className="text-muted-foreground">
                  , {plural(group.lines.length, "line")}
                </span>
              </dt>
              <dd className="text-right font-semibold tabular-nums">
                {formatConverted(group.totals, billingCurrency, pendingRates)}
              </dd>
            </div>
          ))}
        </dl>
      )}
      <Button asChild variant="outline" size="sm">
        <Link to="/admin/project-mappings">Open project mappings</Link>
      </Button>
    </div>
  );
}
