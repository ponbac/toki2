import { Link } from "@tanstack/react-router";
import { AlertTriangle } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import type { AiBillingOverview } from "@/lib/api/queries/ai-admin";
import type { AiProvider } from "@/lib/api/queries/ai-usage";
import { cn } from "@/lib/utils";
import { formatConverted, UNASSIGNED_LABEL } from "../../-lib/billing";
import { formatEstimate, formatShare, listOf } from "../../-lib/format";
import { splitProjectName, type PersonRow } from "../../-lib/ledger";
import type { UploadVerdict } from "../../-lib/upload-health";
import { VerdictBadge } from "./uploads-panel";

/** Projects named in a row before the rest are counted. */
const NAMED_PROJECTS = 2;

/**
 * Everyone on the month's bill, dearest first: what their usage bills and
 * its share of the bill, which plans pay for it, whether their uploads are
 * in, what it would have cost at API prices, and what they kept out of the
 * bill as not client work.
 */
export function PeopleTable({
  overview,
  people,
  verdicts,
  undeclared,
}: {
  overview: AiBillingOverview;
  people: readonly PersonRow[];
  /** Each developer's uploads, once known. */
  verdicts: ReadonlyMap<number, UploadVerdict> | undefined;
  /** Tools each developer used on a plan nobody declared this month. */
  undeclared: ReadonlyMap<number, readonly AiProvider[]>;
}) {
  const currency = overview.billingCurrency;
  const billed = overview.totals.converted.billed;
  const whole = billed === null ? null : Number(billed) * 100;

  return (
    <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="w-[30%]">Developer</TableHead>
            <TableHead className="hidden md:table-cell">Paid by</TableHead>
            <TableHead className="hidden lg:table-cell">Uploads</TableHead>
            <TableHead className="text-right whitespace-nowrap">
              Billed ({currency})
            </TableHead>
            <TableHead className="hidden text-right whitespace-nowrap md:table-cell">
              At API prices
            </TableHead>
            <TableHead
              className="hidden text-right whitespace-nowrap xl:table-cell"
              title={`${PERSONAL_LABEL} usage, left out of the bill`}
            >
              Not client work
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {people.map((person) => {
            const amount = person.totals?.converted ?? null;
            const share =
              whole === null || whole === 0 || amount === null
                ? null
                : amount / whole;
            const verdict = verdicts?.get(person.userId);
            return (
              <TableRow key={person.userId}>
                <TableCell className="py-2.5">
                  <Link
                    to="/admin/ai-billing/$userId"
                    params={{ userId: String(person.userId) }}
                    search={{ month: overview.month }}
                    className="font-medium underline-offset-4 hover:underline"
                  >
                    {person.name}
                  </Link>
                  <div className="truncate text-xs text-muted-foreground">
                    {projectsLine(person)}
                  </div>
                </TableCell>
                <TableCell className="hidden md:table-cell">
                  <PaidBy
                    person={person}
                    undeclared={undeclared.get(person.userId) ?? []}
                  />
                </TableCell>
                <TableCell className="hidden lg:table-cell">
                  {verdict && <VerdictBadge verdict={verdict} />}
                </TableCell>
                <TableCell
                  className={cn(
                    "text-right font-semibold tabular-nums sm:whitespace-nowrap",
                    person.totals === null &&
                      "font-normal text-muted-foreground",
                  )}
                >
                  {person.totals === null
                    ? "—"
                    : formatConverted(
                        person.totals,
                        currency,
                        overview.pendingRates,
                      )}
                  {share !== null && share > 0 && (
                    <div className="text-xs font-normal text-muted-foreground">
                      {formatShare(share)} of the bill
                    </div>
                  )}
                </TableCell>
                <TableCell className="hidden text-right whitespace-nowrap text-muted-foreground tabular-nums md:table-cell">
                  {person.totals === null || person.totals.usage.records === 0
                    ? "—"
                    : formatEstimate(person.totals.usage)}
                </TableCell>
                <TableCell className="hidden text-right whitespace-nowrap text-muted-foreground tabular-nums xl:table-cell">
                  {person.personal === null
                    ? "—"
                    : formatEstimate(person.personal)}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}

/** The customers a developer's usage bills, the dearest first. */
function projectsLine(person: PersonRow): string {
  const customers = [
    ...new Set(person.projects.map((name) => splitProjectName(name).customer)),
  ];
  const named = customers.slice(0, NAMED_PROJECTS);
  const rest = customers.length - named.length;
  const parts = [
    ...named,
    ...(rest > 0 ? [`${rest} more`] : []),
    ...(person.hasUnassigned ? [UNASSIGNED_LABEL.toLowerCase()] : []),
  ];
  if (parts.length === 0) {
    return person.totals === null ? "Nothing billed" : "No project";
  }
  return parts.join(", ");
}

/** The plans that paid for a developer's month, or API prices without one,
 * and any plan a tool reported that nobody declared. */
function PaidBy({
  person,
  undeclared,
}: {
  person: PersonRow;
  undeclared: readonly AiProvider[];
}) {
  return (
    <span className="flex flex-col gap-0.5">
      {person.plans.length === 0 ? (
        <span className="text-sm text-muted-foreground">
          {person.totals === null ? "—" : "API prices"}
        </span>
      ) : (
        person.plans.map((plan) => (
          <span
            key={plan.subscriptionId}
            className="inline-flex items-center gap-1.5 text-sm whitespace-nowrap"
          >
            <AiProviderIcon provider={plan.provider} />
            {plan.plan}
            {plan.coveredDays < plan.daysInMonth && (
              <span className="text-xs text-muted-foreground">
                {plan.coveredDays} of {plan.daysInMonth} days
              </span>
            )}
          </span>
        ))
      )}
      {undeclared.length > 0 && (
        <span className="inline-flex items-center gap-1 text-xs whitespace-nowrap text-[#a86f00] dark:text-[#fab219]">
          <AlertTriangle className="size-3.5 shrink-0" aria-hidden />
          {listOf(
            undeclared.map((provider) => AI_PROVIDER_LABELS[provider]),
            "conjunction",
          )}{" "}
          plan not declared
        </span>
      )}
    </span>
  );
}
