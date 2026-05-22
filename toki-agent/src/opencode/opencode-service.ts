import type { AgentRunRecord } from "../domain/schemas";
import type { SandboxService } from "../sandbox/sandbox-service";
import { redactLog } from "../security/redact";
import type { AgentWorkflow } from "../workflow/load-agent-workflow";
import { renderWorkflowPrompt } from "../workflow/render-prompts";

export type OpenCodePlanResult = {
  readonly markdown: string;
};

export interface OpenCodeService {
  readonly generatePlan: (prompt: string) => Promise<OpenCodePlanResult>;
  readonly implementPlan: (prompt: string) => Promise<{ readonly summary: string }>;
}

export const buildPlanPrompt = (workflow: AgentWorkflow, run: AgentRunRecord): string =>
  renderWorkflowPrompt(workflow, {
    mode: "planFirst",
    source: run.source,
    targetRepo: run.targetRepo,
    actor: run.actor,
    prompt: run.prompt,
  });

export const createOpenCodeService = ({
  sandbox,
  workspaceDir,
  maxTurns,
  model,
  variant,
  openaiApiKey,
  geminiApiKey,
  onProgress,
}: {
  readonly sandbox: SandboxService;
  readonly workspaceDir: string;
  readonly maxTurns: number;
  readonly model?: string;
  readonly variant?: string;
  readonly openaiApiKey?: string;
  readonly geminiApiKey?: string;
  readonly onProgress?: (message: string) => Promise<void>;
}): OpenCodeService => ({
  generatePlan: async (prompt) => {
    await sandbox.prepareOpenCodeAuth();
    const reportProgress = createOpenCodeProgressReporter(onProgress);
    const result = await sandbox.execLong(
      [
        "HOME=/root",
        "PATH=/root/.opencode/bin:/usr/local/bin:/usr/bin:/bin:$PATH",
        "OPENCODE_DISABLE_AUTOUPDATE=true",
        "OPENCODE_DISABLE_PRUNE=true",
        openaiApiKeyEnv(openaiApiKey),
        geminiApiKeyEnv(geminiApiKey),
        "timeout",
        "--kill-after=10s",
        "300s",
        "/root/.opencode/bin/opencode",
        "run",
        "-m",
        shellQuote(model ?? "openai/gpt-5.4"),
        opencodeVariantFlag(variant),
        shellQuote(addPlanConstraints(prompt, maxTurns)),
      ].join(" "),
      {
        cwd: workspaceDir,
        timeoutMs: 10 * 60 * 1000,
        onLogChunk: reportProgress,
      },
    );

    if (result.exitCode !== 0) {
      throw new Error(describeCommandFailure("OpenCode plan generation failed", result));
    }

    const markdown = extractPlanMarkdown(result.stdout);

    if (markdown.length === 0) {
      throw new Error(describeCommandFailure("OpenCode plan generation returned an empty plan", result));
    }

    return {
      markdown,
    };
  },
  implementPlan: async (prompt) => {
    await sandbox.prepareOpenCodeAuth();
    const reportProgress = createOpenCodeProgressReporter(onProgress);
    const result = await sandbox.execLong(
      [
        "HOME=/root",
        "PATH=/root/.opencode/bin:/usr/local/bin:/usr/bin:/bin:$PATH",
        "OPENCODE_DISABLE_AUTOUPDATE=true",
        "OPENCODE_DISABLE_PRUNE=true",
        openaiApiKeyEnv(openaiApiKey),
        geminiApiKeyEnv(geminiApiKey),
        "timeout",
        "--kill-after=10s",
        "900s",
        "/root/.opencode/bin/opencode",
        "run",
        "-m",
        shellQuote(model ?? "openai/gpt-5.4"),
        opencodeVariantFlag(variant),
        shellQuote(prompt),
      ].join(" "),
      {
        cwd: workspaceDir,
        timeoutMs: 16 * 60 * 1000,
        onLogChunk: reportProgress,
      },
    );

    if (result.exitCode !== 0) {
      throw new Error(describeCommandFailure("OpenCode implementation failed", result));
    }

    const summary = result.stdout.trim();

    if (summary.length === 0 && result.stderr.trim().length > 0) {
      throw new Error(describeCommandFailure("OpenCode implementation returned no summary", result));
    }

    return {
      summary,
    };
  },
});

const createOpenCodeProgressReporter = (
  onProgress: ((message: string) => Promise<void>) | undefined,
): ((chunk: { readonly stdout: string; readonly stderr: string }) => Promise<void>) | undefined => {
  if (onProgress === undefined) {
    return undefined;
  }

  let lastMessage = "";

  return async (chunk) => {
    const message = extractOpenCodeProgressMessage(chunk);

    if (message === undefined || message === lastMessage) {
      return;
    }

    lastMessage = message;
    await onProgress(`OpenCode progress: ${message}`);
  };
};

const extractOpenCodeProgressMessage = (
  chunk: { readonly stdout: string; readonly stderr: string },
): string | undefined => {
  const lines = `${chunk.stderr}\n${chunk.stdout}`
    .split(/\r?\n/)
    .map((line) => sanitizeProgressLine(line))
    .filter((line): line is string => line !== undefined);

  return lines.at(-1);
};

