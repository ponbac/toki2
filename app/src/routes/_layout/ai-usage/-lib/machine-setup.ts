import { API_URL } from "@/lib/api/api";

/** A system `token-ledger` runs on. Each has its own shell and scheduler. */
export type MachineOs = (typeof MACHINE_OSES)[number];

/** Every system, in display order. */
export const MACHINE_OSES = ["macos", "linux", "windows"] as const;

type OsGuide = Readonly<{
  label: string;
  /** The shell the commands are written for. */
  shell: string;
  /** What `schedule install` registers, and when it runs a missed sync. */
  scheduler: string;
  missedRun: string;
  /** The command that installs Node.js, which runs the CLI, where one fits all. */
  nodeInstallCommand: string | null;
}>;

/** What differs between systems when connecting a machine. */
export const OS_GUIDES: Readonly<Record<MachineOs, OsGuide>> = {
  macos: {
    label: "macOS",
    shell: "zsh",
    scheduler: "launchd agent",
    missedRun: "runs on wake, and each time you log in",
    nodeInstallCommand: "brew install node",
  },
  linux: {
    label: "Linux",
    shell: "bash",
    scheduler: "systemd user timer",
    missedRun: "runs within ten minutes of your next login",
    nodeInstallCommand: null,
  },
  windows: {
    label: "Windows",
    shell: "PowerShell",
    scheduler: "Task Scheduler task",
    missedRun: "runs at the next opportunity, on battery power as well",
    nodeInstallCommand: "winget install OpenJS.NodeJS",
  },
};

/** The system this browser runs on, or null when its user agent names none. */
export function detectMachineOs(): MachineOs | null {
  const userAgent = navigator.userAgent;
  if (/Windows/.test(userAgent)) {
    return "windows";
  }
  if (/Macintosh|Mac OS X/.test(userAgent)) {
    return "macos";
  }
  if (/Linux|X11/.test(userAgent)) {
    return "linux";
  }
  return null;
}

/** One step of the setup: what it does, and the lines to run for it. */
export type SetupCommand = Readonly<{
  label: string;
  lines: readonly string[];
}>;

/**
 * How to run the CLI. `--yes` keeps npx from asking to install, which would
 * swallow pasted lines. PowerShell would pick `npx.ps1`, which its default
 * execution policy blocks, so Windows names `npx.cmd`.
 */
export function tokenLedger(os: MachineOs): string {
  const npx = os === "windows" ? "npx.cmd" : "npx";
  return `${npx} --yes @ponbac/token-ledger@latest`;
}

/**
 * The commands that connect a machine, with this server and `token` filled in:
 * save both, upload once, then schedule the daily sync. A token is `toki_` and
 * hex digits, so no shell needs it quoted.
 */
export function setupCommands(
  os: MachineOs,
  token: string,
): readonly SetupCommand[] {
  const run = tokenLedger(os);
  const login = `${run} login --server ${API_URL}`;
  return [
    {
      label: "Save the server and your token",
      lines:
        os === "windows"
          ? [`$env:TOKI_API_TOKEN = '${token}'`, login]
          : [`TOKI_API_TOKEN=${token} ${login}`],
    },
    { label: "Upload the last 14 days", lines: [`${run} sync`] },
    {
      label: `Sync every day at 12:30 with a ${OS_GUIDES[os].scheduler}`,
      lines: [`${run} schedule install`],
    },
  ];
}

/** The commands as one paste: a line each, in order. */
export function setupScript(commands: readonly SetupCommand[]): string {
  return commands.flatMap((command) => command.lines).join("\n");
}
