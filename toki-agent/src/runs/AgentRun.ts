import { DurableObject } from "cloudflare:workers";
import type { Env } from "../env";
import type {
  Actor,
  AgentRunEvent,
  AgentRunRecord,
  BackendPublishPayload,
  CompletePublishRequest,
  CreateAgentRunRequest,
  CreateTargetRepo,
  FailPublishRequest,
  FeedbackRequest,
  TargetRepo,
} from "../domain/schemas";
import type { AgentRunStatus } from "../domain/status";
import { canTransitionStatus, isTerminalStatus } from "../domain/status";
import {
  buildImplementationPrompt,
  buildPlanPrompt,
  buildVerificationRepairPrompt,
  createOpenCodeService,
} from "../opencode/opencode-service";
import {
  type AzureDevOpsCommitChange,
  buildDraftPrDescription,
  generateBranchName,
} from "../publish/publish-service";
import { createSandboxService, type SandboxService } from "../sandbox/sandbox-service";
import { parseAgentWorkflow, type AgentWorkflow } from "../workflow/load-agent-workflow";

const RUN_RECORD_KEY = "run";
const GIT_AUTH_HEADER_KEY = "gitAuthHeader";
const AGENT_WORKFLOW_PATH = ".toki/agent.md";
const MAX_VERIFICATION_REPAIR_ATTEMPTS = 2;

type VerificationResult =
  | {
      readonly record: AgentRunRecord;
      readonly command: string;
      readonly result: { readonly exitCode: number; readonly stdout: string; readonly stderr: string };
    }
  | {
      readonly record: AgentRunRecord;
      readonly result?: undefined;
    };

export class AgentRun extends DurableObject<Env> {
  async createRun(id: string, request: CreateAgentRunRequest): Promise<AgentRunRecord> {
    const existing = await this.ctx.storage.get<AgentRunRecord>(RUN_RECORD_KEY);

    if (existing !== undefined) {
      return existing;
    }

    const now = new Date().toISOString();
    const record: AgentRunRecord = {
      id,
      status: "created",
      mode: "planFirst",
      source: request.source,
      targetRepo: targetRepoWithoutSecrets(request.targetRepo),
      actor: request.actor,
      prompt: request.prompt,
      metadata: {
        model: this.env.OPENCODE_MODEL ?? "openai/gpt-5.4",
        reasoningLevel: this.env.OPENCODE_VARIANT,
      },
      workpad: {
        currentPlanMarkdown: "",
        planVersion: 0,
        feedbackHistory: [],
        acceptanceCriteria: [],
        validationChecklist: [],
        notes: ["Run accepted. Starting sandbox orchestration."],
        risksAndConfusions: [],
      },
      events: [
        makeEvent({
          runId: id,
          status: "created",
          message: "Run created.",
          createdAt: now,
        }),
      ],
      createdAt: now,
      updatedAt: now,
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, record);
    if (request.targetRepo.gitAuthHeader !== undefined) {
      await this.ctx.storage.put(GIT_AUTH_HEADER_KEY, request.targetRepo.gitAuthHeader);
    }
    return record;
  }

  async runPlanningStep(): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    try {
      if (
        record === undefined ||
        isTerminalStatus(record.status) ||
        record.status === "awaitingPlanFeedback" ||
        record.status === "planApproved"
      ) {
        return record;
      }

      const gitAuthHeader = await this.ctx.storage.get<string>(GIT_AUTH_HEADER_KEY);
      await this.runPlanningFlow(record, gitAuthHeader);
    } catch (error) {
      if (record !== undefined) {
        await this.failRun(record.id, readableError(error));
      }
    }

