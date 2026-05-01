import type { CreateAgentRunRequest, Source, TargetRepo } from "../domain/schemas";
export { gitUrlWithUsername } from "../git/git-auth";

type PrDescriptionRun = {
  readonly source: Source;
};

export type PrDescriptionInput = {
  readonly run: PrDescriptionRun;
  readonly implementationSummary: string;
  readonly validation: ReadonlyArray<{
    readonly command: string;
    readonly exitCode: number;
  }>;
  readonly changedFiles: ReadonlyArray<string>;
};

export type AzureDevOpsDraftPrInput = {
  readonly targetRepo: TargetRepo;
  readonly gitAuthHeader: string;
  readonly sourceBranch: string;
  readonly title: string;
  readonly description: string;
};

export type AzureDevOpsCommitChange = {
  readonly changeType: "add" | "edit" | "delete";
  readonly path: string;
  readonly content?: string;
};

export type AzureDevOpsPushCommitInput = {
  readonly targetRepo: TargetRepo;
  readonly gitAuthHeader: string;
  readonly sourceBranch: string;
  readonly baseObjectId: string;
  readonly comment: string;
  readonly changes: ReadonlyArray<AzureDevOpsCommitChange>;
};

type AzureDevOpsPullRequestResponse = {
  readonly pullRequestId?: number;
  readonly url?: string;
};

const slugify = (value: string): string =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);

export function generateBranchName(run: CreateAgentRunRequest, pattern: string): string {
  const slug = slugify(run.source.title) || "work-item";

  return pattern
    .replaceAll("{sourceType}", run.source.type)
    .replaceAll("{sourceId}", run.source.id)
    .replaceAll("{slug}", slug);
}

export function buildDraftPrDescription(input: PrDescriptionInput): string {
  const summaryLines = formatSummaryBullets(input.implementationSummary);
  const changedFileLines = formatImportantFileBullets(input.changedFiles);
  const validationLines = formatValidationBullets(input.validation);

  return `## Summary

${summaryLines || "- Implementation completed."}

## Important Files

${changedFileLines || "- No changed files captured."}

## Why

${formatSourceLink(input.run.source)}

## Validation

${validationLines || "- Not run"}`;
}

const formatSourceLink = (source: Source): string =>
  `- [${source.title}](${encodeURI(source.url)})`;

const formatSummaryBullets = (summary: string): string => {
  const focusedSummary = extractSummarySection(summary);

  return focusedSummary
    .split("\n")
    .map((line) => line.trim().replace(/^[-*]\s+/, ""))
    .filter((line) => line.length > 0)
    .slice(0, 4)
    .map((line) => `- ${line}`)
    .join("\n");
};

const extractSummarySection = (summary: string): string => {
  const lines = summary.split("\n");
  const summaryStart = lines.findIndex((line) => /^summary\s*:?$/i.test(line.trim()));

  if (summaryStart === -1) {
    return summary;
  }

  const sectionLines: string[] = [];

  for (const line of lines.slice(summaryStart + 1)) {
    const trimmed = line.trim();

    if (/^[A-Z][A-Za-z ]+\s*:?$/.test(trimmed) && !/^[-*]\s+/.test(trimmed)) {
      break;
    }

    if (trimmed.length === 0 && sectionLines.length > 0) {
      break;
    }

    if (trimmed.length > 0) {
      sectionLines.push(line);
    }
  }

  return sectionLines.length > 0 ? sectionLines.join("\n") : summary;
};

const formatImportantFileBullets = (changedFiles: ReadonlyArray<string>): string =>
  changedFiles
    .map((file) => file.trim())
    .filter((file) => file.length > 0)
    .map((file, index) => ({ file, index, rank: rankChangedFile(file) }))
    .sort((left, right) => left.rank - right.rank || left.index - right.index)
    .slice(0, 5)
    .map(({ file }) => `- \`${file}\``)
    .join("\n");

const rankChangedFile = (file: string): number => {
  if (/(^|\/)(bun\.lock|Cargo\.lock|package-lock\.json|pnpm-lock\.yaml)$/.test(file)) {
    return 4;
  }

  if (/(^|\/)(routeTree\.gen\.ts|\.sqlx\/)/.test(file)) {
    return 5;
  }

  if (/(^|\/)(test|tests|spec|__tests__)\//.test(file) || /\.(test|spec)\.[cm]?[jt]sx?$/.test(file)) {
    return 1;
  }

  if (/(^|\/)(src|app|lib|crates)\//.test(file)) {
    return 0;
  }

  if (/(^|\/)(package\.json|Cargo\.toml|justfile|Justfile|tsconfig\.json)$/.test(file)) {
    return 2;
  }

  if (/(^|\/)(README|CHANGELOG|AGENTS)\.md$/.test(file) || file.startsWith(".docs/")) {
    return 3;
  }

  return 2;
};

const formatValidationBullets = (
  validation: ReadonlyArray<{
    readonly command: string;
    readonly exitCode: number;
  }>,
): string =>
  validation
    .slice(0, 8)
    .map((result) => {
      const status = result.exitCode === 0 ? "passed" : `failed, exit ${result.exitCode}`;
      return `- \`${result.command}\`: ${status}`;
    })
    .join("\n");

export function buildAzureDevOpsPullRequestUrl(targetRepo: TargetRepo): string {
  return `${buildAzureDevOpsRepositoryApiUrl(targetRepo)}/pullrequests?api-version=7.1`;
}

