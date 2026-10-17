import { Link } from "@tanstack/react-router";
import {
  Activity,
  AlertTriangle,
  Briefcase,
  CircleDashed,
  CreditCard,
  UserRound,
} from "lucide-react";
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
import { AiProviderLabel } from "@/components/ai-provider-icon";
import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { cn } from "@/lib/utils";
import { AttentionPill } from "../../-components/attention-pill";
import {
  billableParts,
  formatConverted,
  groupLines,
  hundredths,
  lineBillable,
  lineConverted,
  lineProject,
  lineWarnings,
  type LineGroup,
} from "../../-lib/billing";
import { formatEstimate, formatTokens } from "../../-lib/format";
import { BILLING_MODE_COLORS } from "../../-lib/providers";

/** What a group is, as the icon that leads its row. */
const GROUP_ICONS = {
  project: { icon: Briefcase, className: "text-muted-foreground" },
  developer: { icon: UserRound, className: "text-muted-foreground" },
  unassigned: {
    icon: CircleDashed,
    className: "text-[#c98500] dark:text-[#fab219]",
  },
  // A subscription fee nobody used: the same mark as a subscription line.
  overhead: { icon: CreditCard, className: "text-muted-foreground" },
} as const satisfies Record<LineGroup["kind"], unknown>;

const UPLOADS_MISSING =
  "Not every upload for this month is in: these figures may still grow.";

