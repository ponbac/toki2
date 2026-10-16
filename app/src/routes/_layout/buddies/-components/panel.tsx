import * as React from "react";
import type { BattleResult } from "@/lib/api/queries/buddy";
import { cn } from "@/lib/utils";
import { resultLabel } from "../-lib/format";

/** A titled section of the Buddies page; `action` sits at the title's right. */
export function Panel({
  title,
  description,
  action,
  className,
  children,
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
}) {
  const titleId = React.useId();
  return (
    <section
      aria-labelledby={titleId}
      className={cn("card-elevated rounded-2xl p-5", className)}
    >
      <div className="flex min-h-8 flex-wrap items-center justify-between gap-x-3 gap-y-2">
        <h2
          id={titleId}
          className="font-display text-base font-semibold leading-tight"
        >
          {title}
        </h2>
        {action}
      </div>
      {description && (
        <p className="mt-1 text-sm text-muted-foreground">{description}</p>
      )}
      <div className="mt-4">{children}</div>
    </section>
  );
}

const RESULT_TONES: Readonly<Record<BattleResult, string>> = {
  win: "bg-[#0ca30c]/15 text-[#0a840a] dark:text-[#3fd03f]",
  loss: "bg-[#d03b3b]/15 text-[#b83232] dark:text-[#ee7a7a]",
  draw: "bg-muted text-muted-foreground",
};

/** How a battle ended for the current user, as a label that is never colour alone. */
export function ResultBadge({
  result,
  className,
}: {
  result: BattleResult;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex w-11 shrink-0 justify-center rounded-md px-1.5 py-0.5 text-xs font-semibold",
        RESULT_TONES[result],
        className,
      )}
    >
      {resultLabel(result)}
    </span>
  );
}

/** A small rounded status label, such as the battle cooldown. */
export function Pill({
  tone,
  className,
  children,
}: {
  tone: "ready" | "waiting";
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-semibold tabular-nums",
        tone === "ready"
          ? "bg-primary/15 text-primary"
          : "bg-muted text-muted-foreground",
        className,
      )}
    >
      {children}
    </span>
  );
}
