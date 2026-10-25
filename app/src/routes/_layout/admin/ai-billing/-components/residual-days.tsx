import * as React from "react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import type {
  AiMeteredCheck,
  AiMeteredTotals,
  AiResidualDay,
  AiResidualStatus,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { StatusBadge } from "../../-components/status-badge";
import { monthLabel, weekdayDate } from "../../-lib/format";
import {
  checkState,
  creditHundredths,
  creditsAsUsd,
  formatCreditNumber,
  formatCredits,
  residualDaysToShow,
  residualStatusLabel,
} from "../../-lib/reconciliation";
import { CheckLine } from "./check-line";

const AMOUNT = "text-right tabular-nums whitespace-nowrap";

const STATUS_TONES: Readonly<
  Record<AiResidualStatus, "good" | "warning" | "neutral">
> = {
  settled: "good",
  provisional: "neutral",
  incomplete: "warning",
};

const STATUS_HINTS: Readonly<Record<AiResidualStatus, string>> = {
  settled:
    "Every slice was fetched after the day ended, the settle days passed and the org total equals GitHub's month total.",
  provisional:
    "This day may still change: it is recent, not fully settled, or the org total doesn't equal GitHub's month total yet.",
  incomplete:
    "A slice of this day was never fetched or failed, or the org total couldn't be checked: an import gap, not unknown account.",
};

/**
 * Unknown account, per UTC day: the organization's credits minus the sum of
 * every account's. Credits Toki can't ask GitHub about by login (an account
 * it never listed) show up here. The org total it rests on is checked
 * against GitHub's month total: no day settles until they are equal.
 */
export function ResidualDays({
  days,
  totals,
  month,
  orgCheck,
}: {
  days: readonly AiResidualDay[];
  totals: AiMeteredTotals;
  month: string;
  orgCheck: AiMeteredCheck | null;
}) {
  const [showAll, setShowAll] = React.useState(false);
  const notable = residualDaysToShow(days);
  const shown = showAll ? days : notable;
  const settledLeft = days.some(
    (day) => day.status === "settled" && creditHundredths(day.residual) !== 0,
  );
  const settledDays = days.filter((day) => day.status === "settled").length;
  const orgState = checkState(orgCheck);

  return (
    <section
      aria-labelledby="copilot-residual-heading"
      className="flex flex-col gap-2"
    >
      <div>
        <h3 id="copilot-residual-heading" className="text-base font-semibold">
          Unknown account
        </h3>
        <p className="text-sm text-muted-foreground">
          Org total minus the per-login sum, per UTC day. Days still settling
          can change; days with an import gap are not unknown account. No day
          settles until the org total equals GitHub&apos;s month total.
        </p>
      </div>
      {days.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
          No days of {monthLabel(month)} imported yet.
        </p>
      ) : (
        <>
          <p className="flex flex-wrap items-center gap-1.5 text-sm">
            {orgState === "failed" ? (
              <StatusBadge
                tone="critical"
                label="Couldn't check the org total: every day is an import gap"
              />
            ) : orgState === "differs" ? (
              <StatusBadge
                tone="warning"
                label="The org total differs from GitHub's month total: no day settles"
              />
            ) : orgState === "unchecked" ? (
              <StatusBadge
                tone="neutral"
                label="The org total isn't checked yet: no day settles"
              />
            ) : settledLeft ? (
              <StatusBadge
                tone="warning"
                label={`Unknown account on settled days: ${formatCredits(totals.unknownAccountSettled)}`}
              />
            ) : settledDays > 0 ? (
              <StatusBadge
                tone="good"
                label="Per-login sums equal the org total on every settled day"
              />
            ) : (
              <StatusBadge tone="neutral" label="No day has settled yet" />
            )}
          </p>
          <CheckLine check={orgCheck} subject="Org total" />
          {shown.length > 0 && (
            <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
              <Table>
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead>Day (UTC)</TableHead>
                    <TableHead className="hidden text-right whitespace-nowrap sm:table-cell">
                      Org total
                    </TableHead>
                    <TableHead className="hidden text-right whitespace-nowrap sm:table-cell">
                      Per-login sum
                    </TableHead>
                    <TableHead className="text-right">Residual</TableHead>
                    <TableHead>Status</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {shown.map((day) => (
                    <TableRow key={day.day} className="align-top">
                      <TableCell className="py-2">
                        <div className="flex flex-col">
                          <span className="whitespace-nowrap">
                            {weekdayDate(day.day)}
                          </span>
                          {day.groups.length > 0 && (
                            <span className="text-xs text-muted-foreground">
                              {day.groups
                                .map(
                                  (group) =>
                                    `${group.model || group.sku}: ${formatCreditNumber(group.residual)}`,
                                )
                                .join(", ")}
                            </span>
                          )}
                        </div>
                      </TableCell>
                      <TableCell
                        className={cn(AMOUNT, "hidden py-2 sm:table-cell")}
                      >
                        {formatCreditNumber(day.orgGross)}
                      </TableCell>
                      <TableCell
                        className={cn(AMOUNT, "hidden py-2 sm:table-cell")}
                      >
                        {formatCreditNumber(day.accountsGross)}
                      </TableCell>
                      <TableCell
                        className={cn(AMOUNT, "py-2 font-medium")}
                        title={creditsAsUsd(day.residual)}
                      >
                        {formatCreditNumber(day.residual)}
                      </TableCell>
                      <TableCell className="py-2">
                        <span className="flex flex-wrap gap-1">
                          <span title={STATUS_HINTS[day.status]}>
                            <StatusBadge
                              tone={STATUS_TONES[day.status]}
                              label={residualStatusLabel(day.status)}
                            />
                          </span>
                          {day.accountsExceedOrg && (
                            <StatusBadge
                              tone="warning"
                              label="Accounts above the org total"
                            />
                          )}
                        </span>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
          {notable.length < days.length && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="w-fit"
              onClick={() => setShowAll((current) => !current)}
            >
              {showAll
                ? "Show only days with a residual or still settling"
                : `Show all ${days.length} days`}
            </Button>
          )}
        </>
      )}
    </section>
  );
}
