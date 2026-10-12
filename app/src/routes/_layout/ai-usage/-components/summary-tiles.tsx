import { Link } from "@tanstack/react-router";
import {
  Activity,
  ChevronRight,
  CircleDashed,
  Coins,
  Layers,
  type LucideIcon,
} from "lucide-react";
import type * as React from "react";
import type { AiUsageReport } from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { Cost, UnpricedChip } from "./cost";
import { RollingAmount } from "./rolling-amount";
import { activeDays, knownShare } from "../-lib/aggregate";
import { TOKEN_COLORS } from "../-lib/colors";
import {
  formatCompact,
  formatCount,
  formatPercent,
  knownUsd,
  totalTokens,
} from "../-lib/format";
import { formatAmount, formatMoney, useMoney } from "../-lib/money";

/** The token categories of the strip, dearest per token first. */
const PAID_TOKENS = [
  { key: "output", label: "Output" },
  { key: "cacheWrite", label: "Cache write" },
  { key: "input", label: "Input" },
] as const;

/**
 * The headline figures of the range as one strip: cost, tokens with their
 * mix, requests and unassigned usage. `sessions` is the number of sessions
 * in the range once known, and `mappable` how many of the user's
 * repositories wait to be mapped.
 */
export function SummaryTiles({
  report,
  sessions,
  mappable,
}: {
  report: AiUsageReport;
  sessions: number | undefined;
  mappable: number;
}) {
  const money = useMoney();
  const { totals } = report;
  const days = activeDays(report);
  const known = knownUsd(totals.cost);
  const unassigned = report.projects.find(
    (group) => group.project === null && !group.personal,
  );
  const unassignedShare = unassigned
    ? knownShare(unassigned.totals.cost, totals.cost)
    : null;
  const paid =
    totals.tokens.output + totals.tokens.input + totals.tokens.cacheWrite;

  return (
    <div className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border/70 bg-border/70 shadow-sm lg:grid-cols-[1.2fr_1.4fr_1fr_1fr]">
      <Tile icon={Coins} label="Estimated cost">
        <Value>
          {totals.cost.kind === "partial" && known === 0 ? (
            "Unknown"
          ) : (
            <RollingAmount
              amount={known * money.perUsd}
              unit={money.currency}
              format={(amount) => formatAmount(money, amount)}
            />
          )}
          {totals.cost.kind === "partial" && (
            <UnpricedChip records={totals.cost.unpricedRecords} />
          )}
        </Value>
        {totals.cost.kind === "estimated" && days > 0 && known > 0 ? (
          <Detail>{formatMoney(money, known / days)} per active day</Detail>
        ) : (
          totals.cost.kind === "partial" && (
            <Detail>The known subtotal; the total is unknown</Detail>
          )
        )}
      </Tile>
      <Tile icon={Layers} label="Tokens">
        <Value>{formatCompact(totalTokens(totals.tokens))}</Value>
        <Detail>
          {formatCompact(totals.tokens.cacheRead)} cache reads,{" "}
          {formatCompact(paid)} other
        </Detail>
        {paid > 0 && (
          <>
            <div
              aria-hidden
              className="mt-2 flex h-1.5 gap-0.5 overflow-hidden rounded-full"
            >
              {PAID_TOKENS.map(({ key }) => (
                <span
                  key={key}
                  style={{
                    flexGrow: totals.tokens[key],
                    backgroundColor: TOKEN_COLORS[key],
                  }}
                />
              ))}
            </div>
            <ul className="mt-1.5 flex flex-wrap gap-x-3 gap-y-0.5 text-[11px] text-muted-foreground">
              {PAID_TOKENS.map(({ key, label }) => (
                <li key={key} className="flex items-center gap-1.5">
                  <span
                    aria-hidden
                    className="size-2 rounded-sm"
                    style={{ backgroundColor: TOKEN_COLORS[key] }}
                  />
                  {label} {formatCompact(totals.tokens[key])}
                </li>
              ))}
            </ul>
          </>
        )}
      </Tile>
      <Tile icon={Activity} label="Requests">
        <Value>{formatCount(totals.records)}</Value>
        <Detail>
          {sessions === undefined
            ? `${formatCount(days)} active ${days === 1 ? "day" : "days"}`
            : `${formatCount(sessions)} ${sessions === 1 ? "session" : "sessions"} on ${formatCount(days)} ${days === 1 ? "day" : "days"}`}
        </Detail>
      </Tile>
      <Tile icon={CircleDashed} label="Unassigned" attention={!!unassigned}>
        <Value>
          {unassigned ? <Cost cost={unassigned.totals.cost} /> : "None"}
        </Value>
        <Detail>
          {unassigned
            ? unassignedShare === null
              ? "Usage without a mapped project"
              : `${formatPercent(unassignedShare)} of the known cost`
            : "All usage counts for a project"}
        </Detail>
        {unassigned && (
          <Link
            to="/ai-usage"
            search={(previous) => ({ ...previous, tab: "repositories" })}
            className="mt-1.5 inline-flex items-center gap-0.5 text-xs font-semibold text-primary underline-offset-2 hover:underline"
          >
            {mappable > 0
              ? `Map ${formatCount(mappable)} ${mappable === 1 ? "repository" : "repositories"}`
              : "See why"}
            <ChevronRight aria-hidden className="size-3.5" />
          </Link>
        )}
      </Tile>
    </div>
  );
}

function Tile({
  icon: Icon,
  label,
  attention = false,
  children,
}: {
  icon: LucideIcon;
  label: string;
  /** The figure needs the user: its label takes the attention colour. */
  attention?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="min-w-0 bg-card p-4">
      <p
        className={cn(
          "flex items-center gap-1.5 text-xs",
          attention ? "text-primary" : "text-muted-foreground",
        )}
      >
        <Icon aria-hidden className="size-3.5 shrink-0" />
        {label}
      </p>
      {children}
    </div>
  );
}

function Value({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-1 flex flex-wrap items-center gap-x-2 text-2xl font-semibold tabular-nums tracking-tight">
      {children}
    </p>
  );
}

function Detail({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-1 text-xs tabular-nums text-muted-foreground">
      {children}
    </p>
  );
}