const sanitizeProgressLine = (line: string): string | undefined => {
  const cleaned = redactLog(line)
    .replace(ansiEscapePattern, "")
    .replaceAll("\r", "")
    .trim();

  if (
    cleaned.length === 0 ||
    cleaned === ">" ||
    cleaned.startsWith("> ") ||
    cleaned === "sqlite-migration:done" ||
    cleaned === "Database migration complete." ||
    cleaned.startsWith("Performing one time database migration") ||
    cleaned.startsWith("at ") ||
    cleaned.includes("~effect/Effect/")
  ) {
    return undefined;
  }

  return cleaned.length > 300 ? `${cleaned.slice(0, 297)}...` : cleaned;
};

// biome-ignore lint/complexity/useRegexLiterals: literal form is flagged as a control-character regex.
const ansiEscapePattern = new RegExp(String.raw`\x1B\[[0-?]*[ -/]*[@-~]`, "g");

const geminiApiKeyEnv = (geminiApiKey: string | undefined): string =>
  geminiApiKey === undefined || geminiApiKey.trim().length === 0
    ? ""
    : [
        `GEMINI_API_KEY=${shellQuote(geminiApiKey)}`,
        `GOOGLE_API_KEY=${shellQuote(geminiApiKey)}`,
        `GOOGLE_GENERATIVE_AI_API_KEY=${shellQuote(geminiApiKey)}`,
      ].join(" ");

const opencodeVariantFlag = (variant: string | undefined): string =>
  variant === undefined || variant.trim().length === 0 ? "" : `--variant ${shellQuote(variant)}`;

export const buildImplementationPrompt = (run: AgentRunRecord): string =>
  [
    "Implement the approved plan for this Toki board item.",
    "",
    "Issue:",
    `${run.source.id}: ${run.source.title}`,
    run.source.url,
    "",
    "Approved plan:",
    run.workpad.currentPlanMarkdown,
    "",
    "Rules:",
    "- Make the smallest safe change.",
    "- Follow repository conventions.",
    "- Do not run project validation, build, lint, or test commands; the orchestrator runs verification after you exit.",
    "- Do not create or merge a PR from inside OpenCode; the orchestrator owns publishing.",
    "- Return a concise summary immediately after the code edits are complete.",
  ].join("\n");

export const buildVerificationRepairPrompt = ({
  run,
  command,
  stdout,
  stderr,
  attempt,
  maxAttempts,
}: {
  readonly run: AgentRunRecord;
  readonly command: string;
  readonly stdout: string;
  readonly stderr: string;
  readonly attempt: number;
  readonly maxAttempts: number;
}): string =>
  [
    "Repair the implementation for this Toki board item after orchestrator verification failed.",
    "",
    "Issue:",
    `${run.source.id}: ${run.source.title}`,
    run.source.url,
    "",
    "Approved plan:",
    run.workpad.currentPlanMarkdown,
    "",
    `Repair attempt: ${attempt}/${maxAttempts}`,
    "",
    "Failed verification command:",
    command,
    "",
    "stderr:",
    stderr.trim() || "(empty)",
    "",
    "stdout:",
    stdout.trim() || "(empty)",
    "",
    "Rules:",
    "- Fix the build, lint, or test failure with the smallest safe change.",
    "- Do not broaden the original scope unless the failure proves it is necessary.",
    "- Do not run project validation, build, lint, or test commands; the orchestrator reruns verification after you exit.",
    "- Do not create or merge a PR from inside OpenCode; the orchestrator owns publishing.",
    "- Return a concise summary immediately after the repair edits are complete.",
  ].join("\n");

const addPlanConstraints = (prompt: string, maxTurns: number): string =>
  [
    prompt,
    "",
    "Output only the implementation plan in Markdown.",
    "Start the response with exactly `**Plan**`.",
    "Do not include progress updates, context-gathering notes, or commentary before the plan.",
    `Keep the planning exchange bounded to at most ${maxTurns} agent turns.`,
    "Do not edit files while planning.",
  ].join("\n");

export const extractPlanMarkdown = (value: string): string => {
  const trimmed = value.trim();
  const planHeadingMatch = /(?:^|\n)(?:#{1,3}\s+Plan\b|\*\*Plan\*\*)/i.exec(trimmed);

  if (planHeadingMatch === null) {
    return trimmed;
  }

  return trimmed.slice(planHeadingMatch.index).trim();
};

const shellQuote = (value: string): string => `'${value.replaceAll("'", "'\\''")}'`;

const openaiApiKeyEnv = (openaiApiKey: string | undefined): string =>
  openaiApiKey === undefined || openaiApiKey.trim().length === 0
    ? ""
    : `OPENAI_API_KEY=${shellQuote(openaiApiKey)}`;

const describeCommandFailure = (
  summary: string,
  result: { readonly exitCode: number; readonly stdout: string; readonly stderr: string },
): string =>
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
