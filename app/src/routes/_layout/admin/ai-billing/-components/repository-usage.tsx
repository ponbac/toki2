import { Link } from "@tanstack/react-router";
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
import { GITHUB_REPOSITORIES_ID } from "../../-components/repository-mappings";
import { StatusBadge } from "../../-components/status-badge";
import { monthLabel, plural, weekdayDate } from "../../-lib/format";
import {
  RESIDUAL_STATUS_TONES,
  chargeKindLabel,
  codeReviewCheckTone,
  codeReviewDifferences,
  codeReviewDifferingDays,
  formatCreditNumber,
  formatCredits,
  formatQuantity,
  formatSignedCreditNumber,
  formatUsdAmount,
  hasUnrecognizedRepositoryRows,
  residualStatusLabel,
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
          credits are billed per repository, never per account, to the project
          each repository is mapped to under{" "}
          <Link
            to="/admin/project-mappings"
            hash={GITHUB_REPOSITORIES_ID}
            className="underline underline-offset-4 hover:text-foreground"
          >
            Project mappings &gt; GitHub repositories
          </Link>
          ; Actions minutes are never billed on.
        </p>
      </div>
      {codeReview && (
        <CodeReviewCheck
          codeReview={codeReview}
          unrecognizedRepositories={hasUnrecognizedRepositoryRows(
            repositories,
          )}
        />
      )}
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

/** Code review credits per account against those per repository, per UTC
 * day: the same credits, which bill once, per repository. Days whose
 * repositories exceed their accounts are informational; days whose accounts
 * exceed their repositories leave code review on no line, a warning. */
function CodeReviewCheck({
  codeReview,
  unrecognizedRepositories,
}: {
  codeReview: NonNullable<AiMeteredMonth["codeReview"]>;
  unrecognizedRepositories: boolean;
}) {
  const differing = codeReviewDifferingDays(codeReview.days);
  const { accountsExceed, repositoriesExceed } = codeReviewDifferences(
    codeReview.days,
  );
  return (
    <div className="flex flex-col gap-1.5">
      <p className="flex flex-wrap items-center gap-1.5 text-sm">
        <StatusBadge
          tone={codeReviewCheckTone(codeReview.days)}
          label={
            differing.length === 0
              ? "Code review adds up"
              : `Code review differs on ${plural(differing.length, "day")}`
          }
        />
        <span className="text-muted-foreground">
          {formatCredits(codeReview.byAccounts)} under accounts&apos; “Code
          Review model”, {formatCredits(codeReview.byRepositories)} under
          repositories, over {plural(codeReview.days.length, "day")}.
        </span>
      </p>
      {differing.length > 0 && (
        <details className="rounded-xl border border-border/70 bg-card/60">
          <summary className="cursor-pointer rounded-xl px-4 py-2 text-sm text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden">
            Days that differ
          </summary>
          <div className="flex flex-col gap-1 px-4 pb-2 text-sm text-muted-foreground">
            {accountsExceed && (
              <p>
                {unrecognizedRepositories
                  ? "A negative difference, where accounts exceed repositories, may be code review in the unrecognized rows naming a repository below, which bill on no line and keep repository lines provisional; else it is code review GitHub billed without a repository, inside the IDE aggregate, which bills on no line."
                  : "A negative difference, where accounts exceed repositories, is code review GitHub billed without a repository, inside the IDE aggregate: it bills on no line."}
              </p>
            )}
            {repositoriesExceed && (
              <p>
                A positive difference, where repositories exceed accounts, is
                code review by accounts Toki does not import (unknown
                accounts), or on an import gap by an account whose rows are
                missing. Their “Code Review model” credits are not imported,
                but the repository rows are: it bills once, through its
                repository line, and the bill needs nothing.
              </p>
            )}
          </div>
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Day (UTC)</TableHead>
                <TableHead className="text-right">Accounts</TableHead>
                <TableHead className="text-right">Repositories</TableHead>
                <TableHead className="text-right">Difference</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {differing.map((day) => (
                <TableRow key={day.day}>
                  <TableCell className="py-2 whitespace-nowrap">
                    {weekdayDate(day.day)}
                  </TableCell>
                  <TableCell className={cn(AMOUNT, "py-2")}>
                    {formatCreditNumber(day.byAccounts)}
                  </TableCell>
                  <TableCell className={cn(AMOUNT, "py-2")}>
                    {formatCreditNumber(day.byRepositories)}
                  </TableCell>
                  <TableCell className={cn(AMOUNT, "py-2 font-medium")}>
                    {formatSignedCreditNumber(day.difference)}
                  </TableCell>
                  <TableCell className="py-2">
                    <StatusBadge
                      tone={RESIDUAL_STATUS_TONES[day.status]}
                      label={residualStatusLabel(day.status)}
                    />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </details>
      )}
    </div>
  );
}
