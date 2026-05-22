export const buildGitCloneCommand = ({
  cloneUrl,
  branch,
  workspaceDir,
  askPassPath,
}: {
  readonly cloneUrl: string;
  readonly branch: string;
  readonly workspaceDir: string;
  readonly askPassPath?: string;
}): string =>
  [
    "rm -rf",
    shellQuote(workspaceDir),
    "&&",
    "mkdir -p",
    shellQuote(dirname(workspaceDir)),
    "&&",
    "env",
    "GIT_TERMINAL_PROMPT=0",
    askPassPath === undefined ? "GIT_ASKPASS=/bin/false" : `GIT_ASKPASS=${shellQuote(askPassPath)}`,
    "timeout",
    "300s",
    "git",
    "-c",
    "http.version=HTTP/1.1",
    "clone",
    "--depth",
    "1",
    "--single-branch",
    "--branch",
    shellQuote(branch),
    "--no-tags",
    shellQuote(cloneUrl),
    shellQuote(workspaceDir),
  ].join(" ");

const shellQuote = (value: string): string => `'${value.replaceAll("'", "'\\''")}'`;

const dirname = (path: string): string => {
  const normalized = path.replaceAll(/\/+/g, "/");
  const index = normalized.lastIndexOf("/");

  if (index <= 0) {
    return "/";
  }

  return normalized.slice(0, index);
};
