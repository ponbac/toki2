import { getSandbox, type Sandbox } from "@cloudflare/sandbox";
import type { Env } from "../env";
import { basicAuthPassword, GIT_USERNAME, gitAskPassScript, gitUrlWithUsername } from "../git/git-auth";
import { redactLog } from "../security/redact";
import { buildGitCloneCommand } from "./git-clone-command";
import { sandboxIdFromRunId } from "./sandbox-id";

const SANDBOX_STARTUP_RETRY_COUNT = 24;
const SANDBOX_STARTUP_RETRY_DELAY_MS = 5_000;

export type SandboxCommandResult = {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
};

export type SandboxCommandLogChunk = {
  readonly stdout: string;
  readonly stderr: string;
};

type SandboxCommandOptions = {
  readonly cwd?: string;
  readonly timeoutMs?: number;
  readonly onLogChunk?: (chunk: SandboxCommandLogChunk) => Promise<void>;
};

export interface SandboxService {
  readonly prepare: () => Promise<void>;
  readonly verifyRepositoryAccess: (input: {
    readonly cloneUrl: string;
    readonly branch: string;
    readonly gitAuthHeader?: string;
  }) => Promise<void>;
  readonly cloneRepository: (input: {
    readonly cloneUrl: string;
    readonly branch: string;
    readonly workspaceDir: string;
    readonly gitAuthHeader?: string;
  }) => Promise<void>;
  readonly readTextFile: (path: string) => Promise<string | undefined>;
  readonly writeTextFile: (path: string, content: string) => Promise<void>;
  readonly exec: (
    command: string,
    options?: { readonly cwd?: string; readonly timeoutMs?: number },
  ) => Promise<SandboxCommandResult>;
  readonly execLong: (
    command: string,
    options?: SandboxCommandOptions,
  ) => Promise<SandboxCommandResult>;
  readonly prepareOpenCodeAuth: () => Promise<void>;
}

export const createSandboxService = (env: Env, runId: string): SandboxService | undefined => {
  if (env.SANDBOX_V2 === undefined) {
    return undefined;
  }

  const sandboxId = sandboxIdFromRunId(runId);
  const sandbox = getSandbox(env.SANDBOX_V2, sandboxId, {
    normalizeId: true,
    sleepAfter: "20m",
    containerTimeouts: {
      instanceGetTimeoutMS: 120_000,
      portReadyTimeoutMS: 180_000,
    },
  });

  const opencodeAuthJson = env.OPENCODE_AUTH_JSON;

  return createCloudflareSandboxService(sandbox, opencodeAuthJson);
};

