import { PERSONAL_LABEL } from "@/lib/ai-usage";
import type {
  AiBillingDeveloper,
  AiBillingUsage,
} from "@/lib/api/queries/ai-admin";
import { addUsage, emptyUsage } from "../../-lib/daily";
import { formatEstimate, formatTokens } from "../../-lib/format";

/**
 * Usage the bill leaves out because its developer marked the repository as
 * not client work: an estimate only, and never the repositories' names.
 * Renders nothing without such usage. `developers` names whose it is; omit
 * it on a page about one developer.
 */
export function PersonalUsage({
  usage,
  developers,
}: {
  usage: readonly Readonly<{ userId: number; usage: AiBillingUsage }>[];
  developers?: readonly AiBillingDeveloper[];
}) {
  if (usage.length === 0) {
    return null;
  }
  const total = usage.reduce(
    (sum, personal) => addUsage(sum, personal.usage),
    emptyUsage(),
  );
  const names = new Map(
    developers?.map((developer) => [developer.userId, developer.fullName]),
  );

  return (
    <section
      aria-label={`${PERSONAL_LABEL} usage`}
      className="rounded-xl border border-dashed border-border/80 p-4"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h3 className="text-sm font-medium">
          {PERSONAL_LABEL}: left out of the bill
        </h3>
        <p className="text-sm tabular-nums">
          <span className="font-semibold">{formatEstimate(total)}</span>
          <span className="text-muted-foreground">
            {" "}
            · {formatTokens(total.tokens.total)} tokens
          </span>
        </p>
      </div>
      <p className="mt-1 text-xs text-muted-foreground">
        Usage of repositories a developer marked as not client work. It bills
        nothing and takes no share of a subscription fee; this is what it would
        have cost at API prices. The repositories are not mapped, so their names
        are not shown here.
      </p>
      {developers && (
        <ul className="mt-2 flex flex-wrap gap-x-4 gap-y-0.5 text-xs">
          {usage.map((personal) => (
            <li key={personal.userId} className="tabular-nums">
              <span className="text-muted-foreground">
                {names.get(personal.userId) ?? "Unknown developer"}
              </span>{" "}
              {formatEstimate(personal.usage)}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
