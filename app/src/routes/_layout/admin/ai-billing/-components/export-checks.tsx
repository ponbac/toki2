import * as React from "react";
import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { cn } from "@/lib/utils";
import type { CheckStatus } from "../../-lib/export-checks";

/** One row of the checklist. */
export type ExportCheck = Readonly<{
  id: string;
  label: string;
  status: CheckStatus;
  /** One line, in words, of where the check stands. */
  summary: string;
  /** What the row opens to; a check without it cannot be opened. */
  detail?: React.ReactNode;
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
} as const;

/**
 * What holds a month's bill back, one row per check. A check that needs
 * attention is open until the admin opens or closes a row; mount it with the
 * month as its key so a new month starts over.
 */
export function ExportChecks({ checks }: { checks: readonly ExportCheck[] }) {
  const [chosen, setChosen] = React.useState<string[] | null>(null);
  const open =
    chosen ??
    checks
      .filter((check) => check.status === "attention" && check.detail)
      .map((check) => check.id);

  return (
    <section
      aria-labelledby="export-checks-heading"
      className="rounded-xl border border-border/60 bg-card/60 p-3"
    >
      <h2
        id="export-checks-heading"
        className="px-2 pb-1.5 pt-1 text-sm font-semibold"
      >
        Before you export
      </h2>
      <Accordion
        type="multiple"
        value={open}
        onValueChange={setChosen}
        // The shared accordion animates its panels; the selector stops that
        // for a reader who asked for less motion.
        className="flex flex-col gap-0.5 motion-reduce:[&_[role=region]]:!animate-none"
      >
        {checks.map((check) => {
          const status = STATUS[check.status];
          return (
            <AccordionItem
              key={check.id}
              value={check.id}
              disabled={!check.detail}
              className="border-none"
            >
              <AccordionTrigger className="min-w-0 gap-2.5 rounded-lg px-2 py-2 text-left text-sm hover:bg-muted/50 hover:no-underline focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:hover:bg-transparent motion-reduce:transition-none [&:disabled>svg]:invisible [&>svg]:text-muted-foreground motion-reduce:[&>svg]:transition-none">
                {/* Wrapped: the trigger turns its own icons over when open. */}
                <span className="flex shrink-0">
                  <status.icon
                    className={cn("size-4", status.className)}
                    aria-hidden
                  />
                  <span className="sr-only">{status.label}:</span>
                </span>
                <span className="flex min-w-0 flex-1 flex-col gap-x-3 sm:flex-row sm:items-baseline">
                  <span className="shrink-0 font-semibold sm:w-28">
                    {check.label}
                  </span>
                  <span
                    className="min-w-0 font-normal text-muted-foreground sm:truncate"
                    title={check.summary}
                  >
                    {check.summary}
                  </span>
                </span>
              </AccordionTrigger>
              {check.detail && (
                <AccordionContent className="px-2 pb-3 pt-2 sm:pl-[2.125rem]">
                  {check.detail}
                </AccordionContent>
              )}
            </AccordionItem>
          );
        })}
      </Accordion>
    </section>
  );
}