const createCloudflareSandboxService = (
  sandbox: Sandbox,
  opencodeAuthJson: string | undefined,
): SandboxService => ({
  prepare: async () => {
    const result = await runSandboxCommand(sandbox, "pwd", { timeoutMs: 30_000 });

    if (result.exitCode !== 0) {
      throw new Error(describeCommandFailure("Sandbox startup check failed", result));
    }
  },
  verifyRepositoryAccess: async ({ cloneUrl, branch, gitAuthHeader }) => {
    if (gitAuthHeader === undefined) {
      return;
    }

    const gitAuth = await prepareSandboxGitAuth(sandbox, cloneUrl, gitAuthHeader);

    try {
      const result = await runSandboxCommand(
        sandbox,
        [
          "env",
          "GIT_TERMINAL_PROMPT=0",
          gitAuth.askPassPath === undefined ? "GIT_ASKPASS=/bin/false" : `GIT_ASKPASS=${shellQuote(gitAuth.askPassPath)}`,
          "timeout",
          "30s",
          "git",
          "-c",
          "http.version=HTTP/1.1",
          "ls-remote",
          "--heads",
          shellQuote(gitAuth.cloneUrl),
          shellQuote(branch),
        ].join(" "),
        {
          timeoutMs: 60_000,
        },
      );

      if (result.exitCode !== 0) {
        throw new Error(describeCommandFailure("Repository access check failed", result));
      }
    } finally {
      await cleanupSandboxGitAuth(sandbox, gitAuth);
    }
  },
  cloneRepository: async ({ cloneUrl, branch, workspaceDir, gitAuthHeader }) => {
    if (gitAuthHeader === undefined) {
      await withSandboxStartupRetry(() =>
        sandbox.gitCheckout(cloneUrl, {
          branch,
          targetDir: workspaceDir,
          depth: 1,
          cloneTimeoutMs: 5 * 60 * 1000,
        }),
      );
      return;
    }

    const gitAuth = await prepareSandboxGitAuth(sandbox, cloneUrl, gitAuthHeader);

    try {
      const result = await runSandboxProcess(
        sandbox,
        buildGitCloneCommand({
          cloneUrl: gitAuth.cloneUrl,
          branch,
          workspaceDir,
          askPassPath: gitAuth.askPassPath,
        }),
        {
          timeoutMs: 10 * 60 * 1000,
        },
      );

      if (result.exitCode !== 0) {
        throw new Error(describeCommandFailure("Repository clone failed", result));
      }
    } finally {
      await cleanupSandboxGitAuth(sandbox, gitAuth);
    }
  },
  readTextFile: async (path) =>
    await withSandboxStartupRetry(() => sandbox.readFile(path, { encoding: "utf-8" }))
      .then((file) => file.content)
      .catch(() => undefined),
  writeTextFile: async (path, content) => {
    await withSandboxStartupRetry(() => sandbox.mkdir(dirname(path), { recursive: true }));
    await withSandboxStartupRetry(() => sandbox.writeFile(path, content, { encoding: "utf-8" }));
  },
  exec: async (command, options) => await runSandboxCommand(sandbox, command, options),
  execLong: async (command, options) => await runSandboxProcess(sandbox, command, options),
  prepareOpenCodeAuth: async () => {
    if (opencodeAuthJson === undefined || opencodeAuthJson.trim().length === 0) {
      await runSandboxCommand(sandbox, "rm -f /root/.local/share/opencode/auth.json", {
        timeoutMs: 30_000,
      });
      return;
    }

    await sandbox.mkdir("/root/.local/share/opencode", { recursive: true });
    await runSandboxCommand(sandbox, "chmod 700 /root/.local/share/opencode", {
      timeoutMs: 30_000,
    });
    await sandbox.writeFile("/root/.local/share/opencode/auth.json", opencodeAuthJson, { encoding: "utf-8" });
    await runSandboxCommand(sandbox, "chmod 600 /root/.local/share/opencode/auth.json", {
      timeoutMs: 30_000,
    });
  },
});

const runSandboxCommand = async (
  sandbox: Sandbox,
  command: string,
  options?: { readonly cwd?: string; readonly timeoutMs?: number },
): Promise<SandboxCommandResult> => {
  const result = await withSandboxStartupRetry(() =>
    sandbox.exec(command, {
      cwd: options?.cwd,
      timeout: options?.timeoutMs ?? 30_000,
    }),
  );

  return {
    exitCode: result.exitCode,
    stdout: result.stdout,
    stderr: result.stderr,
  };
};

const runSandboxProcess = async (
  sandbox: Sandbox,
  command: string,
  options?: SandboxCommandOptions,
): Promise<SandboxCommandResult> => {
  const timeoutMs = options?.timeoutMs ?? 20 * 60 * 1000;
  const process = await withSandboxStartupRetry(() =>
    sandbox.startProcess(command, {
      cwd: options?.cwd,
      timeout: timeoutMs,
      processId: `toki-${crypto.randomUUID()}`,
      autoCleanup: false,
    }),
  );
  let stdoutOffset = 0;
  let stderrOffset = 0;
  let done = false;

  const emitLogChunk = async (): Promise<void> => {
    if (options?.onLogChunk === undefined) {
      return;
    }

    const logs = await process.getLogs();
    const stdout = logs.stdout.slice(stdoutOffset);
    const stderr = logs.stderr.slice(stderrOffset);
    stdoutOffset = logs.stdout.length;
    stderrOffset = logs.stderr.length;

    if (stdout.length === 0 && stderr.length === 0) {
      return;
    }

    await options.onLogChunk({ stdout, stderr });
  };

  const logPolling = (async () => {
    if (options?.onLogChunk === undefined) {
      return;
    }

    while (!done) {
      await sleep(5_000);

      if (!done) {
        await emitLogChunk().catch(() => undefined);
      }
    }
  })();

  try {
    const exit = await process.waitForExit(timeoutMs + 5_000);
    done = true;
    await logPolling.catch(() => undefined);
    await emitLogChunk().catch(() => undefined);
    const logs = await process.getLogs();

    return {
      exitCode: exit.exitCode,
      stdout: logs.stdout,
      stderr: logs.stderr,
    };
  } catch (error) {
    done = true;
    await logPolling.catch(() => undefined);
    await process.kill().catch(() => undefined);
    await emitLogChunk().catch(() => undefined);
    const logs = await process.getLogs().catch(() => ({ stdout: "", stderr: "" }));

    return {
      exitCode: 124,
      stdout: logs.stdout,
      stderr: [
        logs.stderr,
        error instanceof Error ? error.message : undefined,
        `Background command timed out after ${timeoutMs}ms.`,
      ]
        .filter((part): part is string => part !== undefined && part.length > 0)
        .join("\n"),
    };
  } finally {
    done = true;
    await sandbox.cleanupCompletedProcesses().catch(() => undefined);
  }
};

