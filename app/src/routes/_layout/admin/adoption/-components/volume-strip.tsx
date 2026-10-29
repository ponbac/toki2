import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import {
  companyTotals,
  type DayRange,
  type PersonStats,
} from "../../-lib/adoption";
import { formatCost } from "./format";
import { formatShare } from "../../-lib/format";
import { useEstimateMoney } from "../../-lib/use-estimate-money";
import { SectionHead } from "./segmented";

/** The smallest share a person gets a block of their own at. */
const OWN_BLOCK_SHARE = 0.04;
/** The smallest share whose block has room for its label. */
const LABEL_SHARE = 0.06;

/**
 * Who the company's cost in the window comes from: a block per person above
 * a few percent, the rest together, and credits no person carries apart.
 */
export function VolumeStrip({
  adoption,
  people,
  range,
}: {
  adoption: AiAdoption;
  people: readonly PersonStats[];
  range: DayRange;
}) {
  const money = useEstimateMoney();
  const totals = companyTotals(adoption, range);
  if (totals.costUsd <= 0) return null;
  const users = people.filter((stats) => stats.costUsd > 0);
  const own = users.filter(
    (stats) => stats.costUsd / totals.costUsd >= OWN_BLOCK_SHARE,
  );
  const rest = users.filter((stats) => !own.includes(stats));
  const restUsd = rest.reduce((sum, stats) => sum + stats.costUsd, 0);
  const blocks = [
    ...own.map((stats) => ({
      key: String(stats.person.userId),
      label: stats.person.fullName.split(" ")[0] ?? stats.person.fullName,
      title: stats.person.fullName,
      usd: stats.costUsd,
      tone: "own" as const,
    })),
    ...(rest.length > 0
      ? [
          {
            key: "rest",
            label: `${rest.length} others`,
            title: `${rest.length} others`,
            usd: restUsd,
            tone: "rest" as const,
          },
        ]
      : []),
    ...(totals.unassignedUsd > 0
      ? [
          {
            key: "company",
            label: "No one",
            title:
              "GitHub Copilot credits no person carries: accounts not linked to anyone, and code review billed per repository",
            usd: totals.unassignedUsd,
            tone: "company" as const,
          },
        ]
      : []),
  ];

  return (
    <section aria-labelledby="adoption-strip" className="flex flex-col gap-3">
      <SectionHead
        id="adoption-strip"
        title="Where the cost is"
        description={`Each block is a share of the ${formatCost(totals.costUsd, money)} of AI in the window. A large share means heavy use on the days AI is used, not that the rest use it less often.`}
      />
      {/* The ramp flips in dark mode, light to dark, so label ink flips too. */}
      <ul className="flex h-12 gap-0.5 overflow-hidden rounded-lg">
        {blocks.map((block) => (
          <li
            key={block.key}
            title={`${block.title}: ${formatCost(block.usd, money)}`}
            className={
              block.tone === "own"
                ? "min-w-0.5 overflow-hidden bg-[var(--viz-seq-5)] px-2 py-1.5 text-xs font-semibold whitespace-nowrap text-white dark:text-[#0d366b]"
                : "min-w-0.5 overflow-hidden bg-[var(--viz-seq-2)] px-2 py-1.5 text-xs font-semibold whitespace-nowrap text-[#0d366b] dark:text-white"
            }
            style={{ flexGrow: block.usd, flexBasis: 0 }}
          >
            {block.usd / totals.costUsd >= LABEL_SHARE ? (
              <>
                <span className="block truncate">{block.label}</span>
                <span className="block truncate font-medium opacity-85">
                  {formatShare(block.usd / totals.costUsd)}
                </span>
              </>
            ) : (
              <span className="sr-only">
                {block.label}, {formatShare(block.usd / totals.costUsd)}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
