import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import {
  amountOf,
  companyTotals,
  type DayRange,
  type PersonStats,
  type VolumeMeasure,
} from "../../-lib/adoption";
import { formatAmount } from "./format";
import { formatShare } from "../../-lib/format";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { SectionHead } from "./segmented";

/** The smallest share a person gets a block of their own at. */
const OWN_BLOCK_SHARE = 0.04;
/** The smallest share at which the company-only block shows its label; a
 * person's block always does, truncated if need be. */
const LABEL_SHARE = 0.06;

/**
 * Who the company's cost or tokens in the window come from: a block per
 * person above a few percent, the rest together, and for cost the credits
 * no person carries apart (they have no tokens).
 */
export function VolumeStrip({
  adoption,
  people,
  range,
  measure,
}: {
  adoption: AiAdoption;
  people: readonly PersonStats[];
  range: DayRange;
  measure: VolumeMeasure;
}) {
  const money = useEstimateMoney();
  const totals = companyTotals(adoption, range);
  const total = amountOf(totals, measure);
  if (total <= 0) return null;
  const users = people.filter((stats) => amountOf(stats, measure) > 0);
  const own = users.filter(
    (stats) => amountOf(stats, measure) / total >= OWN_BLOCK_SHARE,
  );
  const rest = users.filter((stats) => !own.includes(stats));
  const restAmount = rest.reduce(
    (sum, stats) => sum + amountOf(stats, measure),
    0,
  );
  const blocks = [
    ...own.map((stats) => ({
      key: String(stats.person.userId),
      label: stats.person.fullName.split(" ")[0] ?? stats.person.fullName,
      title: stats.person.fullName,
      amount: amountOf(stats, measure),
      tone: "own" as const,
    })),
    ...(rest.length > 0
      ? [
          {
            key: "rest",
            label: `${rest.length} others`,
            title: `${rest.length} others`,
            amount: restAmount,
            tone: "rest" as const,
          },
        ]
      : []),
    ...(measure === "cost" && totals.unassignedUsd > 0
      ? [
          {
            key: "company",
            label: "No one",
            title:
              "GitHub Copilot credits no person carries: accounts not linked to anyone, and code review billed per repository",
            amount: totals.unassignedUsd,
            tone: "company" as const,
          },
        ]
      : []),
  ];
  return (
    <section aria-labelledby="adoption-strip" className="flex flex-col gap-3">
      <SectionHead
        id="adoption-strip"
        title={
          measure === "cost" ? "Where the cost is" : "Where the tokens are"
        }
        description={`Each block is a share of the ${formatAmount(total, measure, money)} in the window. A large share means heavy use on the days AI is used, not that the rest use it less often.`}
      />
      {/* The ramp flips in dark mode, light to dark, so label ink flips too. */}
      <ul className="flex h-12 gap-0.5 overflow-hidden rounded-lg">
        {blocks.map((block) => (
          <li
            key={block.key}
            title={`${block.title}: ${formatAmount(block.amount, measure, money)}`}
            className={
              block.tone === "own"
                ? "min-w-0.5 overflow-hidden bg-[var(--viz-seq-5)] px-2 py-1.5 text-xs font-semibold whitespace-nowrap text-white dark:text-[#0d366b]"
                : "min-w-0.5 overflow-hidden bg-[var(--viz-seq-2)] px-2 py-1.5 text-xs font-semibold whitespace-nowrap text-[#0d366b] dark:text-white"
            }
            style={{ flexGrow: block.amount, flexBasis: 0 }}
          >
            {block.tone !== "company" || block.amount / total >= LABEL_SHARE ? (
              <>
                <span className="block truncate">{block.label}</span>
                <span className="block truncate font-medium opacity-85">
                  {formatShare(block.amount / total)}
                </span>
              </>
            ) : (
              <span className="sr-only">
                {block.label}, {formatShare(block.amount / total)}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
