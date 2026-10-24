import * as React from "react";
import { Link } from "@tanstack/react-router";
import { AlertTriangle, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiBillingLine,
  AiBillingOverview,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import type { AiProvider } from "@/lib/api/queries/ai-usage";
import { cn } from "@/lib/utils";
import { AttentionPill } from "../../-components/attention-pill";
import {
  OVERHEAD_LABEL,
  UNASSIGNED_LABEL,
  formatConverted,
  lineConverted,
  lineWarnings,
} from "../../-lib/billing";
import { formatCount, formatShare, plural } from "../../-lib/format";
import {
  TAIL_STEP,
  ledgerEntries,
  ledgerRowId,
  lineHundredths,
  shareOf,
  type BillableStep,
  type Ledger,
  type LedgerEntry,
} from "../../-lib/ledger";
import { UNDECIDED_FILL } from "./allocation-strip";
import { formatEstimateIn } from "../../-lib/estimates";
import { useEstimateMoney } from "../../-lib/use-estimate-money";

/** Lines billing less than this, in hundredths, fold away in an open row. */
const SMALL_LINE = 100;
/** Fewer small lines than this stay in view. */
const FOLD_AT = 3;

/**
 * The month's bill by project, dearest first: what to invoice each customer,
 * whose usage it is, with what, and what it would have cost at API prices.
 * Usage without a project and fees nobody used follow the projects. A row
 * opens to its lines; the row pointed at lights its segment of the strip.
 */
export function ProjectLedger({
  overview,
  ledger,
  open,
  onToggle,
  focusRow,
  steps,
  undeclared,
  highlighted,
  onHighlight,
}: {
  overview: AiBillingOverview;
  ledger: Ledger;
  open: ReadonlySet<string>;
  onToggle: (key: string) => void;
  /** A row to bring into view; each new object brings it again. */
  focusRow: Readonly<{ key: string }> | null;
  /** Each project's step of the billable ramp, as its strip segment has it. */
  steps: ReadonlyMap<string, BillableStep>;
  /** Tools each developer used on a plan nobody declared this month. */
  undeclared: ReadonlyMap<number, readonly AiProvider[]>;
  highlighted: string | null;
  onHighlight: (key: string | null) => void;
}) {
  const money = useEstimateMoney();
  React.useEffect(() => {
    if (focusRow === null) {
      return;
    }
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
    document.getElementById(ledgerRowId(focusRow.key))?.scrollIntoView({
      behavior: reduce.matches ? "auto" : "smooth",
      block: "center",
    });
  }, [focusRow]);
  const currency = overview.billingCurrency;
  const names = new Map(overview.developers.map((d) => [d.userId, d.fullName]));
  const plans = new Map(
    overview.subscriptions.map((s) => [s.subscriptionId, s]),
  );

  return (
    <div className="overflow-hidden rounded-xl border border-border/70 bg-card/60">
      <Table onMouseLeave={() => onHighlight(null)}>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="w-[34%]">Project</TableHead>
            <TableHead className="hidden md:table-cell">People</TableHead>
            <TableHead className="hidden lg:table-cell">Tools</TableHead>
            <TableHead className="hidden w-[16%] sm:table-cell">
              Share
            </TableHead>
            <TableHead className="text-right whitespace-nowrap">
              Billed ({currency})
            </TableHead>
            <TableHead className="hidden text-right whitespace-nowrap md:table-cell">
              At API prices
            </TableHead>
          </TableRow>
        </TableHeader>
        {ledgerEntries(ledger).map((entry) => (
          <EntryRows
            key={entry.key}
            entry={entry}
            ledger={ledger}
            overview={overview}
            names={names}
            plans={plans}
            step={steps.get(entry.key) ?? null}
            undeclared={undeclared}
            isOpen={open.has(entry.key)}
            onToggle={() => onToggle(entry.key)}
            lit={
              highlighted === entry.key ||
              // The merged tail lights every row it stands for.
              (highlighted === "others" && steps.get(entry.key) === TAIL_STEP)
            }
            onHighlight={onHighlight}
          />
        ))}
        <TableFooter className="bg-transparent">
          <TableRow className="hover:bg-transparent">
            <TableCell className="font-semibold">Total</TableCell>
            <TableCell className="hidden md:table-cell" />
            <TableCell className="hidden lg:table-cell" />
            <TableCell className="hidden sm:table-cell" />
            <TableCell className="text-right font-semibold tabular-nums sm:whitespace-nowrap">
              {formatConverted(ledger.totals, currency, overview.pendingRates)}
            </TableCell>
            <TableCell className="hidden text-right whitespace-nowrap text-muted-foreground tabular-nums md:table-cell">
              {formatEstimateIn(ledger.totals.usage, money)}
            </TableCell>
          </TableRow>
        </TableFooter>
      </Table>
    </div>
  );
}

function EntryRows({
  entry,
  ledger,
  overview,
  names,
  plans,
  step,
  undeclared,
  isOpen,
  onToggle,
  lit,
  onHighlight,
}: {
  entry: LedgerEntry;
  ledger: Ledger;
  overview: AiBillingOverview;
  names: ReadonlyMap<number, string>;
  plans: ReadonlyMap<number, AiSubscriptionMonth>;
  step: BillableStep | null;
  undeclared: ReadonlyMap<number, readonly AiProvider[]>;
  isOpen: boolean;
  onToggle: () => void;
  lit: boolean;
  onHighlight: (key: string | null) => void;
}) {
  const money = useEstimateMoney();
  const [showSmall, setShowSmall] = React.useState(false);
  const currency = overview.billingCurrency;
  const share = shareOf(entry, ledger);
  const small = entry.lines.filter((line) => {
    const amount = lineHundredths(line);
    return amount !== null && amount < SMALL_LINE;
  });
  const folded = small.length >= FOLD_AT && !showSmall;
  const shown = folded
    ? entry.lines.filter((line) => !small.includes(line))
    : entry.lines;
  const detailsId = `${ledgerRowId(entry.key)}-lines`;
  const people = entry.developers.map(
    (userId) => names.get(userId) ?? `User ${userId}`,
  );

  return (
    <TableBody className="border-b border-border/60 last:border-b-0">
      <TableRow
        id={ledgerRowId(entry.key)}
        onMouseEnter={() => onHighlight(entry.key)}
        className={cn(
          "scroll-mt-24 transition-colors motion-reduce:transition-none",
          lit && "bg-primary/[0.07] hover:bg-primary/[0.07]",
          isOpen && "border-b-0",
        )}
      >
        <TableCell className="py-2.5">
          <div className="flex items-center gap-2">
            <button
              type="button"
              aria-expanded={isOpen}
              aria-controls={detailsId}
              onClick={onToggle}
              onFocus={() => onHighlight(entry.key)}
              className="group -ml-1.5 flex min-w-0 items-center gap-1.5 rounded-md px-1.5 py-1 text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
            >
              <ChevronRight
                className={cn(
                  "size-4 shrink-0 text-muted-foreground transition-transform duration-150 motion-reduce:transition-none",
                  isOpen && "rotate-90",
                )}
                aria-hidden
              />
              <EntryMarker entry={entry} step={step} />
              <EntryName entry={entry} />
            </button>
            {entry.kind === "unassigned" && (
              <Button
                asChild
                variant="outline"
                size="sm"
                className="ml-auto hidden h-7 shrink-0 px-2 text-xs sm:inline-flex"
              >
                <Link to="/admin/project-mappings">Map repositories</Link>
              </Button>
            )}
          </div>
        </TableCell>
        <TableCell className="hidden max-w-40 truncate whitespace-nowrap text-muted-foreground md:table-cell">
          {entry.kind === "overhead" ? (
            "—"
          ) : (
            <span title={people.join(", ")}>
              {people.length === 1
                ? people[0]
                : `${formatCount(people.length)} people`}
            </span>
          )}
        </TableCell>
        <TableCell className="hidden lg:table-cell">
          <span className="flex items-center gap-1.5">
            {entry.providers.map((provider) => (
              <span key={provider} title={AI_PROVIDER_LABELS[provider]}>
                <AiProviderIcon provider={provider} className="size-4" />
                <span className="sr-only">{AI_PROVIDER_LABELS[provider]}</span>
              </span>
            ))}
          </span>
        </TableCell>
        <TableCell className="hidden sm:table-cell">
          {share !== null && (
            <ShareBar share={share} kind={entry.kind} step={step} />
          )}
        </TableCell>
        <TableCell
          className={cn(
            "text-right font-semibold tabular-nums sm:whitespace-nowrap",
            entry.totals.converted === null &&
              "text-[#c98500] dark:text-[#fab219]",
          )}
        >
          {formatConverted(entry.totals, currency, overview.pendingRates)}
        </TableCell>
        <TableCell className="hidden text-right whitespace-nowrap text-muted-foreground tabular-nums md:table-cell">
          {entry.kind === "overhead"
            ? "—"
            : formatEstimateIn(entry.totals.usage, money)}
        </TableCell>
      </TableRow>

      {isOpen &&
        shown.map((line, index) => (
          <LineRow
            key={`${line.userId}-${line.provider}-${line.billingMode}-${line.subscriptionId ?? "api"}-${index}`}
            id={index === 0 ? detailsId : undefined}
            line={line}
            name={names.get(line.userId) ?? `User ${line.userId}`}
            month={overview.month}
            subscription={
              line.subscriptionId === null
                ? undefined
                : plans.get(line.subscriptionId)
            }
            currency={currency}
            pendingRates={overview.pendingRates}
            undeclaredPlan={
              line.billingMode === "api" &&
              (undeclared.get(line.userId) ?? []).includes(line.provider)
            }
          />
        ))}
      {isOpen && folded && (
        <TableRow className="hover:bg-transparent">
          <TableCell colSpan={6} className="py-1.5 pl-12">
            <button
              type="button"
              onClick={() => setShowSmall(true)}
              className="rounded-sm text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
            >
              Show {plural(small.length, "more line")} under 1 {currency} each
            </button>
          </TableCell>
        </TableRow>
      )}
    </TableBody>
  );
}

/**
 * A row's key to the strip: a project's segment colour, ringed where the
 * paler steps would fade into the row, and hollow for a project without a
 * segment; Unassigned hatched and overhead muted, as in the strip.
 */
function EntryMarker({
  entry,
  step,
}: {
  entry: LedgerEntry;
  step: BillableStep | null;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "size-2.5 shrink-0 rounded-[3px]",
        entry.kind === "project" &&
          step === null &&
          "shadow-[inset_0_0_0_1.5px_var(--billable-ring)]",
        step !== null &&
          step >= 4 &&
          "shadow-[inset_0_0_0_1px_var(--billable-ring)]",
        entry.kind === "unassigned" &&
          "bg-muted shadow-[inset_0_0_0_1px_hsl(var(--muted-foreground)/0.5)]",
        entry.kind === "overhead" && "bg-muted-foreground/30",
      )}
      style={
        step !== null
          ? { backgroundColor: `var(--billable-${step})` }
          : entry.kind === "unassigned"
            ? { backgroundImage: UNDECIDED_FILL }
            : undefined
      }
    />
  );
}

