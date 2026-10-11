import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ClipboardCopy,
  KeyRound,
  Laptop,
  Loader2,
  PartyPopper,
  RefreshCw,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import { toast } from "sonner";
import { AiProviderLabel } from "@/components/ai-provider-icon";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  useApiTokenIssuance,
  type ApiTokenIssuance,
} from "@/hooks/use-api-token-issuance";
import {
  aiUsageReportQueries,
  aiUsageReportQueryKeys,
  type AiUsageMachine,
  type AiUsageRange,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { OsIcon } from "./os-icon";
import {
  MACHINE_OSES,
  OS_GUIDES,
  detectMachineOs,
  setupCommands,
  setupScript,
  type MachineOs,
} from "../-lib/machine-setup";

const STEPS = [
  { title: "Pick the machine's system", icon: Laptop },
  { title: "Create an API token", icon: KeyRound },
  { title: "Run this in a terminal", icon: Terminal },
  { title: "First sync", icon: RefreshCw },
] as const;

const TOKEN_STEP = 1;
const COMMANDS_STEP = 2;
const SYNC_STEP = 3;

/** How often to look for the sync while the dialog waits for it. */
const POLL_INTERVAL_MS = 3_000;

const SETUP_DOCS = "https://github.com/ponbac/token-ledger#sync-to-toki2";

/**
 * Walks through connecting a machine: its system, an API token created here,
 * the commands with both filled in, and the wait for its first sync. The steps
 * outlive closing the dialog, since the token's secret cannot be shown again.
 */
export function ConnectMachineDialog({
  open,
  onOpenChange,
  range,
  fallbackFocusRef,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The range of the page's machine listing, to share its query. */
  range: AiUsageRange;
  /** Takes focus on close when the control that opened the dialog is gone. */
  fallbackFocusRef: React.RefObject<HTMLElement | null>;
}) {
  const queryClient = useQueryClient();
  const openerRef = React.useRef<HTMLElement | null>(null);
  const detected = detectMachineOs();
  const [os, setOs] = React.useState<MachineOs>(detected ?? "linux");
  const [step, setStep] = React.useState(0);
  const [nameDraft, setNameDraft] = React.useState<string | null>(null);
  const issuance = useApiTokenIssuance();
  const { issued } = issuance;

  const machinesQuery = useQuery({
    ...aiUsageReportQueries.aiUsageMachines(range),
    refetchInterval: (query) =>
      open &&
      issued !== null &&
      !syncedSince(query.state.data?.machines, issued.createdAt)
        ? POLL_INTERVAL_MS
        : false,
  });
  const connected = issued
    ? syncedSince(machinesQuery.data?.machines, issued.createdAt)
    : undefined;
  const connectedId = connected?.machineId;

  // The sync brought usage that the page's other queries have not seen.
  React.useEffect(() => {
    if (connectedId !== undefined) {
      void queryClient.invalidateQueries({
        queryKey: aiUsageReportQueryKeys.all,
      });
    }
  }, [connectedId, queryClient]);

  const reset = () => {
    issuance.dismiss();
    setStep(0);
    setNameDraft(null);
  };

  // The commands stay up after the sync lands: the scheduling one may be left.
  const finished = connected !== undefined && step === SYNC_STEP;
  const { title, icon: StepIcon } = STEPS[step];

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && finished) {
          reset();
        }
        onOpenChange(next);
      }}
    >
      <DialogContent
        className="max-h-[calc(100dvh-2rem)] overflow-y-auto outline-none sm:max-w-xl"
        // Focus starts on the dialog, not on a system that is not the selected one.
        onOpenAutoFocus={(event) => {
          openerRef.current =
            document.activeElement instanceof HTMLElement
              ? document.activeElement
              : null;
          if (event.currentTarget instanceof HTMLElement) {
            event.preventDefault();
            event.currentTarget.focus();
          }
        }}
        // The dialog has no trigger of its own to return focus to.
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          const opener = openerRef.current;
          (opener?.isConnected ? opener : fallbackFocusRef.current)?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Connect a machine</DialogTitle>
          <DialogDescription className="flex items-center justify-center gap-2 sm:justify-start">
            <StepIcon aria-hidden className="size-4 text-primary" />
            Step {step + 1} of {STEPS.length} · {title}
          </DialogDescription>
        </DialogHeader>
        <div aria-hidden className="grid grid-cols-4 gap-1.5">
          {STEPS.map((entry, index) => (
            <span
              key={entry.title}
              className={cn(
                "h-1 rounded-full transition-colors",
                index <= step ? "bg-primary" : "bg-muted",
              )}
            />
          ))}
        </div>

        <div className="min-h-44 space-y-3">
          {step === 0 && (
            <SystemStep os={os} detected={detected} onChange={setOs} />
          )}
          {step === TOKEN_STEP && (
            <TokenStep
              name={nameDraft ?? `token-ledger ${OS_GUIDES[os].label}`}
              onNameChange={setNameDraft}
              issuance={issuance}
              onReplace={issuance.dismiss}
            />
          )}
          {step === COMMANDS_STEP && issued && (
            <CommandsStep os={os} token={issued.token} connected={connected} />
          )}
          {step === SYNC_STEP && <SyncStep connected={connected} />}
        </div>

        <DialogFooter className="gap-2 sm:justify-between sm:space-x-0">
          {finished ? (
            <>
              <Button type="button" variant="ghost" onClick={reset}>
                Add another machine
              </Button>
              <Button
                type="button"
                onClick={() => {
                  reset();
                  onOpenChange(false);
                }}
              >
                Show my usage
              </Button>
            </>
          ) : (
            <>
              <Button
                type="button"
                variant="ghost"
                disabled={step === 0}
                onClick={() => setStep(step - 1)}
              >
                <ArrowLeft />
                Back
              </Button>
              {step === SYNC_STEP ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => onOpenChange(false)}
                >
                  Close
                </Button>
              ) : (
                <Button
                  type="button"
                  disabled={step === TOKEN_STEP && issued === null}
                  onClick={() => setStep(step + 1)}
                >
                  {step === COMMANDS_STEP ? "I ran them" : "Next"}
                  <ArrowRight />
                </Button>
              )}
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/**
 * The machine that synced after the token was created, if any; the listing
 * leads with the latest sync. Another machine's scheduled sync landing in the
 * meantime would pass for it, which its label then shows.
 */