    return await this.getRun();
  }

  async runRevisionStep(): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    try {
      if (record === undefined || isTerminalStatus(record.status) || record.status !== "revisingPlan") {
        return record;
      }

      await this.runRevisionFlow(record);
    } catch (error) {
      if (record !== undefined) {
        await this.failRun(record.id, readableError(error));
      }
    }

    return await this.getRun();
  }

  async runImplementationStep(): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    try {
      if (record === undefined || isTerminalStatus(record.status) || record.status !== "planApproved") {
        return record;
      }

      await this.runImplementationFlow(record);
    } catch (error) {
      if (record !== undefined) {
        await this.failRun(record.id, readableError(error));
      }
    }

    return await this.getRun();
  }

  async recordWorkflowStartFailure(message: string): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || isTerminalStatus(record.status)) {
      return record;
    }

    await this.failRun(record.id, message);
    return await this.getRun();
  }

  async getRun(): Promise<AgentRunRecord | undefined> {
    return await this.ctx.storage.get<AgentRunRecord>(RUN_RECORD_KEY);
  }

  async getEvents(): Promise<ReadonlyArray<AgentRunEvent>> {
    const record = await this.getRun();
    return record?.events ?? [];
  }

  async deleteRun(): Promise<boolean> {
    const record = await this.getRun();

    if (record === undefined) {
      return false;
    }

    await this.ctx.storage.deleteAll();
    return true;
  }

  async addFeedback(request: FeedbackRequest, actor: Actor): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || isTerminalStatus(record.status)) {
      return record;
    }

    const now = new Date().toISOString();
    const nextStatus = canTransitionStatus(record.status, "revisingPlan")
      ? "revisingPlan"
      : record.status;
    const nextRecord: AgentRunRecord = {
      ...record,
      status: nextStatus,
      workpad: {
        ...record.workpad,
        feedbackHistory: [
          ...record.workpad.feedbackHistory,
          {
            id: crypto.randomUUID(),
            message: request.message,
            actor,
            createdAt: now,
          },
        ],
      },
      events: [
        ...record.events,
        makeEvent({
          runId: record.id,
          status: nextStatus,
          message: "Plan feedback received.",
          createdAt: now,
        }),
      ],
      updatedAt: now,
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  async approvePlan(): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || !canTransitionStatus(record.status, "planApproved")) {
      return record;
    }

    return await this.transition(record, "planApproved", "Plan approved.");
  }

  async cancel(): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || !canTransitionStatus(record.status, "canceled")) {
      return record;
    }

    await this.ctx.storage.delete(GIT_AUTH_HEADER_KEY);
    return await this.transition(record, "canceled", "Run canceled.");
  }

  async claimBackendPublish(): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || record.status !== "awaitingBackendPublish") {
      return record;
    }

    return await this.transition(record, "backendPublishing", "Backend claimed publish handoff.");
  }

  async completeBackendPublish(request: CompletePublishRequest): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || record.status !== "backendPublishing") {
      return record;
    }

    await this.ctx.storage.delete(GIT_AUTH_HEADER_KEY);
    let nextRecord = await this.withDraftPrUrl(record, request.draftPrUrl);
    nextRecord = await this.clearPendingPublish(nextRecord);
    nextRecord = await this.transition(nextRecord, "draftPrCreated", "Draft PR created.");
    return await this.transition(nextRecord, "succeeded", "Agent run succeeded.");
  }

  async failBackendPublish(request: FailPublishRequest): Promise<AgentRunRecord | undefined> {
    const record = await this.getRun();

    if (record === undefined || record.status !== "backendPublishing") {
      return record;
    }

    await this.ctx.storage.delete(GIT_AUTH_HEADER_KEY);
    const nextRecord = await this.clearPendingPublish(record);
    return await this.transition(nextRecord, "failed", request.message);
  }

  private async runPlanningFlow(
    initialRecord: AgentRunRecord,
    gitAuthHeader: string | undefined,
  ): Promise<void> {
    let record = await this.transition(initialRecord, "provisioningSandbox", "Provisioning run sandbox.");
    const sandbox = createSandboxService(this.env, record.id);

    if (sandbox === undefined) {
      throw new Error("Cloudflare Sandbox binding is not configured.");
    }

    await sandbox.prepare();

    record = await this.transition(record, "checkingRepositoryAccess", "Checking target repository access.");
    await sandbox.verifyRepositoryAccess({
      cloneUrl: record.targetRepo.cloneUrl,
      branch: record.targetRepo.defaultBranch,
      gitAuthHeader,
    });

    record = await this.transition(record, "cloningRepository", "Cloning target repository.");
    await sandbox.cloneRepository({
      cloneUrl: record.targetRepo.cloneUrl,
      branch: record.targetRepo.defaultBranch,
      workspaceDir: "/workspace/repo",
      gitAuthHeader,
    });

    record = await this.transition(record, "loadingWorkflow", "Loading repo-owned workflow policy.");
    const workflow = await this.loadWorkflow(record, sandbox);

    record = await this.transition(record, "planning", "Generating implementation plan.");
    let planningRecord = record;
    const opencode = createOpenCodeService({
      sandbox,
      workspaceDir: workflow.config.sandbox.workspaceDir,
      maxTurns: workflow.config.agent.maxTurns,
      model: this.env.OPENCODE_MODEL,
      variant: this.env.OPENCODE_VARIANT,
      openaiApiKey: openAiApiKeyForOpenCode(this.env),
      geminiApiKey: this.env.GEMINI_API_KEY,
      onProgress: async (message) => {
        planningRecord = await this.appendNote(planningRecord, message);
      },
    });
    const plan = await opencode.generatePlan(buildPlanPrompt(workflow, planningRecord));
    await this.applyPlan(planningRecord, plan.markdown, workflow, [
      "OpenCode generated this plan inside the Cloudflare Sandbox.",
    ]);
  }

  private async runRevisionFlow(record: AgentRunRecord): Promise<void> {
    const sandbox = createSandboxService(this.env, record.id);

    if (sandbox === undefined) {
      throw new Error("Cloudflare Sandbox binding is not configured.");
    }

    const workflow = workflowFromSnapshot(record);
    let revisionRecord = record;
    const opencode = createOpenCodeService({
      sandbox,
      workspaceDir: workflow.config.sandbox.workspaceDir,
      maxTurns: workflow.config.agent.maxTurns,
      model: this.env.OPENCODE_MODEL,
      variant: this.env.OPENCODE_VARIANT,
      openaiApiKey: openAiApiKeyForOpenCode(this.env),
      geminiApiKey: this.env.GEMINI_API_KEY,
      onProgress: async (message) => {
        revisionRecord = await this.appendNote(revisionRecord, message);
      },
    });
    const prompt = [
      "Revise the current implementation plan using the latest user feedback.",
      "",
      "Current plan:",
      record.workpad.currentPlanMarkdown,
      "",
      "Feedback history:",
      ...record.workpad.feedbackHistory.map((feedback) => `- ${feedback.message}`),
      "",
      "Original workflow prompt:",
      buildPlanPrompt(workflow, record),
    ].join("\n");
    const plan = await opencode.generatePlan(prompt);
    await this.applyPlanRevision(revisionRecord, plan.markdown, "Plan revised from user feedback.");
  }

  private async runImplementationFlow(record: AgentRunRecord): Promise<void> {
    const sandbox = createSandboxService(this.env, record.id);

    if (sandbox === undefined) {
      throw new Error("Cloudflare Sandbox binding is not configured.");
    }

    const workflow = workflowFromSnapshot(record);
    let nextRecord = await this.transition(record, "implementing", "Running OpenCode implementation.");
    const opencode = createOpenCodeService({
      sandbox,
      workspaceDir: workflow.config.sandbox.workspaceDir,
      maxTurns: workflow.config.agent.maxTurns,
      model: this.env.OPENCODE_MODEL,
      variant: this.env.OPENCODE_VARIANT,
      openaiApiKey: openAiApiKeyForOpenCode(this.env),
      geminiApiKey: this.env.GEMINI_API_KEY,
      onProgress: async (message) => {
        nextRecord = await this.appendNote(nextRecord, message);
      },
    });
    const implementation = await opencode.implementPlan(buildImplementationPrompt(nextRecord));
    const implementationSummaries = [implementation.summary];
    nextRecord = await this.appendNote(nextRecord, `OpenCode summary: ${implementation.summary}`);
    const setupCommands = workflow.config.setup.commands;
    const verificationCommands = workflow.config.verify.commands;

    nextRecord = await this.transition(nextRecord, "verifying", "Preparing verification commands.");
    nextRecord = await this.runSetupCommands(nextRecord, sandbox, workflow, setupCommands);
    if (isTerminalStatus(nextRecord.status)) {
      return;
    }

    const validation: Array<{ command: string; exitCode: number }> = [];
    let repairAttempt = 0;

    while (true) {
      const verificationFailure = await this.runVerificationCommands({
        record: nextRecord,
        sandbox,
        workflow,
        commands: verificationCommands,
        validation,
      });
      nextRecord = verificationFailure.record;

      if (verificationFailure.result === undefined) {
        break;
      }

      const failedCommand = verificationFailure.command;

      if (repairAttempt >= MAX_VERIFICATION_REPAIR_ATTEMPTS) {
        await this.transition(nextRecord, "failed", `Verification failed: ${failedCommand}`);
        return;
      }

      repairAttempt += 1;
      nextRecord = await this.appendNote(
        nextRecord,
        `Verification failed. Asking OpenCode to repair attempt ${repairAttempt}/${MAX_VERIFICATION_REPAIR_ATTEMPTS}.`,
      );
      const repair = await opencode.implementPlan(
        buildVerificationRepairPrompt({
          run: nextRecord,
          command: failedCommand,
          stdout: verificationFailure.result.stdout,
          stderr: verificationFailure.result.stderr,
          attempt: repairAttempt,
          maxAttempts: MAX_VERIFICATION_REPAIR_ATTEMPTS,
        }),
      );
      implementationSummaries.push(repair.summary);
      nextRecord = await this.appendNote(nextRecord, `OpenCode repair summary: ${repair.summary}`);
      nextRecord = await this.runSetupCommands(nextRecord, sandbox, workflow, setupCommands);

      if (isTerminalStatus(nextRecord.status)) {
        return;
      }
    }

    nextRecord = await this.transition(nextRecord, "creatingDraftPr", "Creating draft PR.");
    const gitAuthHeader = await this.ctx.storage.get<string>(GIT_AUTH_HEADER_KEY);

    if (gitAuthHeader === undefined) {
      await this.transition(nextRecord, "failed", "Missing Git publishing credentials.");
      return;
    }

    const branchName = generateBranchName(
      {
        mode: "planFirst",
        source: nextRecord.source,
        targetRepo: nextRecord.targetRepo,
        actor: nextRecord.actor,
        prompt: nextRecord.prompt,
      },
      workflow.config.publish.branchPattern,
    );
    const changedFiles = await prepareGitCommit({
      sandbox,
      workspaceDir: workflow.config.sandbox.workspaceDir,
      branchName,
      title: nextRecord.source.title,
    });
    const fallbackCommit = await collectHeadCommitForRestPush({
      sandbox,
      workspaceDir: workflow.config.sandbox.workspaceDir,
      defaultBranch: nextRecord.targetRepo.defaultBranch,
      title: nextRecord.source.title,
    });
    const description = buildDraftPrDescription({
      run: nextRecord,
      implementationSummary: implementationSummaries.join("\n"),
      validation,
      changedFiles,
    });
    await this.ctx.storage.delete(GIT_AUTH_HEADER_KEY);
    nextRecord = await this.withPendingPublish(nextRecord, {
      branchName,
      baseObjectId: fallbackCommit.baseObjectId,
      title: `Agent: ${nextRecord.source.title}`,
      description,
      changes: fallbackCommit.changes,
    });
    await this.transition(nextRecord, "awaitingBackendPublish", "Validation passed. Waiting for backend publish.");
  }

  private async runVerificationCommands({
    record,
    sandbox,
    workflow,
    commands,
    validation,
  }: {
    readonly record: AgentRunRecord;
    readonly sandbox: SandboxService;
    readonly workflow: AgentWorkflow;
    readonly commands: ReadonlyArray<string>;
    readonly validation: Array<{ command: string; exitCode: number }>;
  }): Promise<VerificationResult> {
    let nextRecord = record;

    for (const command of commands) {
      nextRecord = await this.appendNote(nextRecord, `Running validation command: ${command}`);
      const result = await sandbox.execLong(command, {
        cwd: workflow.config.sandbox.workspaceDir,
        timeoutMs: 10 * 60 * 1000,
      });
      validation.push({ command, exitCode: result.exitCode });

      if (result.exitCode !== 0) {
        nextRecord = await this.appendValidation(
          nextRecord,
          validationFailureItem(command, result),
        );
        return { record: nextRecord, command, result };
      }

      nextRecord = await this.appendValidation(nextRecord, `Passed: ${command}`);
    }

    return { record: nextRecord };
  }

  private async loadWorkflow(
    record: AgentRunRecord,
    sandbox: SandboxService,
  ): Promise<AgentWorkflow> {
    const workflowContent = await sandbox.readTextFile(`/workspace/repo/${AGENT_WORKFLOW_PATH}`);
    const parsedWorkflow = parseAgentWorkflow(workflowContent);
    const workflow =
      parsedWorkflow.source === "default"
        ? await discoverDefaultWorkflow(parsedWorkflow, sandbox)
        : parsedWorkflow;
    await this.appendNote(
      record,
      workflow.source === "repo"
        ? "Loaded workflow policy from `.toki/agent.md`."
        : `No \`.toki/agent.md\` found on the base branch. Using discovered defaults: ${workflow.config.verify.commands.join(", ") || "no verification commands"}.`,
    );
    return workflow;
  }

  private async runSetupCommands(
    record: AgentRunRecord,
    sandbox: SandboxService,
    workflow: AgentWorkflow,
    commands: ReadonlyArray<string>,
  ): Promise<AgentRunRecord> {
    let nextRecord = record;

    for (const command of commands) {
      nextRecord = await this.appendNote(nextRecord, `Running setup command: ${command}`);
      const result = await sandbox.exec(command, {
        cwd: workflow.config.sandbox.workspaceDir,
        timeoutMs: 10 * 60 * 1000,
      });
      nextRecord = await this.appendNote(
        nextRecord,
        `Setup command \`${command}\` exited with ${result.exitCode}.`,
      );

      if (result.exitCode !== 0) {
        return await this.transition(nextRecord, "failed", `Setup failed: ${command}`);
      }
    }

    return nextRecord;
  }

  private async applyPlan(
    record: AgentRunRecord,
    planMarkdown: string,
    workflow: AgentWorkflow,
    notes: ReadonlyArray<string>,
  ): Promise<AgentRunRecord> {
    const now = new Date().toISOString();
    const nextRecord: AgentRunRecord = {
      ...record,
      status: "awaitingPlanFeedback",
      workflowSnapshot: {
        source: workflow.source,
        config: workflow.config,
        promptPolicy: workflow.promptPolicy,
      },
      workpad: {
        ...record.workpad,
        currentPlanMarkdown: planMarkdown,
        planVersion: record.workpad.planVersion + 1,
        validationChecklist: workflow.config.verify.commands.map((command) => `Pending: ${command}`),
        notes: [...record.workpad.notes, ...notes],
      },
      events: [
        ...record.events,
        makeEvent({
          runId: record.id,
          status: "awaitingPlanFeedback",
          message: "Plan is ready for feedback.",
          createdAt: now,
        }),
      ],
      updatedAt: now,
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async applyPlanRevision(
    record: AgentRunRecord,
    planMarkdown: string,
    message: string,
  ): Promise<AgentRunRecord> {
    const now = new Date().toISOString();
    const nextRecord: AgentRunRecord = {
      ...record,
      status: "awaitingPlanFeedback",
      workpad: {
        ...record.workpad,
        currentPlanMarkdown: planMarkdown,
        planVersion: record.workpad.planVersion + 1,
      },
      events: [
        ...record.events,
        makeEvent({
          runId: record.id,
          status: "awaitingPlanFeedback",
          message,
          createdAt: now,
        }),
      ],
      updatedAt: now,
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async appendNote(record: AgentRunRecord, note: string): Promise<AgentRunRecord> {
    const nextRecord: AgentRunRecord = {
      ...record,
      workpad: {
        ...record.workpad,
        notes: [...record.workpad.notes, note],
      },
      updatedAt: new Date().toISOString(),
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async appendValidation(record: AgentRunRecord, item: string): Promise<AgentRunRecord> {
    const nextRecord: AgentRunRecord = {
      ...record,
      workpad: {
        ...record.workpad,
        validationChecklist: [...record.workpad.validationChecklist, item],
      },
      updatedAt: new Date().toISOString(),
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async withDraftPrUrl(
    record: AgentRunRecord,
    draftPrUrl: string,
  ): Promise<AgentRunRecord> {
    const nextRecord: AgentRunRecord = {
      ...record,
      workpad: {
        ...record.workpad,
        draftPrUrl,
        finalSummary: `Draft PR created: ${draftPrUrl}`,
      },
      updatedAt: new Date().toISOString(),
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async withPendingPublish(
    record: AgentRunRecord,
    pendingPublish: BackendPublishPayload,
  ): Promise<AgentRunRecord> {
    const nextRecord: AgentRunRecord = {
      ...record,
      pendingPublish,
      updatedAt: new Date().toISOString(),
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async clearPendingPublish(record: AgentRunRecord): Promise<AgentRunRecord> {
    const { pendingPublish: _pendingPublish, ...recordWithoutPublish } = record;
    const nextRecord: AgentRunRecord = {
      ...recordWithoutPublish,
      updatedAt: new Date().toISOString(),
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }

  private async failRun(runId: string, message: string): Promise<void> {
    const record = await this.getRun();

    if (record === undefined || record.id !== runId || isTerminalStatus(record.status)) {
      return;
    }

    await this.ctx.storage.delete(GIT_AUTH_HEADER_KEY);
    await this.transition(record, "failed", message);
  }

  private async transition(
    record: AgentRunRecord,
    status: AgentRunStatus,
    message: string,
  ): Promise<AgentRunRecord> {
    const now = new Date().toISOString();
    const nextRecord: AgentRunRecord = {
      ...record,
      status,
      events: [
        ...record.events,
        makeEvent({
          runId: record.id,
          status,
          message,
          createdAt: now,
        }),
      ],
      updatedAt: now,
    };

    await this.ctx.storage.put(RUN_RECORD_KEY, nextRecord);
    return nextRecord;
  }
}

const readableError = (error: unknown): string => truncateMessage(error instanceof Error ? error.message : "Agent run failed.");

const truncateMessage = (message: string): string => {
  const limit = 4_000;

  if (message.length <= limit) {
    return message;
  }

  const headLength = 1_500;
  const tailLength = limit - headLength - 80;

  return [
    message.slice(0, headLength),
    `... [truncated ${message.length - headLength - tailLength} chars] ...`,
    message.slice(-tailLength),
  ].join("\n");
};

const targetRepoWithoutSecrets = ({
  gitAuthHeader: _gitAuthHeader,
  ...targetRepo
}: CreateTargetRepo): TargetRepo => targetRepo;

const DISCOVER_DEFAULT_WORKFLOW_COMMAND = `node <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const cwd = process.cwd();
const exists = (relativePath) => fs.existsSync(path.join(cwd, relativePath));
const shellQuote = (value) => "'" + value.replaceAll("'", "'\\''") + "'";
const setup = [];
const verify = [];
let needsPnpm = false;

if (exists("justfile") || exists("Justfile")) {
  verify.push("just check", "just tsc", "just lint");
} else {
  const packageDirs = [
    ".",
    ...fs
      .readdirSync(cwd, { withFileTypes: true })
      .filter(
        (entry) =>
          entry.isDirectory() &&
          !entry.name.startsWith(".") &&
          fs.existsSync(path.join(cwd, entry.name, "package.json")),
      )
      .map((entry) => entry.name),
  ];

  for (const dir of packageDirs) {
    const packagePath = path.join(cwd, dir, "package.json");

    if (!fs.existsSync(packagePath)) {
      continue;
    }

    const packageJson = JSON.parse(fs.readFileSync(packagePath, "utf-8"));
    const scripts = packageJson.scripts ?? {};
    const commandPrefix = dir === "." ? "" : "cd " + shellQuote(dir) + " && ";
    const hasPnpmLock = fs.existsSync(path.join(cwd, dir, "pnpm-lock.yaml"));
    const hasPackageLock = fs.existsSync(path.join(cwd, dir, "package-lock.json"));
    needsPnpm ||= hasPnpmLock;
    const installCommand = hasPnpmLock
      ? "pnpm install --frozen-lockfile"
      : hasPackageLock
        ? "npm ci"
        : "npm install";
    const runner = hasPnpmLock ? "pnpm run" : "npm run";
    const scriptNames = selectPackageVerificationScripts(scripts);

    if (scriptNames.length === 0) {
      continue;
    }

    setup.push(commandPrefix + installCommand);
    verify.push(...scriptNames.map((name) => commandPrefix + runner + " " + shellQuote(name)));
  }

  if (needsPnpm) {
    setup.unshift("npm install -g pnpm@latest");
  }

  const solutionDirs = [
    ".",
    ...fs
      .readdirSync(cwd, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && !entry.name.startsWith("."))
      .map((entry) => entry.name),
  ];

  for (const dir of solutionDirs) {
    const absoluteDir = path.join(cwd, dir);
    const hasSolution = fs
      .readdirSync(absoluteDir, { withFileTypes: true })
      .some((entry) => entry.isFile() && entry.name.endsWith(".sln"));

    if (!hasSolution) {
      continue;
    }

    const commandPrefix = dir === "." ? "" : "cd " + shellQuote(dir) + " && ";
    setup.push(commandPrefix + "DOTNET_CLI_TELEMETRY_OPTOUT=1 /usr/share/dotnet/dotnet restore");
    verify.push(commandPrefix + "DOTNET_CLI_TELEMETRY_OPTOUT=1 /usr/share/dotnet/dotnet build --no-restore");
    break;
  }
}

function selectPackageVerificationScripts(scripts) {
  const checkScript = firstExistingScript(scripts, ["check"]);

  if (checkScript !== undefined) {
    return [checkScript];
  }

  return [
    firstExistingScript(scripts, ["type-check", "typecheck"]),
    firstExistingScript(scripts, ["lint:errors-only", "lint"]),
    firstExistingScript(scripts, ["build"]),
  ].filter(Boolean);
}

function firstExistingScript(scripts, names) {
  return names.find((name) => typeof scripts[name] === "string");
}

console.log(JSON.stringify({ setup: { commands: setup }, verify: { commands: verify } }));
NODE`;

const discoverDefaultWorkflow = async (
  workflow: AgentWorkflow,
  sandbox: SandboxService,
): Promise<AgentWorkflow> => {
  const result = await sandbox.exec(DISCOVER_DEFAULT_WORKFLOW_COMMAND, {
    cwd: workflow.config.sandbox.workspaceDir,
    timeoutMs: 30_000,
  });

  if (result.exitCode !== 0) {
    return workflow;
  }

  const discovered = parseAgentWorkflow(`---
setup:
  commands: []
verify:
  commands: []
---
${workflow.promptPolicy}`);

  try {
    const config = JSON.parse(result.stdout) as {
      readonly setup?: { readonly commands?: ReadonlyArray<string> };
      readonly verify?: { readonly commands?: ReadonlyArray<string> };
    };

    return {
      ...workflow,
      config: {
        ...workflow.config,
        setup: {
          commands: config.setup?.commands ?? discovered.config.setup.commands,
        },
        verify: {
          commands: config.verify?.commands ?? discovered.config.verify.commands,
        },
      },
    };
  } catch {
    return workflow;
  }
};

const workflowFromSnapshot = (record: AgentRunRecord): AgentWorkflow =>
  record.workflowSnapshot === undefined
    ? parseAgentWorkflow(undefined)
    : {
        source: record.workflowSnapshot.source,
        config: record.workflowSnapshot.config,
        promptPolicy: record.workflowSnapshot.promptPolicy,
      };

const openAiApiKeyForOpenCode = (env: Env): string | undefined =>
  env.OPENCODE_ALLOW_OPENAI_API_KEY === "1" ? env.OPENAI_API_KEY : undefined;

const prepareGitCommit = async ({
  sandbox,
  workspaceDir,
  branchName,
  title,
}: {
  readonly sandbox: SandboxService;
  readonly workspaceDir: string;
  readonly branchName: string;
  readonly title: string;
}): Promise<ReadonlyArray<string>> => {
  await mustExec(sandbox, `git checkout -b ${shellQuote(branchName)}`, workspaceDir);
  await mustExec(sandbox, "git config user.name 'Toki Agent'", workspaceDir);
  await mustExec(sandbox, "git config user.email 'agent@toki.local'", workspaceDir);
  await mustExec(sandbox, "git add -A", workspaceDir);
  const hasChanges = await sandbox.exec("git diff --cached --quiet", {
    cwd: workspaceDir,
  });

  if (hasChanges.exitCode === 0) {
    throw new Error("OpenCode did not produce any file changes to publish.");
  }

  const changedFiles = await sandbox.exec("git diff --cached --name-only", {
    cwd: workspaceDir,
  });
  await mustExec(
    sandbox,
    `git commit -m ${shellQuote(`Agent: ${title}`)}`,
    workspaceDir,
  );
  return changedFiles.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
};

const collectHeadCommitForRestPush = async ({
  sandbox,
  workspaceDir,
  defaultBranch,
  title,
}: {
  readonly sandbox: SandboxService;
  readonly workspaceDir: string;
  readonly defaultBranch: string;
  readonly title: string;
}): Promise<{ readonly baseObjectId: string; readonly changes: AzureDevOpsCommitChange[] }> => {
  const baseObjectId = (
    await mustExec(
      sandbox,
      `git rev-parse ${shellQuote(`origin/${defaultBranch}`)}`,
      workspaceDir,
    )
  ).stdout.trim();
  const diff = (
    await mustExec(
      sandbox,
      "git diff --name-status --no-renames HEAD~1 HEAD",
      workspaceDir,
    )
  ).stdout;
  const changes: AzureDevOpsCommitChange[] = [];

  for (const line of diff.split("\n")) {
    const trimmed = line.trim();

    if (trimmed.length === 0) {
      continue;
    }

    const [status, path] = trimmed.split(/\t/);

    if (path === undefined || path.length === 0) {
      throw new Error(`Could not parse changed file line for REST publish: ${trimmed}`);
    }

    if (status === "D") {
      changes.push({ changeType: "delete", path });
      continue;
    }

    const content = (
      await mustExec(sandbox, `git show ${shellQuote(`HEAD:${path}`)}`, workspaceDir)
    ).stdout;
    changes.push({
      changeType: status === "A" ? "add" : "edit",
      path,
      content,
    });
  }

  if (changes.length === 0) {
    throw new Error(`No file changes found in commit for ${title}.`);
  }

  return { baseObjectId, changes };
};

const validationFailureItem = (
  command: string,
  result: { readonly stdout: string; readonly stderr: string },
): string => {
  const output = [result.stderr.trim(), result.stdout.trim()]
    .filter((part) => part.length > 0)
    .join("\n");

  return output.length === 0
    ? `Failed: ${command}`
    : `Failed: ${command}\n${truncateMessage(output)}`;
};

const mustExec = async (
  sandbox: SandboxService,
  command: string,
  cwd: string,
): Promise<{ readonly stdout: string; readonly stderr: string }> => {
  const result = await sandbox.exec(command, {
    cwd,
    timeoutMs: 10 * 60 * 1000,
  });

  if (result.exitCode !== 0) {
    throw new Error(result.stderr || `Command failed: ${command}`);
  }

  return { stdout: result.stdout, stderr: result.stderr };
};

const shellQuote = (value: string): string => `'${value.replaceAll("'", "'\\''")}'`;

const makeEvent = ({
  runId,
  status,
  message,
  createdAt,
}: {
  readonly runId: string;
  readonly status: AgentRunStatus;
  readonly message: string;
  readonly createdAt: string;
}): AgentRunEvent => ({
  id: crypto.randomUUID(),
  runId,
  status,
  message,
  createdAt,
});
