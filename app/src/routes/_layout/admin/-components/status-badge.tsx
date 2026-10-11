import { AlertTriangle, CheckCircle2, CircleSlash, Info } from "lucide-react";
import { cn } from "@/lib/utils";

const TONES = {
  good: { icon: CheckCircle2, className: "text-[#0ca30c]" },
  warning: {
    icon: AlertTriangle,
    className: "text-[#c98500] dark:text-[#fab219]",
  },
  critical: {
    icon: CircleSlash,
    className: "text-[#d03b3b] dark:text-[#e66767]",
  },
  neutral: { icon: Info, className: "text-muted-foreground" },
} as const;

/** A state shown as an icon and a label, never by color alone. The label
 * keeps the text color; only the icon carries the status color. */
export function StatusBadge({
  tone,
  label,
  className,
}: {
  tone: keyof typeof TONES;
  label: string;
  className?: string;
}) {
  const { icon: Icon, className: toneClass } = TONES[tone];
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 whitespace-nowrap rounded-md border border-border/60 bg-background/60 px-1.5 py-0.5 text-xs font-medium text-foreground",
        className,
      )}
    >
      <Icon className={cn("size-3.5 shrink-0", toneClass)} aria-hidden />
      {label}
    </span>
  );
}