function syncedSince(
  machines: readonly AiUsageMachine[] | undefined,
  since: string,
): AiUsageMachine | undefined {
  const start = Date.parse(since);
  return machines?.find((machine) => Date.parse(machine.lastSyncedAt) >= start);
}

function SystemStep({
  os,
  detected,
  onChange,
}: {
  os: MachineOs;
  detected: MachineOs | null;
  onChange: (os: MachineOs) => void;
}) {
  const { nodeInstallCommand } = OS_GUIDES[os];
  return (
    <>
      <fieldset className="grid grid-cols-3 gap-2">
        <legend className="sr-only">Operating system</legend>
        {MACHINE_OSES.map((option) => (
          <label
            key={option}
            className={cn(
              "flex cursor-pointer flex-col items-center gap-2 rounded-xl border px-3 py-4 transition-colors has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring",
              option === os
                ? "border-primary bg-primary/10"
                : "border-border/70 text-muted-foreground hover:bg-muted/40 hover:text-foreground",
            )}
          >
            <input
              type="radio"
              name="connect-machine-os"
              className="sr-only"
              checked={option === os}
              onChange={() => onChange(option)}
            />
            <OsIcon os={option} className="size-7" />
            <span className="text-sm font-medium">
              {OS_GUIDES[option].label}
            </span>
            <span className="h-4 text-[11px] text-muted-foreground">
              {option === detected && "This browser"}
            </span>
          </label>
        ))}
      </fieldset>
      <p className="text-xs text-muted-foreground">
        Pick the system of the machine you are connecting. token-ledger runs
        through <code>npx</code> and needs Node.js 24 or newer: check with{" "}
        <code>node --version</code>, and install it with{" "}
        {nodeInstallCommand ? (
          <code>{nodeInstallCommand}</code>
        ) : (
          "your package manager, mise or nvm"
        )}
        .
      </p>
    </>
  );
}

function TokenStep({
  name,
  onNameChange,
  issuance,
  onReplace,
}: {
  name: string;
  onNameChange: (name: string) => void;
  issuance: ApiTokenIssuance;
  /** Drops the token from the setup, for one that was revoked meanwhile. */
  onReplace: () => void;
}) {
  const { issued } = issuance;
  if (issued) {
    return (
      <div className="flex items-start gap-3 rounded-xl border border-primary/30 bg-primary/5 p-3">
        <Check aria-hidden className="mt-0.5 size-4 shrink-0 text-primary" />
        <div className="space-y-1">
          <p className="text-sm">
            <span className="font-medium">{issued.name}</span>{" "}
            <span className="font-mono text-xs text-muted-foreground">
              {issued.prefix}…
            </span>{" "}
            is ready.
          </p>
          <p className="text-xs text-muted-foreground">
            Its secret is filled into the commands of the next step. Toki shows
            it only this once, so finish the setup before you leave this page.
          </p>
          <p className="text-xs text-muted-foreground">
            Revoked it since?{" "}
            <button
              type="button"
              className="underline underline-offset-2 hover:text-foreground"
              onClick={onReplace}
            >
              Create another token
            </button>
            .
          </p>
        </div>
      </div>
    );
  }

  const trimmed = name.trim();
  const disabled = !trimmed || issuance.isPending;
  return (
    <form
      className="space-y-3"
      onSubmit={(event) => {
        event.preventDefault();
        if (!disabled) {
          issuance.issue(trimmed);
        }
      }}
    >
      <div className="flex items-end gap-2">
        <div className="min-w-0 flex-1 space-y-1">
          <Label htmlFor="connect-machine-token-name" className="text-xs">
            Token name
          </Label>
          <Input
            id="connect-machine-token-name"
            value={name}
            maxLength={64}
            onChange={(event) => onNameChange(event.target.value)}
            disabled={issuance.isPending}
          />
        </div>
        <Button type="submit" disabled={disabled}>
          <KeyRound />
          {issuance.isPending ? "Creating..." : "Create token"}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        Name it after the machine. A token per machine lets you revoke one
        machine alone, under API tokens in Account settings.
      </p>
    </form>
  );
}