export function buildAzureDevOpsRefsUrl(targetRepo: TargetRepo): string {
  return `${buildAzureDevOpsRepositoryApiUrl(targetRepo)}/refs?api-version=7.1`;
}

export function buildAzureDevOpsPushesUrl(targetRepo: TargetRepo): string {
  return `${buildAzureDevOpsRepositoryApiUrl(targetRepo)}/pushes?api-version=7.1`;
}

function buildAzureDevOpsRepositoryApiUrl(targetRepo: TargetRepo): string {
  const organization = requiredTargetRepoField(targetRepo.organization, "organization");
  const project = requiredTargetRepoField(targetRepo.project, "project");
  const repoName = requiredTargetRepoField(targetRepo.repoName, "repoName");
  const path = [
    encodeURIComponent(organization),
    encodeURIComponent(project),
    "_apis",
    "git",
    "repositories",
    encodeURIComponent(repoName),
  ].join("/");

  return `https://dev.azure.com/${path}`;
}

export function buildAzureDevOpsPullRequestWebUrl(
  targetRepo: TargetRepo,
  pullRequestId: number,
): string {
  const organization = requiredTargetRepoField(targetRepo.organization, "organization");
  const project = requiredTargetRepoField(targetRepo.project, "project");
  const repoName = requiredTargetRepoField(targetRepo.repoName, "repoName");

  return [
    "https://dev.azure.com",
    encodeURIComponent(organization),
    encodeURIComponent(project),
    "_git",
    encodeURIComponent(repoName),
    "pullrequest",
    String(pullRequestId),
  ].join("/");
}

export function buildAzureDevOpsPullRequestBody(input: AzureDevOpsDraftPrInput) {
  return {
    sourceRefName: `refs/heads/${input.sourceBranch}`,
    targetRefName: `refs/heads/${input.targetRepo.defaultBranch}`,
    title: input.title,
    description: input.description,
    isDraft: true,
  };
}

export function buildAzureDevOpsCreateBranchBody(input: AzureDevOpsPushCommitInput) {
  return [
    {
      name: `refs/heads/${input.sourceBranch}`,
      oldObjectId: "0000000000000000000000000000000000000000",
      newObjectId: input.baseObjectId,
    },
  ];
}

export function buildAzureDevOpsPushCommitBody(input: AzureDevOpsPushCommitInput) {
  return {
    refUpdates: [
      {
        name: `refs/heads/${input.sourceBranch}`,
        oldObjectId: input.baseObjectId,
      },
    ],
    commits: [
      {
        comment: input.comment,
        changes: input.changes.map((change) => {
          if (change.changeType === "delete") {
            return {
              changeType: "delete",
              item: { path: `/${change.path}` },
            };
          }

          return {
            changeType: change.changeType,
            item: { path: `/${change.path}` },
            newContent: {
              content: change.content ?? "",
              contentType: "rawtext",
            },
          };
        }),
      },
    ],
  };
}

export async function createAzureDevOpsDraftPr(
  input: AzureDevOpsDraftPrInput,
): Promise<string> {
  if (input.targetRepo.provider !== "azureDevOps") {
    throw new Error(`Draft PR publishing is not implemented for ${input.targetRepo.provider}.`);
  }

  const response = await fetch(buildAzureDevOpsPullRequestUrl(input.targetRepo), {
    method: "POST",
    headers: {
      authorization: input.gitAuthHeader,
      "content-type": "application/json",
    },
    body: JSON.stringify(buildAzureDevOpsPullRequestBody(input)),
  });
  const responseText = await response.text();

  if (!response.ok) {
    throw new Error(`Azure DevOps draft PR creation failed with ${response.status}: ${responseText}`);
  }

  const body = JSON.parse(responseText) as AzureDevOpsPullRequestResponse;

  if (typeof body.pullRequestId === "number") {
    return buildAzureDevOpsPullRequestWebUrl(input.targetRepo, body.pullRequestId);
  }

  if (typeof body.url === "string") {
    return body.url;
  }

  throw new Error("Azure DevOps draft PR response did not include a PR URL.");
}

export async function pushAzureDevOpsCommit(input: AzureDevOpsPushCommitInput): Promise<void> {
  if (input.targetRepo.provider !== "azureDevOps") {
    throw new Error(`REST commit publishing is not implemented for ${input.targetRepo.provider}.`);
  }

  await azureDevOpsJsonRequest(
    buildAzureDevOpsRefsUrl(input.targetRepo),
    input.gitAuthHeader,
    buildAzureDevOpsCreateBranchBody(input),
    "Azure DevOps branch creation",
  );
  await azureDevOpsJsonRequest(
    buildAzureDevOpsPushesUrl(input.targetRepo),
    input.gitAuthHeader,
    buildAzureDevOpsPushCommitBody(input),
    "Azure DevOps commit push",
  );
}

const azureDevOpsJsonRequest = async (
  url: string,
  gitAuthHeader: string,
  body: unknown,
  operation: string,
): Promise<void> => {
  const response = await fetch(url, {
    method: "POST",
    headers: {
      authorization: gitAuthHeader,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  const responseText = await response.text();

  if (!response.ok) {
    throw new Error(`${operation} failed with ${response.status}: ${responseText}`);
  }
};

const requiredTargetRepoField = (value: string | undefined, field: string): string => {
  if (value === undefined || value.length === 0) {
    throw new Error(`Azure DevOps target repo is missing ${field}.`);
  }

  return value;
};
