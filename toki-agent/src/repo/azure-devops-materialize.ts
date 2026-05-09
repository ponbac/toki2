import type { TargetRepo } from "../domain/schemas";
import type { SandboxService } from "../sandbox/sandbox-service";

type MaterializeInput = {
  readonly targetRepo: TargetRepo;
  readonly branch: string;
  readonly gitAuthHeader: string;
  readonly sandbox: SandboxService;
  readonly workspaceDir: string;
};

type InspectInput = {
  readonly targetRepo: TargetRepo;
  readonly branch: string;
  readonly gitAuthHeader: string;
};

type ListItemsResponse = {
  readonly value?: ReadonlyArray<AzureDevOpsItem>;
};

type AzureDevOpsItem = {
  readonly path?: string;
  readonly gitObjectType?: string;
  readonly isFolder?: boolean;
  readonly contentMetadata?: {
    readonly fileName?: string;
    readonly extension?: string;
  };
};

type ItemContentResponse = {
  readonly content?: string;
};

export type MaterializeRepoResult = {
  readonly filesWritten: number;
  readonly filesSkipped: number;
};

export type AzureDevOpsRepoInspection = {
  readonly totalItems: number;
  readonly candidateFiles: number;
  readonly sampledPaths: ReadonlyArray<string>;
  readonly workflowContent: string | undefined;
};

const MAX_FILES = 350;
const MAX_FILE_CHARS = 300_000;
const MAX_TOTAL_CHARS = 4_000_000;

const SKIPPED_DIRS = [
  "/.git/",
  "/node_modules/",
  "/target/",
  "/dist/",
  "/build/",
  "/coverage/",
  "/.next/",
  "/bin/",
  "/obj/",
];

const SKIPPED_EXTENSIONS = new Set([
  ".png",
  ".jpg",
  ".jpeg",
  ".gif",
  ".webp",
  ".ico",
  ".pdf",
  ".zip",
  ".gz",
  ".tar",
  ".7z",
  ".dll",
  ".exe",
  ".pdb",
  ".so",
  ".dylib",
  ".wasm",
  ".lockb",
]);

export async function materializeAzureDevOpsRepo(input: MaterializeInput): Promise<MaterializeRepoResult> {
  if (input.targetRepo.provider !== "azureDevOps") {
    throw new Error(`REST repo materialization is not implemented for ${input.targetRepo.provider}.`);
  }

  const items = await listAzureDevOpsItems(input.targetRepo, input.branch, input.gitAuthHeader);
  const files = items.filter(shouldMaterializeItem).slice(0, MAX_FILES);
  let filesWritten = 0;
  let totalChars = 0;

  for (const item of files) {
    const path = item.path;
    if (path === undefined) {
      continue;
    }

    const content = await fetchAzureDevOpsItemContent(input.targetRepo, input.branch, input.gitAuthHeader, path);
    if (content === undefined || content.length > MAX_FILE_CHARS || totalChars + content.length > MAX_TOTAL_CHARS) {
      continue;
    }

    await input.sandbox.writeTextFile(`${input.workspaceDir}${path}`, content);
    filesWritten += 1;
    totalChars += content.length;
  }

  return {
    filesWritten,
    filesSkipped: Math.max(0, items.length - filesWritten),
  };
}

export async function inspectAzureDevOpsRepo(input: InspectInput): Promise<AzureDevOpsRepoInspection> {
  if (input.targetRepo.provider !== "azureDevOps") {
    throw new Error(`REST repo inspection is not implemented for ${input.targetRepo.provider}.`);
  }

  const items = await listAzureDevOpsItems(input.targetRepo, input.branch, input.gitAuthHeader);
  const files = items.filter(shouldMaterializeItem);
  const workflowPath = files.find((item) => item.path?.toLowerCase() === "/.toki/agent.md")?.path;
  const workflowContent =
    workflowPath === undefined
      ? undefined
      : await fetchAzureDevOpsItemContent(input.targetRepo, input.branch, input.gitAuthHeader, workflowPath);

  return {
    totalItems: items.length,
    candidateFiles: files.length,
    sampledPaths: files
      .slice(0, 80)
      .map((item) => item.path)
      .filter((path): path is string => path !== undefined),
    workflowContent,
  };
}

const listAzureDevOpsItems = async (
  targetRepo: TargetRepo,
  branch: string,
  gitAuthHeader: string,
): Promise<ReadonlyArray<AzureDevOpsItem>> => {
  const response = await fetch(`${azureDevOpsRepoApiBase(targetRepo)}/items?${new URLSearchParams({
    scopePath: "/",
    recursionLevel: "Full",
    includeContentMetadata: "true",
    "versionDescriptor.version": branch,
    "api-version": "7.1",
  })}`, {
    headers: {
      authorization: gitAuthHeader,
    },
  });

  const text = await response.text();
  if (!response.ok) {
    throw new Error(`Azure DevOps item listing failed with ${response.status}: ${text.slice(0, 500)}`);
  }

  const body = JSON.parse(text) as ListItemsResponse;
  return body.value ?? [];
};

const fetchAzureDevOpsItemContent = async (
  targetRepo: TargetRepo,
  branch: string,
  gitAuthHeader: string,
  path: string,
): Promise<string | undefined> => {
  const response = await fetch(`${azureDevOpsRepoApiBase(targetRepo)}/items?${new URLSearchParams({
    path,
    includeContent: "true",
    "versionDescriptor.version": branch,
    "api-version": "7.1",
  })}`, {
    headers: {
      authorization: gitAuthHeader,
      accept: "application/json",
    },
  });

  if (!response.ok) {
    return undefined;
  }

  const body = JSON.parse(await response.text()) as ItemContentResponse;
  return typeof body.content === "string" ? body.content : undefined;
};

const shouldMaterializeItem = (item: AzureDevOpsItem): boolean => {
  if (item.isFolder === true || item.gitObjectType !== "blob" || item.path === undefined) {
    return false;
  }

  const normalized = item.path.toLowerCase();
  if (SKIPPED_DIRS.some((dir) => normalized.includes(dir))) {
    return false;
  }

  const extension = item.contentMetadata?.extension?.toLowerCase() ?? "";
  return !SKIPPED_EXTENSIONS.has(extension);
};

const azureDevOpsRepoApiBase = (targetRepo: TargetRepo): string => {
  const organization = requiredTargetRepoField(targetRepo.organization, "organization");
  const project = requiredTargetRepoField(targetRepo.project, "project");
  const repoName = requiredTargetRepoField(targetRepo.repoName, "repoName");

  return [
    "https://dev.azure.com",
    encodeURIComponent(organization),
    encodeURIComponent(project),
    "_apis",
    "git",
    "repositories",
    encodeURIComponent(repoName),
  ].join("/");
};

const requiredTargetRepoField = (value: string | undefined, field: string): string => {
  if (value === undefined || value.length === 0) {
    throw new Error(`Azure DevOps target repo is missing ${field}.`);
  }

  return value;
};
