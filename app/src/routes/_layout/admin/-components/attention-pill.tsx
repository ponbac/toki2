import type * as React from "react";
import { cn } from "@/lib/utils";

/** A small amber pill for something that needs attention; its text says what. */
export function AttentionPill({
  title,
  className,
  children,
}: {
  title?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      title={title}
      className={cn(
        "inline-flex items-center gap-1 whitespace-nowrap rounded-full bg-[#c98500]/15 px-1.5 py-0.5 text-[11px] font-semibold tabular-nums leading-none text-[#c98500] dark:bg-[#fab219]/15 dark:text-[#fab219]",
        className,
      )}
    >
      {children}
    </span>
  );
}
