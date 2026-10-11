import type { AiProvider } from "@/lib/api/contracts/ai-usage";

/** Display names of the tools whose usage token-ledger uploads. */
export const AI_PROVIDER_LABELS: Readonly<Record<AiProvider, string>> = {
  codex: "Codex",
  claude: "Claude Code",
  grok: "Grok Build",
  copilot: "Copilot",
};
