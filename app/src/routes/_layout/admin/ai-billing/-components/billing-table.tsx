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
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import {
  formatBillable,
  formatConverted,
  groupLines,
  lineBillable,
  lineConverted,
  lineProject,
  lineWarnings,
} from "../../-lib/billing";
import { formatEstimate, formatTokens } from "../../-lib/format";

/**
 * Month × developer × project: every billing line, grouped by project (for
 * invoicing) or by developer. "Billed" is in the billing currency (SEK), and
 * "Original" the fee share for subscription lines or the API-equivalent
 * estimate in whole cents for API lines, in its own currency.
 */
export function BillingTable({
  month,
  billingCurrency,
  pendingRates = [],
  lines,
  developers,
  subscriptions,
  groupBy,
  linkDevelopers = true,
}: {
  month: string;
  billingCurrency: string;
  /** Currencies whose rate is still being fetched. */
  pendingRates?: readonly string[];
  lines: readonly AiBillingLine[];
  developers: readonly AiBillingDeveloper[];
  subscriptions: readonly AiSubscriptionMonth[];
  groupBy: "project" | "developer";
  /** Off for one developer's own bill: no developer column or links. */
  linkDevelopers?: boolean;
}) {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const plans = new Map(subscriptions.map((s) => [s.subscriptionId, s]));
  const groups = groupLines(lines, developers, groupBy);
  const detailHeading = groupBy === "project" ? "Developer" : "Project";

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-[28%]">
            {groupBy === "project" ? "Project" : "Developer"}
            {linkDevelopers || groupBy === "developer"
              ? ` / ${detailHeading.toLowerCase()}`
              : ""}
          </TableHead>
          <TableHead>Provider</TableHead>
          <TableHead>Billing</TableHead>
          <TableHead className="text-right">
            Billed ({billingCurrency})
          </TableHead>
          <TableHead className="text-right">Original</TableHead>
          <TableHead className="text-right">API-equivalent</TableHead>
          <TableHead className="text-right">Tokens</TableHead>
        </TableRow>
      </TableHeader>
      {groups.map((group) => (
        <TableBody key={group.key} className="border-b border-border/60">
          <TableRow className="bg-muted/30 hover:bg-muted/30">
            <TableCell className="font-semibold">
              {group.userId !== undefined && linkDevelopers ? (
                <DeveloperLink month={month} userId={group.userId}>
                  {group.label}
                </DeveloperLink>
              ) : (
                <span
                  className={cn(
                    group.kind !== "project" &&
                      group.kind !== "developer" &&
                      "italic",
                  )}
                >
                  {group.label}
                </span>
              )}
            </TableCell>
            <TableCell />
            <TableCell />
            <TableCell
              className={cn(
                "text-right font-semibold tabular-nums",
                group.totals.converted === null &&
                  "text-[#c98500] dark:text-[#fab219]",
              )}
            >
              {formatConverted(group.totals, billingCurrency, pendingRates)}
            </TableCell>
            <TableCell className="text-right tabular-nums text-muted-foreground">
              {formatBillable(group.totals)}
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {group.totals.usage.records > 0
                ? formatEstimate(group.totals.usage)
                : "—"}
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {group.totals.usage.records > 0
                ? formatTokens(group.totals.usage.tokens.total)
                : "—"}
            </TableCell>
          </TableRow>
          {group.lines.map((line, index) => {
            const subscription =
              line.subscriptionId !== null
                ? plans.get(line.subscriptionId)
                : undefined;
            const warnings = lineWarnings(
              line,
              subscription,
              billingCurrency,
              pendingRates,
            );
            const detail =
              groupBy === "developer" ? (
                lineProject(line).label
              ) : linkDevelopers ? (
                <DeveloperLink month={month} userId={line.userId}>
                  {names.get(line.userId) ?? `User ${line.userId}`}
                </DeveloperLink>
              ) : null;
            return (
              <TableRow key={`${group.key}-${index}`}>
                <TableCell className="pl-6">{detail}</TableCell>
                <TableCell>{AI_PROVIDER_LABELS[line.provider]}</TableCell>
                <TableCell>
                  <span className="inline-flex items-center gap-1.5">
                    {line.billingMode === "api"
                      ? "API"
                      : `Subscription${subscription ? ` · ${subscription.plan}` : ""}`}
                    {warnings.length > 0 && <Warnings warnings={warnings} />}
                  </span>
                </TableCell>
                <TableCell
                  className={cn(
                    "text-right tabular-nums",
                    line.missingRate && "text-[#c98500] dark:text-[#fab219]",
                  )}
                >
                  {lineConverted(line, billingCurrency, pendingRates)}
                </TableCell>
                <TableCell className="text-right tabular-nums text-muted-foreground">
                  {lineBillable(line)}
                  {line.exchangeRate !== null && (
                    <span className="block text-xs">
                      at {line.exchangeRate}
                    </span>
                  )}
                </TableCell>
                <TableCell className="text-right tabular-nums text-muted-foreground">
                  {line.unallocatedOverhead ? "—" : formatEstimate(line.usage)}
                </TableCell>
                <TableCell className="text-right tabular-nums text-muted-foreground">
                  {line.unallocatedOverhead
                    ? "—"
                    : formatTokens(line.usage.tokens.total)}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      ))}
    </Table>
  );
}

function DeveloperLink({
  month,
  userId,
  children,
}: {
  month: string;
  userId: number;
  children: React.ReactNode;
}) {
  return (
    <Link
      to="/admin/ai-billing/$userId"
      params={{ userId: String(userId) }}
      search={{ month }}
      className="underline-offset-4 hover:underline"
    >
      {children}
    </Link>
  );
}

function Warnings({ warnings }: { warnings: readonly string[] }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex rounded-sm text-[#c98500] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring dark:text-[#fab219]"
          aria-label={warnings.join(" ")}
        >
          <AlertTriangle className="size-3.5" />
        </button>
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">
        {warnings.map((warning) => (
          <p key={warning}>{warning}</p>
        ))}
      </TooltipContent>
    </Tooltip>
  );
}
