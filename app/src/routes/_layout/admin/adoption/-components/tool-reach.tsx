import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type { AiAdoption } from "@/lib/api/queries/ai-admin";
import { toolReach, type DayRange } from "../../-lib/adoption";
import { PROVIDER_COLOR_VARS } from "../../-lib/providers";
import { SectionHead } from "./segmented";

/** How many people use each tool: in the window, and per week. */
export function ToolReach({
  adoption,
  range,
  weeks,
}: {
  adoption: AiAdoption;
  range: DayRange;
  weeks: number;
}) {
  const reach = toolReach(adoption, range);
  // A bar's height is the share of everyone listed who used the tool.
  const max = Math.max(1, adoption.people.length);
  return (
    <section aria-labelledby="adoption-tools" className="flex flex-col gap-3">
      <SectionHead
        id="adoption-tools"
        title="Tools people reach for"
        description="People using each tool per week, over the last 13 weeks. One person can use several."
      />
      <ul className="grid grid-cols-2 gap-4 rounded-xl border border-border/60 bg-card p-4 sm:grid-cols-3">
        {reach.map(({ tool, people, weekly }) => (
          <li key={tool} className="flex flex-col gap-1">
            <span className="flex items-center gap-1.5 text-[13px] font-bold">
              <AiProviderIcon provider={tool} />
              {AI_PROVIDER_LABELS[tool]}
            </span>
            <span className="text-xs text-muted-foreground">
              <b className="mr-1 text-lg font-bold text-foreground tabular-nums">
                {people}
              </b>
              people in the last {weeks} weeks
            </span>
            <span
              className="flex h-12 items-end gap-0.5"
              role="img"
              aria-label={`${AI_PROVIDER_LABELS[tool]} users per week, oldest first: ${weekly.join(", ")}`}
            >
              {weekly.map((count, index) => (
                <span
                  key={index}
                  className="block flex-1 rounded-t-sm"
                  style={{
                    height: count > 0 ? `${(count / max) * 100}%` : "1px",
                    background:
                      count > 0 ? PROVIDER_COLOR_VARS[tool] : "var(--viz-grid)",
                  }}
                />
              ))}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
