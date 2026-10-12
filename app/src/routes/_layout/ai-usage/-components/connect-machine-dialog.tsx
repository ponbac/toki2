import * as React from "react";
import {
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ClipboardCopy,
  Info,
  KeyRound,
  Laptop,
  Loader2,
  PartyPopper,
  RefreshCw,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import { toast } from "sonner";
import { AiProviderIcon, AiProviderLabel } from "@/components/ai-provider-icon";
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
  type AiUsageMachineList,
  type AiUsageRange,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { Segmented } from "./chart-card";
import { CopilotSurfaceIcon } from "./copilot-surface-icon";
import { OsIcon } from "./os-icon";
import { QueryError } from "./query-state";
import {
  COPILOT_CLI_VARIABLE,
  COPILOT_GUIDES,
  COPILOT_SURFACES,
  COPILOT_SURFACE_LABELS,
  COPILOT_VSCODE_SETTINGS,
  VSCODE_SYNC_IGNORED,
  copilotCliCommands,
  copilotEnableCommand,
  copilotSettingsCommand,
  isTracked,
  type CopilotSurface,
  type TrackedCopilotSurface,
} from "../-lib/copilot-setup";
import {
  MACHINE_OSES,
  OS_GUIDES,
  detectMachineOs,
  setupCommands,
  setupScript,
  type MachineOs,
  type SetupCommand,
} from "../-lib/machine-setup";

type StepId = "system" | "token" | "commands" | "copilot" | "sync";

const STEPS: Readonly<
  Record<StepId, { title: string; icon: React.ElementType }>
> = {
  system: { title: "Pick the machine's system", icon: Laptop },
  token: { title: "Create an API token", icon: KeyRound },
  commands: { title: "Run this in a terminal", icon: Terminal },
  copilot: { title: "Turn on Copilot's usage export", icon: CopilotStepIcon },
  sync: { title: "First sync", icon: RefreshCw },
};

/**
 * The steps in order. Copilot's comes before the wait for the sync, and only
 * for someone who uses Copilot where its usage can be read.
 */
function stepsFor(copilot: boolean): readonly StepId[] {
  return copilot
    ? ["system", "token", "commands", "copilot", "sync"]
    : ["system", "token", "commands", "sync"];
}

/** Copilot's mark as a step's icon, in the colour the step gives it. */
function CopilotStepIcon({ className }: { className?: string }) {
  return <AiProviderIcon provider="copilot" plain className={className} />;
}

/** How often to look for the sync while the dialog waits for it. */
const POLL_INTERVAL_MS = 3_000;

const SETUP_DOCS = "https://github.com/ponbac/token-ledger#sync-to-toki2";
const UPLOAD_DOCS =
  "https://github.com/ponbac/token-ledger#what-leaves-the-machine";
const VSCODE_EXPORT_BUG_DOCS =
  "https://github.com/ponbac/token-ledger#workaround-for-vs-codes-file-exporter-bug";
const VSCODE_CONTENT_BUG = "https://github.com/microsoft/vscode/issues/326254";

/**
 * Walks through connecting a machine: its system and where Copilot runs on it,
 * an API token created here, the commands with both filled in, Copilot's
 * export for someone who uses it, and the wait for the first sync. The steps
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
  const [copilot, setCopilot] = React.useState<readonly CopilotSurface[]>([]);
  const [step, setStep] = React.useState(0);
  const [nameDraft, setNameDraft] = React.useState<string | null>(null);
  // The machines that had synced before the token was created.
  const [knownIds, setKnownIds] = React.useState<readonly string[]>([]);
  const issuance = useApiTokenIssuance();
  const { issued } = issuance;

  // A known machine's sync does not end the wait: the new one may still come.
  const machinesQuery = useQuery({
    ...aiUsageReportQueries.aiUsageMachines(range),
    refetchInterval: (query) =>
      open &&
      issued !== null &&
      (findArrival(query.state.data?.machines, issued.createdAt, knownIds)
        ?.known ??
        true)
        ? POLL_INTERVAL_MS
        : false,
  });
  const arrival = issued
    ? findArrival(machinesQuery.data?.machines, issued.createdAt, knownIds)
    : undefined;
  const arrivedId = arrival?.machine.machineId;

  // The sync brought usage that the page's other queries have not seen.
  React.useEffect(() => {
    if (arrivedId !== undefined) {
      void queryClient.invalidateQueries({
        queryKey: aiUsageReportQueryKeys.all,
      });
    }
  }, [arrivedId, queryClient]);

  // Without the listing, every machine would later pass for a new one.
  const machines = machinesQuery.data?.machines;
  const issueToken = (name: string) => {
    if (machines === undefined) {
      return;
    }
    setKnownIds(machines.map((machine) => machine.machineId));
    issuance.issue(name);
  };

  const reset = () => {
    issuance.dismiss();
    setStep(0);
    setNameDraft(null);
  };

  const finish = () => {
    reset();
    onOpenChange(false);
  };

  // The commands stay up after the sync lands: the scheduling one may be left.
  // A known machine's sync may not be the setup's, so it does not end it either.
  const copilotSurfaces = COPILOT_SURFACES.filter(isTracked).filter((surface) =>
    copilot.includes(surface),
  );
  // The surfaces can only change on the first step, so `step` stays in range.
  const steps = stepsFor(copilotSurfaces.length > 0);
  const current = steps[step];
  const finished =
    arrival !== undefined && !arrival.known && current === "sync";
  const { title, icon: StepIcon } = STEPS[current];

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
            Step {step + 1} of {steps.length} · {title}
          </DialogDescription>
        </DialogHeader>
        <div aria-hidden className="grid auto-cols-fr grid-flow-col gap-1.5">
          {steps.map((entry, index) => (
            <span
              key={entry}
              className={cn(
                "h-1 rounded-full transition-colors",
                index <= step ? "bg-primary" : "bg-muted",
              )}
            />
          ))}
        </div>

        <div className="min-h-44 space-y-3">
          {current === "system" && (
            <SystemStep
              os={os}
              detected={detected}
              onChange={setOs}
              copilot={copilot}
              onCopilotChange={setCopilot}
            />
          )}
          {current === "token" && (
            <>
              {machines === undefined &&
                issued === null &&
                (machinesQuery.isError ? (
                  <QueryError
                    message="Could not load your machines, which the setup needs first."
                    error={machinesQuery.error}
                    onRetry={() => void machinesQuery.refetch()}
                  />
                ) : (
                  <p className="text-xs text-muted-foreground" role="status">
                    Loading your machines…
                  </p>
                ))}
              <TokenStep
                name={nameDraft ?? `token-ledger ${OS_GUIDES[os].label}`}
                onNameChange={setNameDraft}
                issuance={{ ...issuance, issue: issueToken }}
                ready={machines !== undefined}
                onReplace={issuance.dismiss}
              />
            </>
          )}
          {current === "commands" && issued && (
            <CommandsStep
              os={os}
              token={issued.token}
              synced={arrival?.known ? undefined : arrival?.machine}
              copilotNext={copilotSurfaces.length > 0}
            />
          )}
          {current === "copilot" && (
            <CopilotStep os={os} surfaces={copilotSurfaces} />
          )}
          {current === "sync" && (
            <SyncStep
              arrival={arrival}
              query={machinesQuery}
              onConfirm={finish}
            />
          )}
        </div>

        <DialogFooter className="gap-2 sm:justify-between sm:space-x-0">
          {finished ? (
            <>
              <Button type="button" variant="ghost" onClick={reset}>
                Add another machine
              </Button>
              <Button type="button" onClick={finish}>
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
              {current === "sync" ? (
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
                  disabled={current === "token" && issued === null}
                  onClick={() => setStep(step + 1)}
                >
                  {current === "commands" ? "I ran them" : "Next"}
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

/** A sync that landed during the setup, and whether its machine had synced before it. */
type Arrival = Readonly<{ machine: AiUsageMachine; known: boolean }>;

/**
 * The sync that landed after the token was created, if any. One from a machine
 * unseen before the setup is surely the setup's. One from a known machine may
 * be that machine set up again, or just its scheduled sync, so a new machine's
 * takes precedence. The listing leads with the latest sync.
 */
function findArrival(
  machines: readonly AiUsageMachine[] | undefined,
  since: string,
  knownIds: readonly string[],
): Arrival | undefined {
  const start = Date.parse(since);
  const synced =
    machines?.filter((machine) => Date.parse(machine.lastSyncedAt) >= start) ??
    [];
  const added = synced.find((machine) => !knownIds.includes(machine.machineId));
  const machine = added ?? synced[0];
  return machine && { machine, known: added === undefined };
}

function SystemStep({
  os,
  detected,
  onChange,
  copilot,
  onCopilotChange,
}: {
  os: MachineOs;
  detected: MachineOs | null;
  onChange: (os: MachineOs) => void;
  /** Where Copilot is used on the machine, which nothing is by default. */
  copilot: readonly CopilotSurface[];
  onCopilotChange: (copilot: readonly CopilotSurface[]) => void;
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
      <fieldset>
        <legend className="mb-2 text-sm">
          <span className="font-medium">
            Do you use Copilot on this machine?
          </span>{" "}
          <span className="text-muted-foreground">
            Tick where. The other tools need nothing extra.
          </span>
        </legend>
        <div className="grid gap-2 sm:grid-cols-3">
          {COPILOT_SURFACES.map((surface) => {
            const checked = copilot.includes(surface);
            return (
              <label
                key={surface}
                className={cn(
                  "relative flex cursor-pointer flex-col gap-0.5 rounded-xl border px-3 py-2.5 text-sm transition-colors has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring",
                  checked
                    ? "border-primary bg-primary/10"
                    : "border-border/70 text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                  !isTracked(surface) && "border-dashed",
                )}
              >
                <input
                  type="checkbox"
                  className="sr-only"
                  checked={checked}
                  onChange={(event) =>
                    onCopilotChange(
                      event.target.checked
                        ? [...copilot, surface]
                        : copilot.filter((other) => other !== surface),
                    )
                  }
                />
                <span
                  aria-hidden
                  className={cn(
                    "absolute right-2.5 top-2.5 flex size-4 items-center justify-center rounded-[4px] border border-primary",
                    checked && "bg-primary text-primary-foreground",
                  )}
                >
                  {checked && <Check className="size-3" />}
                </span>
                <CopilotSurfaceIcon
                  surface={surface}
                  className="mb-1.5 size-5"
                />
                <span className="font-medium">
                  {COPILOT_SURFACE_LABELS[surface]}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  {isTracked(surface) ? "Adds a step" : "Not tracked"}
                </span>
              </label>
            );
          })}
        </div>
      </fieldset>
      {copilot.includes("visualStudio") && (
        <p role="status" className="flex items-start gap-2 text-xs">
          <Info aria-hidden className="mt-px size-3.5 shrink-0 text-primary" />
          <span>
            Copilot in Visual Studio is not tracked. token-ledger reads Copilot
            CLI and VS Code Copilot Chat only, so usage from Visual Studio, from
            other IDEs and from inline completions will be missing here, not
            shown as zero.
          </span>
        </p>
      )}
    </>
  );
}

function TokenStep({
  name,
  onNameChange,
  issuance,
  ready,
  onReplace,
}: {
  name: string;
  onNameChange: (name: string) => void;
  issuance: ApiTokenIssuance;
  /** Whether the machines are loaded, which creating a token waits for. */
  ready: boolean;
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
  const disabled = !ready || !trimmed || issuance.isPending;
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
  synced,
  copilotNext,
}: {
  os: MachineOs;
  token: string;
  /** The new machine, once its sync has landed while the commands are up. */
  synced: AiUsageMachine | undefined;
  /** Whether Copilot's export has the next step, and so needs no word here. */
  copilotNext: boolean;
}) {
  const guide = OS_GUIDES[os];
  return (
    <>
      <CommandBlock os={os} commands={setupCommands(os, token)} token={token} />
      {synced && (
        <p role="status" className="flex items-start gap-2 text-xs">
          <Check aria-hidden className="mt-px size-3.5 shrink-0 text-primary" />
          <span>
            {synced.label} has synced. Run the last command as well, so that it
            keeps syncing.
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
          session, project, tool and model, with the machine's name and time
          zone. A project is its configured name or its repository, as{" "}
          <code>host/owner/repo</code>. Never prompts, responses, code or file
          paths.{" "}
          <a
            href={UPLOAD_DOCS}
            target="_blank"
            rel="noreferrer"
            className="underline underline-offset-2"
          >
            Everything that is uploaded
          </a>
          .
        </span>
      </p>
      <p className="text-xs text-muted-foreground">
        A missed daily run {guide.missedRun}.
        {os === "linux" &&
          " Without systemd, as in WSL, schedule it with cron instead."}{" "}
        {!copilotNext && "Copilot is read only once its file export is on. "}
        See the{" "}
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

/**
 * Commands for one system's shell, with a way to copy them as one paste. A
 * lone command goes unnumbered.
 */
function CommandBlock({
  os,
  commands,
  token,
}: {
  os: MachineOs;
  commands: readonly SetupCommand[];
  /** The secret among the commands, to set apart. */
  token?: string;
}) {
  const guide = OS_GUIDES[os];
  const lone = commands.length === 1;
  return (
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
              toast.success(lone ? "Command copied" : "Commands copied");
            } catch {
              toast.error(
                lone
                  ? "Failed to copy the command"
                  : "Failed to copy the commands",
              );
            }
          }}
        >
          <ClipboardCopy />
          {lone ? "Copy" : "Copy all"}
        </Button>
      </div>
      <ol className="space-y-2.5 p-3">
        {commands.map((command, index) => (
          <li key={command.label} className="space-y-1">
            <p className="text-xs text-muted-foreground">
              {!lone && `${index + 1}. `}
              {command.label}
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
  );
}

/** A command line with the token set apart, as the part that is secret. */
function CommandLine({ line, token }: { line: string; token?: string }) {
  const [before, after] = token === undefined ? [line] : line.split(token);
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

type CopilotSetupMode = "automatic" | "manual";

const COPILOT_SETUP_MODES: readonly Readonly<{
  value: CopilotSetupMode;
  label: string;
}>[] = [
  { value: "automatic", label: "Automatic" },
  { value: "manual", label: "By hand" },
];

/**
 * Turns on Copilot's usage export where it is used on the machine: by one
 * command that asks before it changes anything, or by hand.
 */
function CopilotStep({
  os,
  surfaces,
}: {
  os: MachineOs;
  surfaces: readonly TrackedCopilotSurface[];
}) {
  const [mode, setMode] = React.useState<CopilotSetupMode>("automatic");
  const guide = COPILOT_GUIDES[os];
  const cli = surfaces.includes("cli");
  const vscode = surfaces.includes("vscode");
  return (
    <>
      <p className="text-xs text-muted-foreground">
        Copilot keeps no usage log until you turn one on, and sessions before
        that cannot be recovered. The log is a file on this machine. What is
        uploaded from it is{" "}
        <a
          href={UPLOAD_DOCS}
          target="_blank"
          rel="noreferrer"
          className="underline underline-offset-2"
        >
          the same totals as for the other tools
        </a>
        , never prompts or responses.
      </p>
      <div>
        <Segmented
          label="How to turn the export on"
          value={mode}
          options={COPILOT_SETUP_MODES}
          onChange={setMode}
        />
      </div>
      {mode === "automatic" ? (
        <>
          <CommandBlock
            os={os}
            commands={[copilotEnableCommand(os, surfaces)]}
          />
          <p className="text-xs text-muted-foreground">
            It lists these changes and asks before making them:
          </p>
          <ul className="space-y-1.5 text-xs text-muted-foreground">
            <li className="flex items-start gap-2">
              <Check
                aria-hidden
                className="mt-px size-3.5 shrink-0 text-primary"
              />
              <span>
                Creates <code>{guide.exportDir}</code>, the folder token-ledger
                already reads.
              </span>
            </li>
            {cli && (
              <li className="flex items-start gap-2">
                <CopilotSurfaceIcon surface="cli" className="mt-px" />
                <span>
                  Copilot CLI: sets <code>{COPILOT_CLI_VARIABLE}</code>{" "}
                  {guide.persistence}.
                </span>
              </li>
            )}
            {vscode && (
              <li className="flex items-start gap-2">
                <CopilotSurfaceIcon surface="vscode" className="mt-px" />
                <span>
                  VS Code: adds the <code>{COPILOT_VSCODE_SETTINGS}</code>{" "}
                  settings to <code>{guide.vscodeSettings}</code>, with this
                  machine's path filled in, and tells Settings Sync not to copy
                  that path to your other machines. The settings are shared by
                  all VS Code profiles, and VS Code Insiders gets them too if it
                  is installed.
                </span>
              </li>
            )}
          </ul>
          <p className="flex items-start gap-2 text-xs">
            <ShieldCheck
              aria-hidden
              className="mt-px size-3.5 shrink-0 text-primary"
            />
            <span>
              <span className="font-medium">No admin rights needed.</span>{" "}
              {guide.ownRights}
            </span>
          </p>
          <p className="text-xs text-muted-foreground">
            Then{" "}
            {cli && (
              <>
                open a new terminal before your next <code>copilot</code>{" "}
                session
              </>
            )}
            {cli && vscode && ", and "}
            {vscode && (
              <>
                in VS Code press <PaletteShortcut os={os} />, run{" "}
                <span className="font-medium">Developer: Reload Window</span>{" "}
                and start a new chat
              </>
            )}
            .
          </p>
        </>
      ) : (
        <>
          {cli && (
            <section className="space-y-2">
              <h3 className="flex items-center gap-2 text-sm font-medium">
                <CopilotSurfaceIcon surface="cli" className="size-4" />
                {COPILOT_SURFACE_LABELS.cli}
              </h3>
              <CommandBlock os={os} commands={copilotCliCommands(os)} />
              <p className="text-xs text-muted-foreground">
                Open a new terminal before your next <code>copilot</code>{" "}
                session.
                {os !== "windows" &&
                  " In another shell, add the export line to its startup file instead."}
              </p>
            </section>
          )}
          {vscode && (
            <section className="space-y-2">
              <h3 className="flex items-center gap-2 text-sm font-medium">
                <CopilotSurfaceIcon surface="vscode" className="size-4" />
                {COPILOT_SURFACE_LABELS.vscode}
              </h3>
              <ol className="list-decimal space-y-2 pl-5 text-sm">
                <li className="space-y-1.5">
                  <p>Run this in a terminal.</p>
                  <CommandBlock
                    os={os}
                    commands={[copilotSettingsCommand(os)]}
                  />
                </li>
                <li>
                  In VS Code, press <PaletteShortcut os={os} /> to open the
                  command palette, run{" "}
                  <span className="font-medium">
                    Preferences: Open User Settings (JSON)
                  </span>
                  , and paste {guide.copiesSettings ? "" : "the settings "}
                  on a new line right after the opening brace. The last line
                  keeps Settings Sync from copying this machine's path to your
                  other machines. In a VS Code profile other than the default,
                  run{" "}
                  <span className="font-medium">
                    Preferences: Open Application Settings (JSON)
                  </span>{" "}
                  instead: these settings are shared by all profiles and kept
                  there.
                </li>
                <li>
                  If the file already has one of these settings further down,
                  the older line wins, so delete it. For the{" "}
                  <code>{VSCODE_SYNC_IGNORED}</code> list, add the new item to
                  the older list and delete the pasted line instead.
                </li>
                <li>
                  Press <PaletteShortcut os={os} /> again, run{" "}
                  <span className="font-medium">Developer: Reload Window</span>,
                  and start a new chat.
                </li>
              </ol>
            </section>
          )}
        </>
      )}
      {vscode && (
        <p className="text-xs text-muted-foreground">
          VS Code is told to leave prompts and responses out of its log, but{" "}
          <a
            href={VSCODE_CONTENT_BUG}
            target="_blank"
            rel="noreferrer"
            className="underline underline-offset-2"
          >
            an open VS Code bug
          </a>{" "}
          can write them to that local file anyway. They are never uploaded.
        </p>
      )}
      {vscode && (
        <p className="text-xs text-muted-foreground">
          VS Code 1.139.1 leaves the repository out of this export, so its usage
          arrives as unassigned until you update.{" "}
          <a
            href={VSCODE_EXPORT_BUG_DOCS}
            target="_blank"
            rel="noreferrer"
            className="underline underline-offset-2"
          >
            Workaround
          </a>
          .
        </p>
      )}
    </>
  );
}

/** VS Code's default shortcut for the command palette on a system. */
function PaletteShortcut({ os }: { os: MachineOs }) {
  return (
    <span className="whitespace-nowrap">
      {COPILOT_GUIDES[os].paletteShortcut.map((key) => (
        <kbd
          key={key.cap}
          className="mx-px rounded-[5px] border border-b-2 border-input bg-background px-1.5 py-px font-mono text-[11px] text-foreground"
        >
          {key.name ? (
            <>
              <span aria-hidden>{key.cap}</span>
              <span className="sr-only">{key.name}</span>
            </>
          ) : (
            key.cap
          )}
        </kbd>
      ))}
    </span>
  );
}

function SyncStep({
  arrival,
  query,
  onConfirm,
}: {
  arrival: Arrival | undefined;
  /** The listing the wait polls; its failure would otherwise pass for waiting. */
  query: UseQueryResult<AiUsageMachineList, Error>;
  /** Ends the setup on the word that a known machine is the one set up. */
  onConfirm: () => void;
}) {
  // Until a new machine's sync arrives, the listing is still being polled.
  const waiting = arrival === undefined || arrival.known;
  return (
    <>
      {waiting && query.isError && (
        <QueryError
          message="Could not check for the sync."
          error={query.error}
          onRetry={() => void query.refetch()}
        />
      )}
      {arrival ? (
        <SyncedMachine arrival={arrival} onConfirm={onConfirm} />
      ) : (
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
              This updates by itself once <code>sync</code> has uploaded.
              Nothing after a minute? Check the command's output in your
              terminal.
            </p>
          </div>
        </div>
      )}
    </>
  );
}

function SyncedMachine({
  arrival: { machine, known },
  onConfirm,
}: {
  arrival: Arrival;
  onConfirm: () => void;
}) {
  const Icon = known ? Check : PartyPopper;
  const read = machine.coverage
    .filter((entry) => entry.status === "ok")
    .map((entry) => entry.provider);
  return (
    <div
      role="status"
      className="space-y-2 rounded-xl border border-primary/30 bg-primary/5 p-4"
    >
      <p className="flex items-center gap-2 text-sm font-medium">
        <Icon aria-hidden className="size-4 shrink-0 text-primary" />
        <span className="min-w-0 truncate">
          {machine.label} {known ? "synced just now" : "is connected"}
        </span>
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
      {known ? (
        <p className="text-xs text-muted-foreground">
          It had synced before this setup, so this may be its usual sync. If you
          are setting up another machine, its sync has not arrived yet, and this
          keeps checking.{" "}
          <button
            type="button"
            className="underline underline-offset-2 hover:text-foreground"
            onClick={onConfirm}
          >
            This is the machine I set up
          </button>
          .
        </p>
      ) : (
        <p className="text-xs text-muted-foreground">
          Do the same on every machine where you use a coding agent.
        </p>
      )}
    </div>
  );
}
