import { cn } from "@/lib/utils";
import { UNASSIGNED_LABEL, formatHundredths } from "../../-lib/billing";
import { formatShare } from "../../-lib/format";
import type { AllocationSegment } from "../../-lib/ledger";

/** Usage that has no project yet: hatched, since where it goes is undecided. */
export const UNDECIDED_FILL =
  "repeating-linear-gradient(135deg, hsl(var(--muted-foreground) / 0.5) 0 1.5px, transparent 1.5px 6px)";

/**
 * The month's bill as one bar, split by where its money goes: each project a
 * solid segment in the order of the ledger beneath, filled with its step of
 * the billable ramp (deepest for the dearest, the palest for the tail of
 * small ones), the usage without a project hatched, and fees nobody used
 * muted. A segment and the ledger rows it stands for light up together;
 * choosing one opens it in the ledger.
 */
export function AllocationStrip({
  segments,
  currency,
  highlighted,
  onHighlight,
  onSelect,
}: {
  segments: readonly AllocationSegment[];
  currency: string;
  /** The ledger entry or segment pointed at, if any. */
  highlighted: string | null;
  onHighlight: (key: string | null) => void;
  onSelect: (segment: AllocationSegment) => void;
}) {
  const total = segments.reduce((sum, segment) => sum + segment.amount, 0);
  if (total === 0) {
    return null;
  }
  const isLit = (segment: AllocationSegment) =>
    highlighted === segment.key ||
    segment.entries.some((entry) => entry.key === highlighted);
  const anyLit = segments.some(isLit);

  return (
    <ul
      aria-label="Where the bill goes"
      className="flex h-14 w-full gap-[3px]"
      onMouseLeave={() => onHighlight(null)}
    >
      {segments.map((segment, index) => {
        const share = segment.amount / total;
        const name = segmentName(segment);
        const lines = describe(segment, total, currency);
        const solid = segment.step !== null;
        return (
          <li
            key={segment.key}
            className="@container flex min-w-1.5"
            style={{ flex: `${segment.amount} 1 0` }}
          >
            <button
              type="button"
              aria-label={lines.join("; ")}
              title={lines.join("\n")}
              onMouseEnter={() => onHighlight(segment.key)}
              onFocus={() => onHighlight(segment.key)}
              onBlur={() => onHighlight(null)}
              onClick={() => onSelect(segment)}
              className={cn(
                "relative flex w-full min-w-0 flex-col justify-center overflow-hidden rounded-[3px] px-2 text-left transition-opacity duration-150 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-hidden motion-reduce:transition-none @max-[4.5rem]:px-1.5",
                index === 0 && "rounded-l-lg",
                index === segments.length - 1 && "rounded-r-lg",
                segment.kind === "unassigned" &&
                  "bg-muted text-foreground shadow-[inset_0_0_0_1px_hsl(var(--muted-foreground)/0.35)]",
                segment.kind === "overhead" &&
                  "bg-muted-foreground/15 text-foreground",
                anyLit && !isLit(segment) && "opacity-35",
              )}
              style={
                segment.step !== null
                  ? {
                      backgroundColor: `var(--billable-${segment.step})`,
                      color: `var(--billable-ink-${segment.step})`,
                    }
                  : segment.kind === "unassigned"
                    ? { backgroundImage: UNDECIDED_FILL }
                    : undefined
              }
            >
              {/* A narrower segment shortens its name, then drops it for its
                  share alone, then shows nothing; its title, label and the
                  ledger still name it. */}
              <span
                aria-hidden
                className="flex min-w-0 flex-col @max-[1.75rem]:hidden"
              >
                <span
                  className={cn(
                    "truncate text-xs leading-tight font-semibold @max-[3rem]:hidden",
                    !solid &&
                      "w-fit max-w-full rounded-sm bg-background/85 px-1",
                  )}
                >
                  {segment.short === name ? (
                    name
                  ) : (
                    <>
                      <span className="@max-[10rem]:hidden">{name}</span>
                      <span className="flex min-w-0 @min-[10rem]:hidden">
                        <span className="truncate">{segment.short}</span>
                        {segment.more > 0 && (
                          <span className="shrink-0">
                            &nbsp;+{segment.more}
                          </span>
                        )}
                      </span>
                    </>
                  )}
                </span>
                <span
                  className={cn(
                    "truncate text-[11px] leading-tight tabular-nums",
                    !solid &&
                      "w-fit max-w-full rounded-sm bg-background/85 px-1",
                  )}
                >
                  {formatShare(share)}
                </span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/**
 * A segment in words, one line each: what it is, its amount and its share of
 * the bill, then every project of a merged tail, dearest first.
 */
function describe(
  segment: AllocationSegment,
  total: number,
  currency: string,
): string[] {
  const figures = (amount: number) =>
    `${formatHundredths(amount, currency)}, ${formatShare(amount / total)} of the bill`;
  switch (segment.kind) {
    case "others":
      return [
        `${segment.entries.length} projects, ${segment.label}: ${figures(segment.amount)}`,
        ...segment.entries.map(
          (entry) => `${entry.label}: ${figures(entry.totals.converted ?? 0)}`,
        ),
      ];
    case "unassigned":
      return [
        `${UNASSIGNED_LABEL}: ${segmentName(segment)}: ${figures(segment.amount)}`,
      ];
    case "project":
    case "overhead":
      return [
        `${segment.kind === "project" ? segment.label : segmentName(segment)}: ${figures(segment.amount)}`,
      ];
  }
}

/** What a segment is called: a project by its customer, as on an invoice. */
function segmentName(segment: AllocationSegment): string {
  switch (segment.kind) {
    case "project":
      return segment.entries[0]?.name.customer ?? segment.label;
    case "unassigned":
    case "others":
    case "overhead":
      return segment.label;
  }
}
