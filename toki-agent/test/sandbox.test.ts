import { describe, expect, it } from "vitest";
import { buildGitCloneCommand } from "../src/sandbox/git-clone-command";
import { defaultSandboxSessionId, sandboxIdFromRunId } from "../src/sandbox/sandbox-id";

describe("sandbox id generation", () => {
  it("keeps Cloudflare Sandbox IDs within DNS label limits", () => {
    const id = sandboxIdFromRunId(
      "df368d4fc3f450f2d8e57edac196cd0b56a838ef6ac9a18c5ca12fa932605abd",
    );

    expect(id).toBe("run-df368d4fc3f450f2d8e57edac196cd0b");
    expect(id.length).toBeLessThanOrEqual(63);
  });

  it("matches the Sandbox SDK default session id", () => {
    expect(defaultSandboxSessionId("run-abc123")).toBe("sandbox-run-abc123");
  });
});

describe("git clone command", () => {
  it("disables interactive prompts and bounds command runtime", () => {
    const command = buildGitCloneCommand({
      cloneUrl: "https://toki-agent@dev.azure.com/org/project/_git/repo",
      branch: "main",
      workspaceDir: "/workspace/repo",
      askPassPath: "/tmp/askpass.sh",
    });

    expect(command).toContain("GIT_TERMINAL_PROMPT=0");
    expect(command).toContain("GIT_ASKPASS='/tmp/askpass.sh'");
    expect(command).toContain("timeout 300s git");
    expect(command).toContain("http.version=HTTP/1.1");
    expect(command).toContain("clone --depth 1 --single-branch --branch 'main' --no-tags");
    expect(command).toContain("'https://toki-agent@dev.azure.com/org/project/_git/repo' '/workspace/repo'");
  });
});