/**
 * Month × developer × project: every billing line, grouped by project (for
 * invoicing) or by developer. "Billed" is in the billing currency (SEK), and
 * "Original" the fee share for subscription lines or the API-equivalent
 * estimate in whole cents for API lines, in its own currency. A bar beside
 * each billed amount shows it against the largest group's, in the color of
 * the line's billing mode.
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
  uploadsMissing,
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
  /** The developers whose uploads for the month are incomplete. */
  uploadsMissing?: ReadonlySet<number>;
}) {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const plans = new Map(subscriptions.map((s) => [s.subscriptionId, s]));
  const groups = groupLines(lines, developers, groupBy);
  const detailHeading = groupBy === "project" ? "Developer" : "Project";
  // Every bar is a share of the largest converted amount on screen. A group
  // with a line that has no rate has no total, but its other lines still
  // have amounts, and one of them may be the largest.
  const largest = Math.max(
    0,
    ...groups.map((group) => group.totals.converted ?? 0),
    ...lines.map((line) => lineAmount(line) ?? 0),
  );
  const share = (amount: number | null) =>
    amount === null || largest === 0 ? null : amount / largest;

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-[24%]">
            {groupBy === "project" ? "Project" : "Developer"}
            {linkDevelopers || groupBy === "developer"
              ? ` / ${detailHeading.toLowerCase()}`
              : ""}
          </TableHead>
          <TableHead>Provider</TableHead>
          <TableHead>Billing</TableHead>
          {/* The bars: decoration for the billed amounts beside them. */}
          <TableHead className="hidden w-[10%] min-w-24 px-0 md:table-cell" />
          <TableHead className="whitespace-nowrap text-right">
            Billed ({billingCurrency})
          </TableHead>
          <TableHead className="text-right">Original</TableHead>
          <TableHead className="whitespace-nowrap text-right">
            API-equivalent
          </TableHead>
          <TableHead className="text-right">Tokens</TableHead>
        </TableRow>
      </TableHeader>
      {groups.map((group) => {
        const groupIcon = GROUP_ICONS[group.kind];
        const originals = billableParts(group.totals);
        return (
          <TableBody key={group.key} className="border-b border-border/60">
            <TableRow className="bg-muted/30 hover:bg-muted/30">
              {/* Across the provider and billing columns, which a group has
                  nothing in. */}
              <TableCell colSpan={3} className="font-semibold">
                <span className="flex items-center gap-2">
                  <groupIcon.icon
                    className={cn("size-4 shrink-0", groupIcon.className)}
                    aria-hidden
                  />
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
                  {group.userId !== undefined &&
                    uploadsMissing?.has(group.userId) && <UploadsPill />}
                </span>
              </TableCell>
              <Magnitude
                share={share(group.totals.converted)}
                color="hsl(var(--foreground) / 0.55)"
              />
              <TableCell
                className={cn(
                  "whitespace-nowrap text-right font-semibold tabular-nums",
                  group.totals.converted === null &&
                    "text-[#c98500] dark:text-[#fab219]",
                )}
              >
                {formatConverted(group.totals, billingCurrency, pendingRates)}
              </TableCell>
              <TableCell className="text-right tabular-nums text-muted-foreground">
                {/* One currency to a line where they do not fit side by side. */}
                {originals.length === 0
                  ? "—"
                  : originals.map((part, index) => (
                      <span
                        key={part}
                        className="ml-1 inline-block whitespace-nowrap"
                      >
                        {index > 0 && "+ "}
                        {part}
                      </span>
                    ))}
              </TableCell>
              <TableCell className="whitespace-nowrap text-right tabular-nums">
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
                  <span className="inline-flex items-center gap-2">
                    <DeveloperLink month={month} userId={line.userId}>
                      {names.get(line.userId) ?? `User ${line.userId}`}
                    </DeveloperLink>
                    {uploadsMissing?.has(line.userId) && <UploadsPill />}
                  </span>
                ) : null;
              const ModeIcon =
                line.billingMode === "api" ? Activity : CreditCard;
              return (
                <TableRow key={`${group.key}-${index}`}>
                  <TableCell className="pl-10">{detail}</TableCell>
                  <TableCell>
                    <AiProviderLabel provider={line.provider} />
                  </TableCell>
                  <TableCell>
                    <span className="inline-flex items-center gap-1.5">
                      <ModeIcon
                        className="size-3.5 shrink-0 text-muted-foreground"
                        aria-hidden
                      />
                      {line.billingMode === "api"
                        ? "API usage"
                        : `Subscription${subscription ? `, ${subscription.plan}` : ""}`}
                      {warnings.length > 0 && <Warnings warnings={warnings} />}
                    </span>
                  </TableCell>
                  <Magnitude
                    share={share(lineAmount(line))}
                    color={BILLING_MODE_COLORS[line.billingMode]}
                  />
                  <TableCell
                    className={cn(
                      "whitespace-nowrap text-right tabular-nums",
                      line.missingRate && "text-[#c98500] dark:text-[#fab219]",
                    )}
                  >
                    {lineConverted(line, billingCurrency, pendingRates)}
                  </TableCell>
                  <TableCell className="whitespace-nowrap text-right tabular-nums text-muted-foreground">
                    {lineBillable(line)}
                    {line.exchangeRate !== null && (
                      <span className="block text-xs">
                        at {line.exchangeRate}
                      </span>
                    )}
                  </TableCell>
                  <TableCell className="whitespace-nowrap text-right tabular-nums text-muted-foreground">
                    {line.unallocatedOverhead
                      ? "—"
                      : formatEstimate(line.usage)}
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
        );
      })}
    </Table>
  );
}

/**
 * The cell of a bar as long as a billed amount's share of the largest
 * group's. The bar is decoration: the amount in the next cell carries the
 * value.
 */
function Magnitude({
  share,
  color,
}: {
  /** Null where the amount is unknown or has no rate: no bar. */
  share: number | null;
  color: string;
}) {
  return (
    <TableCell className="hidden px-0 md:table-cell">
      {share !== null && share > 0 && (
        <span
          aria-hidden
          className="block h-2 min-w-0.5 rounded-sm"
          style={{ width: `${share * 100}%`, backgroundColor: color }}
        />
      )}
    </TableCell>
  );
}

/** Marks a developer whose uploads for the month are incomplete. */
function UploadsPill() {
  return (
    <AttentionPill title={UPLOADS_MISSING}>
      <AlertTriangle className="size-3" aria-hidden />
      uploads
      <span className="sr-only">: {UPLOADS_MISSING}</span>
    </AttentionPill>
  );
}

/** A line's amount in the billing currency, or null without a rate for it. */
function lineAmount(line: AiBillingLine): number | null {
  return line.missingRate || line.convertedAmount === null
    ? null
    : hundredths(line.convertedAmount);
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
          className="inline-flex rounded-sm text-[#c98500] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring dark:text-[#fab219]"
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