const describeCommandFailure = (summary: string, result: SandboxCommandResult): string =>
  redactLog(
    [
      `${summary}.`,
      `exitCode=${result.exitCode}`,
      result.stderr.trim().length > 0 ? `stderr:\n${result.stderr.trim()}` : undefined,
      result.stdout.trim().length > 0 ? `stdout:\n${result.stdout.trim()}` : undefined,
    ]
      .filter((part): part is string => part !== undefined)
      .join("\n"),
  );

const shellQuote = (value: string): string => `'${value.replaceAll("'", "'\\''")}'`;

type SandboxGitAuth = {
  readonly cloneUrl: string;
  readonly askPassPath?: string;
};

const prepareSandboxGitAuth = async (
  sandbox: Sandbox,
  cloneUrl: string,
  gitAuthHeader: string,
): Promise<SandboxGitAuth> => {
  const password = basicAuthPassword(gitAuthHeader);
  const authenticatedCloneUrl =
    password === undefined ? undefined : gitUrlWithUsername(cloneUrl, GIT_USERNAME);

  if (password === undefined || authenticatedCloneUrl === undefined) {
    return { cloneUrl };
  }

  const askPassPath = `/tmp/toki-git-askpass-${crypto.randomUUID()}.sh`;
  await withSandboxStartupRetry(() => sandbox.writeFile(askPassPath, gitAskPassScript(password), { encoding: "utf-8" }));
  const chmodResult = await runSandboxCommand(sandbox, `chmod 700 ${shellQuote(askPassPath)}`, {
    timeoutMs: 30_000,
  });

  if (chmodResult.exitCode !== 0) {
    throw new Error(describeCommandFailure("Git askpass setup failed", chmodResult));
  }

  return { cloneUrl: authenticatedCloneUrl, askPassPath };
};

const cleanupSandboxGitAuth = async (sandbox: Sandbox, gitAuth: SandboxGitAuth): Promise<void> => {
  if (gitAuth.askPassPath === undefined) {
    return;
  }

  await runSandboxCommand(sandbox, `rm -f ${shellQuote(gitAuth.askPassPath)}`, {
    timeoutMs: 30_000,
  }).catch(() => undefined);
};

const withSandboxStartupRetry = async <A>(operation: () => Promise<A>): Promise<A> => {
  let lastError: unknown;

  for (let attempt = 0; attempt <= SANDBOX_STARTUP_RETRY_COUNT; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;

      if (!isTransientSandboxStartupError(error) || attempt === SANDBOX_STARTUP_RETRY_COUNT) {
        throw error;
      }

      await sleep(SANDBOX_STARTUP_RETRY_DELAY_MS);
    }
  }

  throw lastError;
};

const isTransientSandboxStartupError = (error: unknown): boolean => {
  const message = error instanceof Error ? error.message : String(error);

  return [
    "Container is starting",
    "currently provisioning",
    "retry in a moment",
    "Connection refused",
    "container port not found",
    "Service Unavailable",
  ].some((fragment) => message.includes(fragment));
};

const sleep = async (ms: number): Promise<void> => {
  await new Promise((resolve) => setTimeout(resolve, ms));
};

const dirname = (path: string): string => {
  const normalized = path.replaceAll(/\/+/g, "/");
  const index = normalized.lastIndexOf("/");

  if (index <= 0) {
    return "/";
  }

  return normalized.slice(0, index);
};
