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
  SPINIT_KIND_LABELS,
  SPINIT_LABEL,
  UNASSIGNED_KIND_LABELS,
  UNASSIGNED_LABEL,
  formatConverted,
  lineConverted,
  lineUserId,
  lineWarnings,
  formatLineCredits,
  meteredAccountDetail,
  meteredName,
  meteredSku,
  provisionalWarning,
  subjectLabel,
  trackedSkus,
} from "../../-lib/billing";
import { formatCount, formatShare, plural } from "../../-lib/format";
import {
  ledgerEntries,
  ledgerRowId,
  shareOf,
  type BillableStep,
  type Ledger,
  type LedgerEntry,
  type LedgerPerson,
} from "../../-lib/ledger";
import { UNDECIDED_FILL } from "./allocation-strip";
import { formatEstimateIn } from "../../-lib/estimates";
import { useEstimateMoney } from "../../-lib/use-estimate-money";

/** Developers billing less than this, in hundredths, fold away in an open
 * row. */
const SMALL_PERSON = 100;
/** Fewer small developers than this stay in view. */
const FOLD_AT = 3;

/**
 * The month's bill by project, dearest first: what to invoice each customer,
 * whose usage it is, with what, and what it would have cost at API prices.
 * Usage without a project, fees nobody used and a metered payer's credits
 * that stay with Spinit follow the projects. A row opens to its developers
 * (or accounts), each with their lines beneath when they have more than one;
 * the row pointed at lights its segment of the strip.
 */
