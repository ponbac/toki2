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
              <span className="font-medium text-foreground">Currencies.</span>{" "}
              Everything is billed in SEK at the Riksbank&apos;s average rate
              for the month, stored with the month so the bill can be
              reproduced; the original amount and currency stay beside it. A
              month in progress uses its average so far, which is{" "}
              <span className="font-medium text-foreground">provisional</span>{" "}
              and refreshed after each Riksbank publication (12:15 on banking
              days, at most hourly) until the month&apos;s final average
              replaces it. An admin can override a month&apos;s rate; the
              fetched rate is kept beneath it, never replaces it, and applies
              again on reset.
            </li>
            <li>
              Each line converts its billed amount (for API lines, the whole
              cents) and rounds to öre. A subscription&apos;s pro-rated fee is
              converted once and split across projects like the fee, so the SEK
              shares add up to the converted fee exactly. SEK totals are sums of
              the converted lines.
            </li>
            <li>
              Without a rate, nothing is guessed: the SEK amounts that need it
              and the totals that include them are left empty and flagged. The
              CSV keeps <code>billable_amount</code> and{" "}
              <code>billable_currency</code> beside <code>billable_sek</code>,{" "}
              <code>rate</code>, <code>rate_source</code> and{" "}
              <code>rate_provisional</code>, so it can still be billed per
              currency.
            </li>
          </ul>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
