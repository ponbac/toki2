import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type {
  AiMeteredMonth,
  AiMeteredRepository,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { StatusBadge } from "../../-components/status-badge";
import { monthLabel, plural } from "../../-lib/format";
import {
  chargeKindLabel,
  creditHundredths,
  formatCreditNumber,
  formatCredits,
  formatQuantity,
  formatUsdAmount,
} from "../../-lib/reconciliation";

const AMOUNT = "text-right tabular-nums whitespace-nowrap";

/**
 * The organization's rows per SKU and repository: code review credits per
 * repository, the IDE aggregate (never added to accounts), the cloud agent,
 * Actions and seats, with the code review check.
 */
export function RepositoryUsage({
  repositories,
  codeReview,
  month,
}: {
  repositories: readonly AiMeteredRepository[];
  codeReview: AiMeteredMonth["codeReview"];
  month: string;
}) {
  return (
    <section
      aria-labelledby="copilot-repositories-heading"
      className="flex flex-col gap-2"
    >
      <div>
        <h3
          id="copilot-repositories-heading"
          className="text-base font-semibold"
        >
          Per repository
        </h3>
        <p className="text-sm text-muted-foreground">
          GitHub&apos;s organization rows for {monthLabel(month)}. Code review
          credits are billed per repository, never per account; Actions minutes
          are never billed on.
        </p>
      </div>
      {codeReview && <CodeReviewCheck codeReview={codeReview} />}
      {repositories.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
          No organization rows imported for {monthLabel(month)}.
        </p>
      ) : (
        <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Repository</TableHead>
                <TableHead className="hidden md:table-cell">SKU</TableHead>
                <TableHead className="text-right">Quantity</TableHead>
                <TableHead className="text-right">Gross</TableHead>
                <TableHead className="hidden text-right sm:table-cell">
                  Discount
                </TableHead>
                <TableHead className="hidden text-right sm:table-cell">
                  Net
                </TableHead>
                <TableHead className="hidden text-right lg:table-cell">
                  Days
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {repositories.map((row) => (
                <TableRow
                  key={`${row.kind}:${row.product}:${row.sku}:${row.unit}:${row.repository}`}
                  className="align-top"
                >
                  <TableCell className="py-2.5">
                    <div className="flex flex-col gap-0.5">
                      <span
                        className={cn(
                          "font-medium",
                          row.repository === "" && "text-muted-foreground",
                        )}
                      >
                        {row.repository || "No repository"}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {row.kind === "accountCreditsAggregate" ||
                        row.kind === "unrecognized" ? (
                          <StatusBadge
                            tone={
                              row.kind === "unrecognized"
                                ? "warning"
                                : "neutral"
                            }
                            label={chargeKindLabel(row.kind)}
                          />
                        ) : (
                          chargeKindLabel(row.kind)
                        )}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell className="hidden py-2.5 text-xs text-muted-foreground md:table-cell">
                    {row.product} / {row.sku}
                  </TableCell>
                  <TableCell className={cn(AMOUNT, "py-2.5")}>
                    {row.credits !== null ? (
                      <span title={`${row.quantity} ${row.unit}`}>
                        {formatCreditNumber(row.credits)} credits
                      </span>
                    ) : (
                      <span title={`${row.quantity} ${row.unit}`}>
                        {formatQuantity(row.quantity)}{" "}
                        <span className="text-muted-foreground">
                          {row.unit}
                        </span>
                      </span>
                    )}
                  </TableCell>
                  <TableCell className={cn(AMOUNT, "py-2.5")}>
                    {formatUsdAmount(row.grossUsd)}
                  </TableCell>
                  <TableCell
                    className={cn(AMOUNT, "hidden py-2.5 sm:table-cell")}
                  >
                    {formatUsdAmount(row.discountUsd)}
                  </TableCell>
                  <TableCell
                    className={cn(AMOUNT, "hidden py-2.5 sm:table-cell")}
                  >
                    {formatUsdAmount(row.netUsd)}
                  </TableCell>
                  <TableCell
                    className={cn(AMOUNT, "hidden py-2.5 lg:table-cell")}
                  >
                    {plural(row.days, "day")}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </section>
  );
}

/** Code review credits per account against those per repository: the same
 * credits, which bill once, per repository. */
function CodeReviewCheck({
  codeReview,
}: {
  codeReview: NonNullable<AiMeteredMonth["codeReview"]>;
}) {
  const equal =
    creditHundredths(codeReview.byAccounts) ===
    creditHundredths(codeReview.byRepositories);
  return (
    <p className="flex flex-wrap items-center gap-1.5 text-sm">
      <StatusBadge
        tone={equal ? "good" : "warning"}
        label={equal ? "Code review adds up" : "Code review differs"}
      />
      <span className="text-muted-foreground">
        {formatCredits(codeReview.byAccounts)} under accounts&apos; “Code Review
        model”, {formatCredits(codeReview.byRepositories)} under repositories.
      </span>
    </p>
  );
}
