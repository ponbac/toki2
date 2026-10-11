import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";

/** The billing rules, as the server applies them, and the steps after export. */
export function BillingRules({ timeZone }: { timeZone: string }) {
  return (
    <Accordion
      type="single"
      collapsible
      className="rounded-xl border border-border/60 bg-card/60 px-4"
    >
      <AccordionItem value="rules" className="border-none">
        <AccordionTrigger className="text-sm font-semibold">
          How AI usage is billed
        </AccordionTrigger>
        <AccordionContent>
          <ul className="list-disc space-y-2 pl-5 text-sm text-muted-foreground">
            <li>
              Days and months are calendar days in {timeZone}. Per developer and
              provider, each day is billed either by the subscription that
              covers it or as API usage.
            </li>
            <li>
              <span className="font-medium text-foreground">API days</span> bill
              the estimated API-equivalent cost of that day&apos;s usage, in
              USD, to the project it belongs to. Each line bills its estimate
              rounded to whole cents, and totals add up those amounts, so the
              CSV sums to the same figures; the unrounded estimate is shown
              apart.
            </li>
            <li>
              <span className="font-medium text-foreground">Subscriptions</span>{" "}
              bill their monthly fee in its own currency, pro-rated by covered
              days over days in the month (rounded half up to hundredths). The
              pro-rated fee is split across projects by each project&apos;s
              API-equivalent cost on the covered days, in exact hundredths that
              sum to the fee (largest remainder, ties to the first project).
            </li>
            <li>
              Unpriced usage has no known cost, never zero. A split weighs only
              priced usage; when nothing covered is priced, it falls back to
              token share, then record share, and says so.
            </li>
            <li>
              A subscription without usage on any covered day is{" "}
              <span className="font-medium text-foreground">
                unallocated overhead
              </span>
              . Usage whose project key is unmapped, stale or unattributed is{" "}
              <span className="font-medium text-foreground">Unassigned</span>;
              map its keys under Project mappings and the bill updates.
            </li>
            <li>
              <span className="font-medium text-foreground">Currencies</span>{" "}
              are never mixed or converted here. To invoice in one currency,
              convert each amount in the CSV export from its{" "}
              <code>billable_currency</code> at the exchange rate you choose, as
              a separate step.
            </li>
          </ul>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
