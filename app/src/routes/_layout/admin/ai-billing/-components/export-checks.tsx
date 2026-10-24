import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import type { CheckStatus } from "../../-lib/export-checks";

/** One check before a month's bill is exported. */
export type ExportCheck = Readonly<{
  id: string;
  label: string;
  status: CheckStatus;
  /** One line, in words, of where the check stands. */
  summary: string;
  /** Opens where the check is resolved. */
  onOpen: () => void;
}>;

const STATUS = {
  passed: {
    icon: CheckCircle2,
    className: "text-[#0ca30c]",
    label: "Passed",
  },
  attention: {
    icon: AlertTriangle,
    className: "text-[#c98500] dark:text-[#fab219]",
    label: "Needs attention",
  },
  pending: {
    icon: Loader2,
    className: "animate-spin text-muted-foreground motion-reduce:animate-none",
    label: "Checking",
  },
} as const satisfies Record<CheckStatus, unknown>;

/**
 * What holds the month's bill back, one cell per check. Each says where it
 * stands in a line and opens the place where it is resolved; none unfolds
 * here, so the bill stays in view however many developers there are.
 */
export function ExportChecks({ checks }: { checks: readonly ExportCheck[] }) {
  const open = checks.filter((check) => check.status === "attention").length;
  return (
    <section
      aria-labelledby="export-checks-heading"
      className="flex flex-col gap-2"
    >
      <h2 id="export-checks-heading" className="text-sm font-semibold">
        Before you export
        <span className="ml-2 font-normal text-muted-foreground">
          {open === 0
            ? "Nothing holds this bill back."
            : `${open} of ${checks.length} checks need a look.`}
        </span>
      </h2>
      <ul className="grid gap-px overflow-hidden rounded-xl border border-border/70 bg-border/70 sm:grid-cols-2 xl:grid-cols-4">
        {checks.map((check) => {
          const status = STATUS[check.status];
          return (
            <li key={check.id} className="bg-card">
              <button
                type="button"
                onClick={check.onOpen}
                className={cn(
                  "flex h-full w-full items-start gap-2.5 px-3.5 py-3 text-left transition-colors hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden focus-visible:ring-inset motion-reduce:transition-none",
                  check.status === "attention" &&
                    "bg-[#c98500]/[0.06] dark:bg-[#fab219]/[0.06]",
                )}
              >
                <status.icon
                  className={cn("mt-0.5 size-4 shrink-0", status.className)}
                  aria-hidden
                />
                <span className="flex min-w-0 flex-col gap-0.5">
                  <span className="text-sm font-semibold">
                    <span className="sr-only">{status.label}: </span>
                    {check.label}
                  </span>
                  <span
                    className="line-clamp-2 text-xs text-muted-foreground"
                    title={check.summary}
                  >
                    {check.summary}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
