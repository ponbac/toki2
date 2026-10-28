import { Link } from "@tanstack/react-router";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type {
  AiAdjustedMonth,
  AiAdjustmentLine,
  AiBillingDeveloper,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import {
  adjustedMonthsOf,
  adjustmentKindLabel,
  adjustmentProject,
  formatLineCredits,
  formatSignedHundredths,
  groupAdjustments,
  hundredths,
  sekOnlyReason,
  subjectLabel,
  type AdjustmentGroup,
} from "../../-lib/billing";
import { listOf, monthLabel } from "../../-lib/format";

/**
 * The month's adjustment lines: changes to closed months since they closed,
 * each billed once, here, at the rate of the month it corrects. Grouped by
 * the project they correct, as an invoice needs them, with subtotals per
 * currency and in the billing currency.
 */
export function AdjustmentsSection({
  month,
  billingCurrency,
  lines,
  adjustedMonths,
  developers,
  totals,
}: {
  month: string;
  billingCurrency: string;
  lines: readonly AiAdjustmentLine[];
  adjustedMonths: readonly AiAdjustedMonth[];
  developers: readonly AiBillingDeveloper[];
  /** The bill's own totals of its adjustment lines. */
  totals: Readonly<{
    amounts: readonly { currency: string; amount: string }[];
    converted: string | null;
  }>;
}) {
  const groups = groupAdjustments(lines);
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const changed = new Set(lines.map((line) => line.adjustsMonth));
  const unchanged = adjustedMonths
    .filter((adjusted) => !changed.has(adjusted.month))
    .map((adjusted) => monthLabel(adjusted.month));

  return (
    <section aria-labelledby="adjustments-heading" className="space-y-4">
      <div className="max-w-3xl space-y-1">
        <h3 id="adjustments-heading" className="text-base font-semibold">
          Adjustments
        </h3>
        <p className="text-sm text-muted-foreground">
          What changed in closed months since they closed: a mapping, a
          subscription, GitHub&apos;s credits or a rate. Each change bills once,
          here, as the difference to what was invoiced, in {billingCurrency} at
          the rate of the month it corrects. A credit is negative.
        </p>
      </div>
      {unchanged.length > 0 && (
        <p className="text-sm text-muted-foreground">
          No change to {listOf(unchanged, "conjunction")} since{" "}
          {unchanged.length === 1 ? "it" : "they"} closed.
        </p>
      )}
      {lines.length > 0 && (
        <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Project and line</TableHead>
                <TableHead>Corrects</TableHead>
                <TableHead className="text-right">Amount</TableHead>
                <TableHead className="text-right">
                  In {billingCurrency}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {groups.map((group) => (
                <AdjustmentGroupRows
                  key={group.key}
                  month={month}
                  group={group}
                  names={names}
                  billingCurrency={billingCurrency}
                />
              ))}
            </TableBody>
            <TableFooter>
              <TableRow>
                <TableCell colSpan={2} className="font-semibold">
                  Adjustments in total
                </TableCell>
                <TableCell className="text-right font-semibold tabular-nums">
                  {totals.amounts.length === 0
                    ? "—"
                    : totals.amounts
                        .map(({ currency, amount }) =>
                          formatSignedHundredths(hundredths(amount), currency),
                        )
                        .join(" + ")}
                </TableCell>
                <TableCell className="text-right font-semibold tabular-nums">
                  {totals.converted === null ? (
                    <NoRate />
                  ) : (
                    formatSignedHundredths(
                      hundredths(totals.converted),
                      billingCurrency,
                    )
                  )}
                </TableCell>
              </TableRow>
            </TableFooter>
          </Table>
        </div>
      )}
    </section>
  );
}

function NoRate() {
  return <span className="text-[#c98500] dark:text-[#fab219]">No rate</span>;
}

function AdjustmentGroupRows({
  month,
  group,
  names,
  billingCurrency,
}: {
  month: string;
  group: AdjustmentGroup;
  names: ReadonlyMap<number, string>;
  billingCurrency: string;
}) {
  const amounts = [...group.amounts.entries()].sort(([a], [b]) =>
    a.localeCompare(b),
  );
  return (
    <>
      <TableRow className="bg-muted/30 hover:bg-muted/30">
        <TableCell colSpan={2} className="font-semibold">
          {group.label}
        </TableCell>
        <TableCell className="text-right font-semibold tabular-nums">
          {amounts
            .map(([currency, amount]) =>
              formatSignedHundredths(amount, currency),
            )
            .join(" + ")}
        </TableCell>
        <TableCell className="text-right font-semibold tabular-nums">
          {group.converted === null ? (
            <NoRate />
          ) : (
            formatSignedHundredths(group.converted, billingCurrency)
          )}
        </TableCell>
      </TableRow>
      {group.lines.map((line, index) => (
        <AdjustmentRow
          key={`${group.key}:${index}`}
          month={month}
          line={line}
          names={names}
          billingCurrency={billingCurrency}
        />
      ))}
    </>
  );
}

/** One adjustment line: what it corrects, whose, and by how much. */
export function AdjustmentRow({
  month,
  line,
  names,
  billingCurrency,
  showProject = false,
  linkDeveloper = true,
}: {
  /** The month on screen, which the line is posted on or frozen in. */
  month: string;
  line: AiAdjustmentLine;
  names: ReadonlyMap<number, string>;
  billingCurrency: string;
  /** Name the project in the row, for a list not grouped by project. */
  showProject?: boolean;
  /** Link a developer's name to their page for the month on screen. */
  linkDeveloper?: boolean;
}) {
  const subject = subjectLabel(line.subject, names);
  const detail = [
    adjustmentKindLabel(line),
    ...(line.plan === null ? [] : [line.plan]),
    ...(line.credits === null ? [] : [formatLineCredits(line.credits)]),
  ].join(" · ");
  return (
    <TableRow>
      <TableCell className="pl-6">
        <div className="flex items-center gap-1.5 font-medium">
          <AiProviderIcon provider={line.provider} className="size-3.5" />
          {linkDeveloper && line.subject.kind === "user" ? (
            <Link
              to="/admin/ai-billing/$userId"
              params={{ userId: String(line.subject.userId) }}
              search={{ month }}
              className="underline-offset-4 hover:underline"
            >
              {subject}
            </Link>
          ) : (
            subject
          )}
          {showProject && (
            <span className="font-normal text-muted-foreground">
              · {adjustmentProject(line).label}
            </span>
          )}
        </div>
        <div className="text-xs text-muted-foreground">{detail}</div>
      </TableCell>
      <TableCell className="text-sm">
        <Link
          to="/admin/ai-billing"
          search={{ month: line.adjustsMonth }}
          className="underline-offset-2 hover:underline"
          title={`Open ${monthLabel(line.adjustsMonth)} as it was invoiced (snapshot #${line.snapshotId})`}
          disabled={line.adjustsMonth === month}
        >
          {monthLabel(line.adjustsMonth)}
        </Link>
      </TableCell>
      <TableCell
        className={cn(
          "text-right tabular-nums",
          line.sekOnly && "text-muted-foreground",
        )}
      >
        {formatSignedHundredths(
          hundredths(line.billableAmount),
          line.billableCurrency,
        )}
      </TableCell>
      <TableCell className="text-right tabular-nums">
        <div className="flex items-center justify-end gap-1.5">
          {line.sekOnly && (
            <span
              className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none font-semibold text-muted-foreground"
              title={
                sekOnlyReason(
                  line,
                  billingCurrency,
                  monthLabel(line.adjustsMonth),
                ) ?? undefined
              }
            >
              {billingCurrency} only
            </span>
          )}
          {line.convertedAmount === null ? (
            <NoRate />
          ) : (
            formatSignedHundredths(
              hundredths(line.convertedAmount),
              billingCurrency,
            )
          )}
        </div>
        {line.exchangeRate !== null && (
          <div className="text-xs text-muted-foreground">
            at {line.exchangeRate} ({monthLabel(line.adjustsMonth)}&apos;s rate)
          </div>
        )}
      </TableCell>
    </TableRow>
  );
}

/** A developer's adjustment lines in one small table, for their page. */
export function DeveloperAdjustments({
  month,
  lines,
  names,
  billingCurrency,
}: {
  month: string;
  lines: readonly AiAdjustmentLine[];
  names: ReadonlyMap<number, string>;
  billingCurrency: string;
}) {
  const months = adjustedMonthsOf(lines).map(monthLabel);
  return (
    <section
      aria-labelledby="developer-adjustments-heading"
      className="flex flex-col gap-2"
    >
      <div>
        <h3
          id="developer-adjustments-heading"
          className="text-base font-semibold"
        >
          Adjustments
        </h3>
        <p className="text-sm text-muted-foreground">
          Changes to {listOf(months, "conjunction")} since{" "}
          {months.length === 1 ? "it" : "they"} closed, billed on{" "}
          {monthLabel(month)}&apos;s bill at the rate of the month each
          corrects.
        </p>
      </div>
      <div className="overflow-hidden rounded-xl border border-border/60 bg-card/60">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Line</TableHead>
              <TableHead>Corrects</TableHead>
              <TableHead className="text-right">Amount</TableHead>
              <TableHead className="text-right">In {billingCurrency}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {lines.map((line, index) => (
              <AdjustmentRow
                key={index}
                month={month}
                line={line}
                names={names}
                billingCurrency={billingCurrency}
                showProject
                linkDeveloper={false}
              />
            ))}
          </TableBody>
        </Table>
      </div>
    </section>
  );
}
