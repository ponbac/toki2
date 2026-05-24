import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function EntryCardFrame(props: {
  children: ReactNode;
  className?: string;
  showHoverOverlay?: boolean;
}) {
  return (
    <div
      className={cn(
        "group relative overflow-hidden rounded-xl border border-border/50 bg-card/50 p-4 transition-all duration-300",
        "hover:border-border hover:bg-card hover:shadow-elevated",
        props.className,
      )}
    >
      {props.showHoverOverlay && (
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-r from-primary/0 via-primary/0 to-primary/0 opacity-0 transition-opacity duration-300 group-hover:from-primary/[0.02] group-hover:to-transparent group-hover:opacity-100" />
      )}
      <div className="relative flex gap-4">{props.children}</div>
    </div>
  );
}
