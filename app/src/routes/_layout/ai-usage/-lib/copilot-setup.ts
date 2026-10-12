import {
  tokenLedger,
  type MachineOs,
  type SetupCommand,
} from "./machine-setup";

/**
 * Where someone may use Copilot. It keeps no usage log until a file export is
 * turned on, which only the first two have, so Visual Studio is asked about
 * only to say that its usage is not read.
 */
export const COPILOT_SURFACES = ["cli", "vscode", "visualStudio"] as const;

export type CopilotSurface = (typeof COPILOT_SURFACES)[number];

/** A surface whose usage `token-ledger` reads once its export is on. */
export type TrackedCopilotSurface = Exclude<CopilotSurface, "visualStudio">;

export const COPILOT_SURFACE_LABELS: Readonly<Record<CopilotSurface, string>> =
  {
    cli: "Copilot CLI",
    vscode: "VS Code Copilot Chat",
    visualStudio: "Visual Studio",
  };

/** Whether a surface's usage can be read. */
export function isTracked(
  surface: CopilotSurface,
): surface is TrackedCopilotSurface {
  return surface !== "visualStudio";
}

/** A key of a shortcut: what its cap shows, and its name where that is a symbol. */
type ShortcutKey = Readonly<{ cap: string; name?: string }>;

type CopilotGuide = Readonly<{
  /** The folder the exports are written to, which `token-ledger` reads. */
  exportDir: string;
  /** Where the variable is set to last for new terminals, to end a sentence. */
  persistence: string;
  /**
   * VS Code's user settings, for the stable build's default profile, which is
   * where the export's settings are read from whatever profile is in use.
   */
  vscodeSettings: string;
  /** VS Code's default shortcut for the command palette. */
  paletteShortcut: readonly ShortcutKey[];
  /** Whether the settings command leaves the settings on the clipboard. */
  copiesSettings: boolean;
  /** How to run a command with only one's own rights, and why. */
  ownRights: string;
}>;

const CTRL_SHIFT_P: readonly ShortcutKey[] = [
  { cap: "Ctrl" },
  { cap: "Shift" },
  { cap: "P" },
];

const WITHOUT_SUDO =
  "Run it without sudo. Everything it changes is in your own home folder, and it stops when run as root, since Copilot could not write to files that root owns.";

/** What differs between systems when turning on Copilot's export. */
export const COPILOT_GUIDES: Readonly<Record<MachineOs, CopilotGuide>> = {
  macos: {
    exportDir: "~/.copilot/otel",
    persistence: "in your shell's startup file, such as ~/.zshrc",
    vscodeSettings: "~/Library/Application Support/Code/User/settings.json",
    paletteShortcut: [
      { cap: "⇧", name: "Shift" },
      { cap: "⌘", name: "Command" },
      { cap: "P" },
    ],
    copiesSettings: true,
    ownRights: WITHOUT_SUDO,
  },
  linux: {
    exportDir: "~/.copilot/otel",
    persistence: "in your shell's startup file, such as ~/.bashrc",
    vscodeSettings: "~/.config/Code/User/settings.json",
    paletteShortcut: CTRL_SHIFT_P,
    copiesSettings: false,
    ownRights: WITHOUT_SUDO,
  },
  windows: {
    exportDir: "%USERPROFILE%\\.copilot\\otel",
    persistence: "as a user environment variable",
    vscodeSettings: "%APPDATA%\\Code\\User\\settings.json",
    paletteShortcut: CTRL_SHIFT_P,
    copiesSettings: true,
    ownRights:
      'Use a normal PowerShell window, not "Run as administrator". Everything it changes belongs to your own account, and an administrator window signed in as someone else would set up that account instead.',
  },
};

/** The variable whose path is the whole switch for Copilot CLI's export. */
export const COPILOT_CLI_VARIABLE = "COPILOT_OTEL_FILE_EXPORTER_PATH";

/** VS Code's list of the settings that Settings Sync leaves on one machine. */
export const VSCODE_SYNC_IGNORED = "settingsSync.ignoredSettings";

