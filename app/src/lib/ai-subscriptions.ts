import {
  MAX_PLAN_CODE_POINTS,
  codePointLength,
  type AiProvider,
  type AiSubscription,
  type AiSubscriptionMismatch,
  type AiSubscriptionTerms,
} from "@/lib/api/queries/ai-usage";

/** Suggested plan names. Plans are free text, so any name is accepted. */
export const PLAN_SUGGESTIONS: Record<AiProvider, readonly string[]> = {
  codex: ["ChatGPT Plus", "ChatGPT Pro"],
  claude: ["Claude Pro", "Claude Max 5x", "Claude Max 20x"],
  grok: [],
  copilot: ["Copilot Business"],
};

export const CURRENCIES: readonly string[] = ["SEK", "USD", "EUR"];

/** Form state as typed; the cost may use a decimal comma. */
export type Draft = Readonly<{
  provider: AiProvider;
  plan: string;
  monthlyCost: string;
  currency: string;
  validFrom: string;
  validTo: string;
}>;

/** What a subscription form edits: a new subscription or an existing one. */
export type SubscriptionEditor =
  | Readonly<{ mode: "create"; initial: Draft }>
  | Readonly<{ mode: "update"; subscriptionId: number; initial: Draft }>;

export type ParsedDraft =
  | Readonly<{ ok: true; terms: AiSubscriptionTerms }>
  | Readonly<{ ok: false; problem: string }>;

export function emptyDraft(provider: AiProvider, validFrom: string): Draft {
  return {
    provider,
    plan: "",
    monthlyCost: "",
    currency: "SEK",
    validFrom,
    validTo: "",
  };
}

export function draftFrom(subscription: AiSubscription): Draft {
  return {
    provider: subscription.provider,
    plan: subscription.plan,
    monthlyCost: subscription.monthlyCost,
    currency: subscription.currency,
    validFrom: subscription.validFrom,
    validTo: subscription.validTo ?? "",
  };
}

export function parseDraft(draft: Draft): ParsedDraft {
  const plan = draft.plan.trim();
  if (!plan) {
    return { ok: false, problem: "Enter a plan, such as Claude Max 5x." };
  }
  if (codePointLength(plan) > MAX_PLAN_CODE_POINTS) {
    return {
      ok: false,
      problem: `Shorten the plan to at most ${MAX_PLAN_CODE_POINTS} characters.`,
    };
  }

  const monthlyCost = draft.monthlyCost.replace(/\s/g, "").replace(",", ".");
  if (!/^\d{1,10}(\.\d{1,2})?$/.test(monthlyCost)) {
    return {
      ok: false,
      problem:
        "Enter the monthly cost as an amount with at most two decimals, such as 1100 or 19.99.",
    };
  }

  if (!draft.validFrom) {
    return { ok: false, problem: "Enter the first day it covers." };
  }
  if (draft.validTo && draft.validTo < draft.validFrom) {
    return { ok: false, problem: "The end must not be before the start." };
  }

  return {
    ok: true,
    terms: {
      provider: draft.provider,
      plan,
      monthlyCost,
      currency: draft.currency,
      validFrom: draft.validFrom,
      validTo: draft.validTo || null,
    },
  };
}

export function formatPeriod(subscription: AiSubscription): string {
  return subscription.validTo
    ? `${subscription.validFrom} – ${subscription.validTo}`
    : `from ${subscription.validFrom}`;
}

/** Which days a mismatch covers, in words. */
export function describeDays(mismatch: AiSubscriptionMismatch): string {
  if (mismatch.firstDay === mismatch.lastDay) {
    return `on ${mismatch.firstDay}`;
  }

  const days = `${mismatch.days} ${mismatch.days === 1 ? "day" : "days"}`;
  return `on ${days} from ${mismatch.firstDay} to ${mismatch.lastDay}`;
}

/**
 * Status on the server's `today`. Local dates compare correctly as
 * `YYYY-MM-DD` strings.
 */
export function periodStatus(
  subscription: AiSubscription,
  today: string,
): string {
  if (subscription.validFrom > today) {
    return "upcoming";
  }
  if (subscription.validTo !== null && subscription.validTo < today) {
    return "ended";
  }
  return subscription.validTo === null ? "ongoing" : "active";
}