/** A project as an invoice names it: the customer, then the project. */
function EntryName({ entry }: { entry: LedgerEntry }) {
  switch (entry.kind) {
    case "project":
      return (
        <span className="min-w-0 sm:truncate">
          <span className="font-semibold">{entry.name.customer}</span>
          {entry.name.project !== null && (
            <span className="text-muted-foreground"> {entry.name.project}</span>
          )}
        </span>
      );
    case "unassigned":
      return (
        <span className="min-w-0 sm:truncate">
          <span className="font-semibold">{UNASSIGNED_LABEL}</span>
          <span className="text-muted-foreground"> no project yet</span>
        </span>
      );
    case "overhead":
      return (
        <span className="min-w-0 sm:truncate">
          <span className="font-semibold">{OVERHEAD_LABEL}</span>
          <span className="text-muted-foreground"> fees without usage</span>
        </span>
      );
  }
}

/** An entry's share of the bill, as a bar and a number. */
function ShareBar({
  share,
  kind,
  step,
}: {
  share: number;
  kind: LedgerEntry["kind"];
  step: BillableStep | null;
}) {
  // A project without a segment prints its share without a bar.
  const bar = kind !== "project" || step !== null;
  return (
    <span className="flex items-center gap-2">
      <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        {bar && (
          <span
            className={cn(
              "block h-full rounded-full",
              kind === "unassigned" && "bg-muted-foreground/60",
              kind === "overhead" && "bg-muted-foreground/30",
            )}
            style={{
              width: `${Math.max(share * 100, 1.5)}%`,
              ...(step !== null && {
                backgroundColor: `var(--billable-${step})`,
              }),
            }}
          />
        )}
      </span>
      <span className="w-14 shrink-0 text-right text-xs text-muted-foreground tabular-nums">
        {formatShare(share)}
      </span>
    </span>
  );
}