export function ProjectLedger({
  overview,
  ledger,
  open,
  onToggle,
  focusRow,
  steps,
  segmentOf,
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
  /** The strip segments each entry is drawn in, by entry key. */
  segmentOf: ReadonlyMap<string, readonly string[]>;
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
  const tracked = trackedSkus(overview.lines);

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
            tracked={tracked}
            step={steps.get(entry.key) ?? null}
            undeclared={undeclared}
            isOpen={open.has(entry.key)}
            onToggle={() => onToggle(entry.key)}
            lit={
              highlighted === entry.key ||
              // A merged segment lights every row it stands for.
              (highlighted !== null &&
                (segmentOf.get(entry.key) ?? []).includes(highlighted))
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
  tracked,
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
  tracked: ReadonlySet<string>;
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
  // Only an amount known in full folds away as small.
  const small = entry.people.filter(
    ({ totals }) =>
      totals.converted !== null &&
      totals.unknownRecords === 0 &&
      totals.converted < SMALL_PERSON,
  );
  const folded = small.length >= FOLD_AT && !showSmall;
  const shown = folded
    ? entry.people.filter((person) => !small.includes(person))
    : entry.people;
  const detailsId = `${ledgerRowId(entry.key)}-lines`;
  const accounts = entry.people.every((person) => person.userId === null);
  const people = entry.people.map((person) =>
    subjectLabel(person.subject, names),
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
            {entry.unassigned === "unmapped" && (
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
                : `${formatCount(people.length)} ${accounts ? "accounts" : "people"}`}
            </span>
          )}
        </TableCell>
        <TableCell className="hidden lg:table-cell">
          <ProviderIcons providers={entry.providers} className="size-4" />
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
          {entry.kind === "overhead" || entry.totals.usage.records === 0
            ? "—"
            : formatEstimateIn(entry.totals.usage, money)}
        </TableCell>
      </TableRow>

      {isOpen &&
        shown.map((person, index) => (
          <PersonRows
            key={person.key}
            id={index === 0 ? detailsId : undefined}
            person={person}
            name={subjectLabel(person.subject, names)}
            overview={overview}
            plans={plans}
            undeclared={undeclared}
            tracked={tracked}
          />
        ))}
      {isOpen && folded && (
        <TableRow
          id={shown.length === 0 ? detailsId : undefined}
          className="hover:bg-transparent"
        >
          <TableCell colSpan={6} className="py-1.5 pl-12">
            <button
              type="button"
              onClick={() => setShowSmall(true)}
              className="rounded-sm text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
            >
              Show{" "}
              {plural(
                small.length,
                accounts ? "more account" : "more developer",
              )}{" "}
              under 1 {currency} each
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
 * segment; Unassigned hatched, overhead muted and Spinit's credits solid
 * muted, as in the strip.
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
        entry.kind === "spinit" &&
          (entry.spinit === "untracked"
            ? "bg-muted-foreground/60"
            : "bg-muted-foreground/45"),
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
          <span className="text-muted-foreground">
            {" "}
            {entry.unassigned === null
              ? "no project yet"
              : UNASSIGNED_KIND_LABELS[entry.unassigned].detail}
          </span>
        </span>
      );
    case "overhead":
      return (
        <span className="min-w-0 sm:truncate">
          <span className="font-semibold">{OVERHEAD_LABEL}</span>
          <span className="text-muted-foreground"> fees without usage</span>
        </span>
      );
    case "spinit":
      return (
        <span className="min-w-0 sm:truncate">
          <span className="font-semibold">{SPINIT_LABEL}</span>
          <span className="text-muted-foreground">
            {" "}
            {SPINIT_KIND_LABELS[entry.spinit ?? "untracked"].detail}
          </span>
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
              kind === "spinit" && "bg-muted-foreground/45",
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

/** How a developer's lines show: what each tool and plan is needed for. */
type LineSource = Readonly<{
  overview: AiBillingOverview;
  plans: ReadonlyMap<number, AiSubscriptionMonth>;
  undeclared: ReadonlyMap<number, readonly AiProvider[]>;
  /** The metered SKUs Toki tracks, which lines need not name. */
  tracked: ReadonlySet<string>;
}>;

/**
 * A developer in an open row. With one line they take a single row; with
 * more, a row of their subtotal and a row beneath it for each line.
 */
function PersonRows({
  id,
  person,
  name,
  ...source
}: LineSource & {
  id?: string;
  person: LedgerPerson;
  name: string;
}) {
  const money = useEstimateMoney();
  const { overview } = source;
  const [only, ...others] = person.lines;
  if (only !== undefined && others.length === 0) {
    return <LineRow id={id} line={only} name={name} {...source} />;
  }
  const overhead = person.lines.every((line) => line.unallocatedOverhead);
  const tools = <ProviderIcons providers={person.providers} />;
  return (
    <>
      <TableRow id={id} className="border-0 text-[13px] hover:bg-muted/30">
        <TableCell className="py-1.5 pl-12">
          <SubjectLink
            line={person.lines[0]}
            name={name}
            month={overview.month}
          />
          <span className="sr-only">, subtotal</span>
          <span className="mt-1 block md:hidden">{tools}</span>
        </TableCell>
        <DetailCells>{tools}</DetailCells>
        <TableCell
          className={cn(
            "py-1.5 text-right whitespace-nowrap tabular-nums",
            person.totals.converted === null &&
              "text-[#c98500] dark:text-[#fab219]",
          )}
        >
          {formatConverted(
            person.totals,
            overview.billingCurrency,
            overview.pendingRates,
          )}
        </TableCell>
        <TableCell className="hidden py-1.5 text-right whitespace-nowrap text-muted-foreground tabular-nums md:table-cell">
          {overhead || person.totals.usage.records === 0
            ? "—"
            : formatEstimateIn(person.totals.usage, money)}
        </TableCell>
      </TableRow>
      {person.lines.map((line, index) => (
        <ToolLineRow
          key={`${line.provider}-${line.billingMode}-${line.subscriptionId ?? line.metered?.sku ?? "api"}-${index}`}
          line={line}
          name={name}
          last={index === person.lines.length - 1}
          {...source}
        />
      ))}
    </>
  );
}

/** The marks of the tools used, each named for screen readers. */
function ProviderIcons({
  providers,
  className,
}: {
  providers: readonly AiProvider[];
  className?: string;
}) {
  return (
    <span className="flex items-center gap-1.5">
      {providers.map((provider) => (
        <span key={provider} title={AI_PROVIDER_LABELS[provider]}>
          <AiProviderIcon provider={provider} className={className} />
          <span className="sr-only">{AI_PROVIDER_LABELS[provider]}</span>
        </span>
      ))}
    </span>
  );
}

/** A developer's only line in an open row: whose usage, with what, paid how. */
function LineRow({
  id,
  line,
  name,
  ...source
}: LineSource & {
  id?: string;
  line: AiBillingLine;
  name: string;
}) {
  const { overview } = source;
  return (
    <TableRow id={id} className="border-0 text-[13px] hover:bg-muted/30">
      <TableCell className="py-1.5 pl-12">
        <SubjectLink line={line} name={name} month={overview.month} />
        <ToolDetail
          line={line}
          {...source}
          className="mt-0.5 flex flex-wrap text-xs md:hidden"
        />
      </TableCell>
      <DetailCells>
        <ToolDetail line={line} {...source} className="inline-flex" />
      </DetailCells>
      <LineAmounts line={line} overview={overview} />
    </TableRow>
  );
}

/** One line beneath a developer's subtotal, hung off a guide from their name. */
function ToolLineRow({
  line,
  name,
  last,
  ...source
}: LineSource & {
  line: AiBillingLine;
  name: string;
  last: boolean;
}) {
  return (
    <TableRow className="border-0 text-xs text-muted-foreground hover:bg-muted/30">
      <TableCell className="relative py-1 pl-[4.25rem]">
        <span
          aria-hidden
          className={cn(
            "absolute top-0 left-[3.25rem] border-l border-border",
            last ? "bottom-1/2" : "bottom-0",
          )}
        />
        <span
          aria-hidden
          className="absolute top-1/2 left-[3.25rem] w-3 border-t border-border"
        />
        <span className="sr-only">Breakdown for {name}: </span>
        <ToolDetail
          line={line}
          {...source}
          className="flex flex-wrap md:hidden"
        />
      </TableCell>
      <DetailCells className="py-1">
        <ToolDetail line={line} {...source} className="inline-flex" />
      </DetailCells>
      <LineAmounts line={line} overview={source.overview} className="py-1" />
    </TableRow>
  );
}

/**
 * Whose lines a row holds: a developer, linking to their month, or an account
 * no developer owns, by login and why, linking to where accounts are linked.
 */
function SubjectLink({
  line,
  name,
  month,
}: {
  /** One of the subject's lines. */
  line: AiBillingLine;
  name: string;
  month: string;
}) {
  const userId = lineUserId(line);
  if (userId !== null) {
    return (
      <Link
        to="/admin/ai-billing/$userId"
        params={{ userId: String(userId) }}
        search={{ month }}
        className="underline-offset-4 hover:underline"
      >
        {name}
      </Link>
    );
  }
  const detail = meteredAccountDetail(line);
  return (
    <span className="inline-flex items-baseline gap-1.5">
      {line.subject.kind === "providerAccount" ? (
        <Link
          to="/admin/github-logins"
          className="underline-offset-4 hover:underline"
        >
          {name}
        </Link>
      ) : (
        <span>{name}</span>
      )}
      {detail !== null && line.subject.kind === "providerAccount" && (
        <span className="text-xs text-muted-foreground">{detail}</span>
      )}
    </span>
  );
}

/**
 * A row's tool and plan, and its empty share. They run under People and
 * Tools on wide screens; where Tools is hidden, under People and Share.
 */
function DetailCells({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <>
      <TableCell
        className={cn("hidden py-1.5 md:table-cell lg:hidden", className)}
        colSpan={2}
      >
        {children}
      </TableCell>
      <TableCell
        className={cn("hidden py-1.5 lg:table-cell", className)}
        colSpan={2}
      >
        {children}
      </TableCell>
      <TableCell
        className={cn(
          "hidden py-1.5 sm:table-cell md:hidden lg:table-cell",
          className,
        )}
      />
    </>
  );
}

/** A line's tool, how it is paid, and what to look into about it. */
function ToolDetail({
  line,
  overview,
  plans,
  undeclared,
  tracked,
  className,
}: LineSource & { line: AiBillingLine; className?: string }) {
  const subscription =
    line.subscriptionId === null ? undefined : plans.get(line.subscriptionId);
  const provisional = provisionalWarning(line);
  // The provisional reasons are the pill's, not the warning's.
  const warnings = lineWarnings(
    line,
    subscription,
    overview.billingCurrency,
    overview.pendingRates,
  ).filter((warning) => warning !== provisional);
  const userId = lineUserId(line);
  // The tool reported a paid plan nobody declared: these API prices are
  // likely more than the plan's share would be.
  const undeclaredPlan =
    line.billingMode === "api" &&
    userId !== null &&
    (undeclared.get(userId) ?? []).includes(line.provider);
  const sku = meteredSku(line, tracked);
  return (
    <span
      className={cn(
        "flex-wrap items-center gap-x-1.5 whitespace-nowrap text-muted-foreground",
        className,
      )}
    >
      <AiProviderIcon provider={line.provider} />
      {line.metered !== null ? (
        <>
          <span className="text-foreground">
            {meteredName(line.provider)} ·{" "}
            {formatLineCredits(line.metered.credits)}
          </span>
          {sku !== null && <span>{sku}</span>}
        </>
      ) : (
        <>
          <span className="text-foreground">
            {AI_PROVIDER_LABELS[line.provider]}
          </span>
          {line.billingMode === "subscription"
            ? `on ${subscription?.plan ?? "a subscription"}`
            : "at API prices"}
        </>
      )}
      {warnings.length > 0 && <Warnings warnings={warnings} />}
      {provisional !== null && (
        <AttentionPill title={provisional}>
          provisional
          <span className="sr-only">: {provisional}</span>
        </AttentionPill>
      )}
      {undeclaredPlan && (
        <AttentionPill title="The tool reported a paid plan nobody declared, so this bills at API prices instead of a share of the plan's fee.">
          plan not declared
        </AttentionPill>
      )}
    </span>
  );
}

/** What a line bills, and what it would have cost at API prices. */
function LineAmounts({
  line,
  overview,
  className,
}: {
  line: AiBillingLine;
  overview: AiBillingOverview;
  className?: string;
}) {
  const money = useEstimateMoney();
  return (
    <>
      <TableCell
        className={cn(
          "py-1.5 text-right whitespace-nowrap tabular-nums",
          line.missingRate && "text-[#c98500] dark:text-[#fab219]",
          className,
        )}
      >
        {lineConverted(line, overview.billingCurrency, overview.pendingRates)}
      </TableCell>
      <TableCell
        className={cn(
          "hidden py-1.5 text-right whitespace-nowrap text-muted-foreground tabular-nums md:table-cell",
          className,
        )}
      >
        {line.unallocatedOverhead ||
        (line.metered !== null && line.usage.records === 0)
          ? "—"
          : formatEstimateIn(line.usage, money)}
      </TableCell>
    </>
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
