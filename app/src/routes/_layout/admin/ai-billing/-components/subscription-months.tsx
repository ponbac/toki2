import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiBillingDeveloper,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import { allocationLabel } from "../../-lib/billing";
import { formatEstimate, formatFee, shortDate } from "../../-lib/format";

/** How each subscription's fee was pro-rated and split this month. */
export function SubscriptionMonths({
  subscriptions,
  developers,
  billingCurrency,
  showDeveloper = true,
}: {
  subscriptions: readonly AiSubscriptionMonth[];
  developers: readonly AiBillingDeveloper[];
  billingCurrency: string;
  showDeveloper?: boolean;
}) {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  if (subscriptions.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No subscription covers days of this month; all usage bills at its
        estimated API cost.
      </p>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          {showDeveloper && <TableHead>Developer</TableHead>}
          <TableHead>Plan</TableHead>
          <TableHead>Covers</TableHead>
          <TableHead className="text-right">Monthly fee</TableHead>
          <TableHead className="text-right">Pro-rated fee</TableHead>
          <TableHead>Split</TableHead>
          <TableHead className="text-right">API-equivalent</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {subscriptions.map((subscription) => (
          <TableRow key={subscription.subscriptionId}>
            {showDeveloper && (
              <TableCell className="font-medium">
                {names.get(subscription.userId) ??
                  `User ${subscription.userId}`}
              </TableCell>
            )}
            <TableCell>
              <div>{subscription.plan}</div>
              <div className="text-xs text-muted-foreground">
                {AI_PROVIDER_LABELS[subscription.provider]}
              </div>
            </TableCell>
            <TableCell className="tabular-nums">
              {shortDate(subscription.coveredFrom)}–
              {shortDate(subscription.coveredTo)}
              <div className="text-xs text-muted-foreground">
                {subscription.coveredDays} of {subscription.daysInMonth} days
              </div>
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {formatFee(subscription.monthlyCost, subscription.currency)}
            </TableCell>
            <TableCell className="text-right tabular-nums">
              <div className="font-semibold">
                {subscription.convertedProratedFee === null ? (
                  <span className="text-[#c98500] dark:text-[#fab219]">
                    No rate
                  </span>
                ) : (
                  formatFee(subscription.convertedProratedFee, billingCurrency)
                )}
              </div>
              {subscription.currency !== billingCurrency && (
                <div className="text-xs text-muted-foreground">
                  {formatFee(subscription.proratedFee, subscription.currency)}
                  {subscription.exchangeRate !== null &&
                    ` at ${subscription.exchangeRate}`}
                </div>
              )}
            </TableCell>
            <TableCell>
              {subscription.allocation === "apiCost" ? (
                <span className="text-sm">
                  {allocationLabel(subscription.allocation)}
                  {subscription.usage.unpricedRecords > 0 && (
                    <span className="block text-xs text-muted-foreground">
                      {subscription.usage.unpricedRecords} unpriced records
                      carry no weight
                    </span>
                  )}
                </span>
              ) : (
                <StatusBadge
                  tone="warning"
                  label={allocationLabel(subscription.allocation)}
                />
              )}
            </TableCell>
            <TableCell className="text-right tabular-nums text-muted-foreground">
              {subscription.allocation === "unallocated"
                ? "—"
                : formatEstimate(subscription.usage)}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