/** One line of an open row: whose usage, with what, paid how. */
function LineRow({
  id,
  line,
  name,
  month,
  subscription,
  currency,
  pendingRates,
  undeclaredPlan,
}: {
  id?: string;
  line: AiBillingLine;
  name: string;
  month: string;
  subscription: AiSubscriptionMonth | undefined;
  currency: string;
  pendingRates: readonly string[];
  /** The tool reported a paid plan nobody declared: these API prices are
   * likely more than the plan's share would be. */
  undeclaredPlan: boolean;
}) {
  const money = useEstimateMoney();
  const warnings = lineWarnings(line, subscription, currency, pendingRates);
  return (
    <TableRow id={id} className="border-0 text-[13px] hover:bg-muted/30">
      <TableCell className="py-1.5 pl-12">
        <Link
          to="/admin/ai-billing/$userId"
          params={{ userId: String(line.userId) }}
          search={{ month }}
          className="underline-offset-4 hover:underline"
        >
          {name}
        </Link>
      </TableCell>
      <TableCell className="hidden py-1.5 md:table-cell" colSpan={2}>
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-muted-foreground">
          <AiProviderIcon provider={line.provider} />
          <span className="text-foreground">
            {AI_PROVIDER_LABELS[line.provider]}
          </span>
          {line.billingMode === "subscription"
            ? `on ${subscription?.plan ?? "a subscription"}`
            : "at API prices"}
          {warnings.length > 0 && <Warnings warnings={warnings} />}
          {undeclaredPlan && (
            <AttentionPill title="The tool reported a paid plan nobody declared, so this bills at API prices instead of a share of the plan's fee.">
              plan not declared
            </AttentionPill>
          )}
        </span>
      </TableCell>
      <TableCell className="hidden py-1.5 sm:table-cell" />
      <TableCell
        className={cn(
          "py-1.5 text-right whitespace-nowrap tabular-nums",
          line.missingRate && "text-[#c98500] dark:text-[#fab219]",
        )}
      >
        {lineConverted(line, currency, pendingRates)}
      </TableCell>
      <TableCell className="hidden py-1.5 text-right whitespace-nowrap text-muted-foreground tabular-nums md:table-cell">
        {line.unallocatedOverhead ? "—" : formatEstimateIn(line.usage, money)}
      </TableCell>
    </TableRow>
  );
}

function Warnings({ warnings }: { warnings: readonly string[] }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex rounded-sm text-[#c98500] focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden dark:text-[#fab219]"
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
