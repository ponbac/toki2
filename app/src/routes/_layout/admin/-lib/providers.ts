import type { AiProvider } from "@/lib/api/queries/ai-usage";

/**
 * Chart colors per provider. Color follows the provider, never its rank, so a
 * filter never repaints the others. The order is the validated categorical
 * order (slots 1–4, checked for color-vision deficiency against the app's card
 * surfaces in both themes); stacks draw providers in this order.
 */
export const PROVIDER_COLOR_VARS: Record<AiProvider, string> = {
  codex: "var(--ai-provider-codex)",
  claude: "var(--ai-provider-claude)",
  grok: "var(--ai-provider-grok)",
  copilot: "var(--ai-provider-copilot)",
};

/** Tailwind classes that define the provider colors for light and dark. */
export const PROVIDER_COLOR_SCOPE = [
  "[--ai-provider-codex:#2a78d6] dark:[--ai-provider-codex:#3987e5]",
  "[--ai-provider-claude:#eb6834] dark:[--ai-provider-claude:#d95926]",
  "[--ai-provider-grok:#1baf7a] dark:[--ai-provider-grok:#199e70]",
  "[--ai-provider-copilot:#eda100] dark:[--ai-provider-copilot:#c98500]",
  "[--ai-series-1:#2a78d6] dark:[--ai-series-1:#3987e5]",
].join(" ");
