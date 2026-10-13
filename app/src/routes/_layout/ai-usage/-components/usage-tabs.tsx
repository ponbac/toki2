import { Link } from "@tanstack/react-router";
import { motion } from "framer-motion";
import {
  AlertTriangle,
  ChartColumn,
  FolderGit2,
  Laptop,
  List,
  type LucideIcon,
} from "lucide-react";
import type * as React from "react";
import { cn } from "@/lib/utils";
import { formatCount } from "../-lib/format";
import { DEFAULT_TAB, type UsageTab } from "../-lib/tabs";

const TABS = [
  { value: "overview", label: "Overview", icon: ChartColumn },
  { value: "sessions", label: "Sessions", icon: List },
  { value: "repositories", label: "Repositories", icon: FolderGit2 },
  { value: "machines", label: "Machines", icon: Laptop },
] as const satisfies readonly {
  value: UsageTab;
  label: string;
  icon: LucideIcon;
}[];

/**
 * The page's tabs, each a link that keeps the dates and filters. A tab says
 * what waits behind it: how many sessions and machines there are, and, in
 * the attention colour, how many repositories are still to map and any
 * warning from `warnings`. What a tab holds is out of sight from the others,
 * so a tab whose data failed to load says so here too.
 */
export function UsageTabs({
  tab,
  sessions,
  mappable,
  machines,
  warnings,
}: {
  tab: UsageTab;
  /** Sessions in the range and filters, once known. */
  sessions: number | undefined;
  /** Repositories the user can map to a project. */
  mappable: number;
  machines: number | undefined;
  /** What is wrong behind a tab, in a few words for a screen reader. */
  warnings: Readonly<Partial<Record<UsageTab, string>>>;
}) {
  const counts: Readonly<Record<UsageTab, React.ReactNode>> = {
    overview: null,
    sessions: sessions !== undefined && <Count>{formatCount(sessions)}</Count>,
    repositories: mappable > 0 && (
      <Count attention>{formatCount(mappable)} to map</Count>
    ),
    machines: machines !== undefined && <Count>{formatCount(machines)}</Count>,
  };

  return (
    <nav
      aria-label="AI usage sections"
      className="flex gap-1 overflow-x-auto border-b border-border/60"
    >
      {TABS.map(({ value, label, icon: Icon }) => {
        const active = value === tab;
        return (
          <Link
            key={value}
            from="/ai-usage"
            to="/ai-usage"
            search={(previous) => ({
              ...previous,
              tab: value === DEFAULT_TAB ? undefined : value,
            })}
            aria-current={active ? "page" : undefined}
            className={cn(
              "relative flex items-center gap-2 whitespace-nowrap rounded-t-md px-3 py-2.5 text-sm font-semibold outline-none transition-colors focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
              active
                ? "text-foreground"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon aria-hidden className="size-4" />
            {label}
            {counts[value]}
            {warnings[value] !== undefined && (
              <>
                <AlertTriangle aria-hidden className="size-3.5 text-primary" />
                <span className="sr-only">{warnings[value]}</span>
              </>
            )}
            {active && (
              <motion.span
                layoutId="ai-usage-tab"
                className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-primary"
                transition={{ duration: 0.25, ease: [0.3, 0.7, 0.2, 1] }}
              />
            )}
          </Link>
        );
      })}
    </nav>
  );
}

function Count({
  attention = false,
  children,
}: {
  attention?: boolean;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        "rounded-full px-1.5 py-0.5 text-[11px] font-semibold tabular-nums leading-none",
        attention
          ? "bg-primary/10 text-primary"
          : "bg-muted text-muted-foreground",
      )}
    >
      {children}
    </span>
  );
}