function CommandsStep({
  os,
  token,
  connected,
}: {
  os: MachineOs;
  token: string;
  connected: AiUsageMachine | undefined;
}) {
  const guide = OS_GUIDES[os];
  const commands = setupCommands(os, token);
  return (
    <>
      <div className="overflow-hidden rounded-xl border border-border/70 bg-muted/40">
        <div className="flex items-center justify-between gap-2 border-b border-border/60 py-1.5 pl-3 pr-1.5">
          <span className="flex items-center gap-2 text-xs text-muted-foreground">
            <OsIcon os={os} className="size-3.5" />
            {guide.label} · {guide.shell}
          </span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8"
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(setupScript(commands));
                toast.success("Commands copied");
              } catch {
                toast.error("Failed to copy the commands");
              }
            }}
          >
            <ClipboardCopy />
            Copy all
          </Button>
        </div>
        <ol className="space-y-2.5 p-3">
          {commands.map((command, index) => (
            <li key={command.label} className="space-y-1">
              <p className="text-xs text-muted-foreground">
                {index + 1}. {command.label}
              </p>
              <pre className="whitespace-pre-wrap font-mono text-xs leading-5 [overflow-wrap:anywhere]">
                {command.lines.map((line, lineIndex) => (
                  <React.Fragment key={line}>
                    {lineIndex > 0 && "\n"}
                    <CommandLine line={line} token={token} />
                  </React.Fragment>
                ))}
              </pre>
            </li>
          ))}
        </ol>
      </div>
      {connected && (
        <p role="status" className="flex items-start gap-2 text-xs">
          <Check aria-hidden className="mt-px size-3.5 shrink-0 text-primary" />
          <span>
            {connected.label} has synced. Run the last command as well, so that
            it keeps syncing.
          </span>
        </p>
      )}
      <p className="flex items-start gap-2 text-xs text-muted-foreground">
        <ShieldCheck
          aria-hidden
          className="mt-px size-3.5 shrink-0 text-primary"
        />
        <span>
          Only totals leave the machine: tokens and estimated cost per hour,
          session, project, tool and model. Never prompts, responses, code or
          file paths.
        </span>
      </p>
      <p className="text-xs text-muted-foreground">
        A missed daily run {guide.missedRun}.
        {os === "linux" &&
          " Without systemd, as in WSL, schedule it with cron instead."}{" "}
        Copilot is read only once its file export is on. See the{" "}
        <a
          href={SETUP_DOCS}
          target="_blank"
          rel="noreferrer"
          className="underline underline-offset-2"
        >
          setup details
        </a>
        .
      </p>
    </>
  );
}

/** A command line with the token set apart, as the part that is secret. */
function CommandLine({ line, token }: { line: string; token: string }) {
  const [before, after] = line.split(token);
  if (after === undefined) {
    return line;
  }
  return (
    <>
      {before}
      <span className="text-primary">{token}</span>
      {after}
    </>
  );
}

function SyncStep({ connected }: { connected: AiUsageMachine | undefined }) {
  if (!connected) {
    return (
      <div
        role="status"
        className="flex items-start gap-3 rounded-xl border border-border/70 bg-muted/40 p-4"
      >
        <Loader2
          aria-hidden
          className="mt-0.5 size-4 shrink-0 animate-spin text-primary"
        />
        <div className="space-y-1">
          <p className="text-sm font-medium">Waiting for its first sync…</p>
          <p className="text-xs text-muted-foreground">
            This updates by itself once <code>sync</code> has uploaded. Nothing
            after a minute? Check the command's output in your terminal.
          </p>
        </div>
      </div>
    );
  }

  const read = connected.coverage
    .filter((entry) => entry.status === "ok")
    .map((entry) => entry.provider);
  return (
    <div
      role="status"
      className="space-y-2 rounded-xl border border-primary/30 bg-primary/5 p-4"
    >
      <p className="flex items-center gap-2 text-sm font-medium">
        <PartyPopper aria-hidden className="size-4 shrink-0 text-primary" />
        <span className="min-w-0 truncate">{connected.label} is connected</span>
      </p>
      {read.length > 0 ? (
        <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          History read in full:
          {read.map((provider) => (
            <AiProviderLabel
              key={provider}
              provider={provider}
              iconClassName="size-3"
            />
          ))}
        </p>
      ) : (
        <p className="text-xs text-muted-foreground">
          No tool's history was read in full; the Machines card says what is
          missing.
        </p>
      )}
      <p className="text-xs text-muted-foreground">
        Do the same on every machine where you use a coding agent.
      </p>
    </div>
  );
}
