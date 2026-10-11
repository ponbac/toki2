import { Laptop, Plug, ShieldCheck } from "lucide-react";
import { AiProviderLabel } from "@/components/ai-provider-icon";
import { Button } from "@/components/ui/button";
import { AI_PROVIDERS } from "@/lib/api/contracts/ai-usage";

/** The heights of the preview's bars, in percent; a shape, not data. */
const PREVIEW_BARS = [30, 55, 42, 70, 20, 8, 12, 64, 88, 52, 76, 40, 10, 14];

/**
 * What the page shows before any machine has synced: the outline of the
 * dashboard to come, and the way to connect the first machine.
 */
export function ConnectMachinePrompt({ onConnect }: { onConnect: () => void }) {
  return (
    <section
      aria-labelledby="connect-machine-heading"
      className="grid [&>*]:col-start-1 [&>*]:row-start-1"
    >
      <DashboardPreview />
      <div className="z-10 w-[calc(100%-2rem)] max-w-md space-y-4 place-self-center rounded-2xl border border-border/70 bg-card p-6 text-center shadow-elevated-lg">
        <span className="mx-auto flex size-11 items-center justify-center rounded-xl bg-primary/10 text-primary">
          <Laptop aria-hidden className="size-5" />
        </span>
        <div className="space-y-1.5">
          <h2
            id="connect-machine-heading"
            className="font-display text-lg font-semibold tracking-tight"
          >
            No machine is syncing yet
          </h2>
          <p className="text-sm text-muted-foreground">
            Connect one and this page fills with your usage per day, project and
            model. It takes about two minutes.
          </p>
        </div>
        <ul
          aria-label="Tools whose usage is read"
          className="flex flex-wrap justify-center gap-x-3 gap-y-1.5 text-xs text-muted-foreground"
        >
          {AI_PROVIDERS.map((provider) => (
            <li key={provider}>
              <AiProviderLabel provider={provider} />
            </li>
          ))}
        </ul>
        <Button type="button" onClick={onConnect}>
          <Plug />
          Connect a machine
        </Button>
        <p className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
          <ShieldCheck aria-hidden className="size-3.5 shrink-0" />
          Only token totals leave the machine.
        </p>
      </div>
    </section>
  );
}

/** The dashboard's outline, dimmed and without figures. */
function DashboardPreview() {
  return (
    <div
      aria-hidden
      className="pointer-events-none select-none space-y-5 opacity-50"
    >
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <div
            key={index}
            className="space-y-2.5 rounded-2xl border border-border/70 bg-card p-4"
          >
            <div className="h-2 w-2/3 rounded-full bg-muted" />
            <div className="h-5 w-2/5 rounded-md bg-muted" />
            <div className="h-2 w-4/5 rounded-full bg-muted" />
          </div>
        ))}
      </div>
      <div className="rounded-2xl border border-border/70 bg-card p-5">
        <div className="h-2 w-32 rounded-full bg-muted" />
        <div className="mt-5 flex h-72 items-end gap-2">
          {PREVIEW_BARS.map((height, index) => (
            <div
              key={index}
              className="flex flex-1 flex-col overflow-hidden rounded-t-md"
              style={{ height: `${height}%` }}
            >
              <div className="flex-[2] bg-[color:var(--viz-2)]" />
              <div className="flex-[3] bg-[color:var(--viz-1)]" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