/** The prefix of the settings that turn on VS Code Copilot Chat's export. */
export const COPILOT_VSCODE_SETTINGS = "github.copilot.chat.otel.*";

/**
 * The one command that turns the export on for `surfaces`: `token-ledger`
 * lists what it will change, asks, and then does it.
 */
export function copilotEnableCommand(
  os: MachineOs,
  surfaces: readonly TrackedCopilotSurface[],
): SetupCommand {
  const flags = surfaces.map((surface) => ` --${surface}`).join("");
  const names = surfaces.map((surface) => COPILOT_SURFACE_LABELS[surface]);
  return {
    label: `Turn the export on for ${names.join(" and ")}`,
    lines: [`${tokenLedger(os)} copilot enable${flags}`],
  };
}

/**
 * The commands that turn on Copilot CLI's export by hand: the folder, then
 * the variable for every new terminal. The macOS and Linux line is for the
 * shell the system's commands are written for, and starts on a line of its
 * own even after a startup file that does not end with one.
 */
export function copilotCliCommands(os: MachineOs): readonly SetupCommand[] {
  const folder = "Create the folder the export is written to";
  const variable = "Turn the export on in every new terminal";
  if (os === "windows") {
    return [
      {
        label: folder,
        lines: ['New-Item -ItemType Directory -Force "$HOME\\.copilot\\otel"'],
      },
      {
        label: variable,
        lines: [
          `[Environment]::SetEnvironmentVariable("${COPILOT_CLI_VARIABLE}", "$HOME\\.copilot\\otel\\usage.jsonl", "User")`,
        ],
      },
    ];
  }
  const startupFile = os === "macos" ? "~/.zshrc" : "~/.bashrc";
  return [
    { label: folder, lines: ["mkdir -p ~/.copilot/otel"] },
    {
      label: variable,
      lines: [
        `printf '\\n%s\\n' 'export ${COPILOT_CLI_VARIABLE}="$HOME/.copilot/otel/usage.jsonl"' >> ${startupFile}`,
      ],
    },
  ];
}

/**
 * VS Code's export settings, as lines of its settings file, writing to `home`.
 * The last keeps Settings Sync from copying the path, which is this machine's
 * alone, to the person's other machines, where it would name no folder. Each
 * line ends in a comma, so that they can go first in a file that has other
 * settings; VS Code accepts the trailing one in a file that has none.
 */
function vscodeSettingLines(home: string): readonly string[] {
  return [
    '"github.copilot.chat.otel.enabled": true,',
    '"github.copilot.chat.otel.exporterType": "file",',
    `"github.copilot.chat.otel.outfile": "${home}/.copilot/otel/vscode-chat.jsonl",`,
    '"github.copilot.chat.otel.captureContent": false,',
    `"${VSCODE_SYNC_IGNORED}": ["github.copilot.chat.otel.outfile"],`,
  ];
}

/**
 * The command that creates the export folder and hands over VS Code's export
 * settings with the machine's home folder filled in, which the page cannot
 * know: on the clipboard where the system has one command for that, printed
 * otherwise. VS Code takes forward slashes on Windows too.
 */
export function copilotSettingsCommand(os: MachineOs): SetupCommand {
  const label = `Create the folder and ${COPILOT_GUIDES[os].copiesSettings ? "copy" : "print"} the settings, with this machine's path filled in`;
  if (os === "windows") {
    return {
      label,
      lines: [
        'New-Item -ItemType Directory -Force "$HOME\\.copilot\\otel" | Out-Null',
        '@"',
        ...vscodeSettingLines(`$("$HOME" -replace '\\\\','/')`),
        '"@ | Set-Clipboard',
      ],
    };
  }
  return {
    label,
    lines: [
      `mkdir -p ~/.copilot/otel && cat <<EOF${os === "macos" ? " | pbcopy" : ""}`,
      ...vscodeSettingLines("$HOME"),
      "EOF",
    ],
  };
}
