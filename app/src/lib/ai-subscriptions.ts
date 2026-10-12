import { isValid, parseISO } from "date-fns";
import {
  AI_PROVIDERS,
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
  pi: [],
};

/**
 * The provider a subscription is declared for to pay for `provider`'s usage.
 * Pi has no subscriptions of its own: a ChatGPT subscription, declared for
 * Codex, pays for Pi usage on the days Pi ran on its ChatGPT sign-in.
 */
export function subscriptionProvider(provider: AiProvider): AiProvider {
  return provider === "pi" ? "codex" : provider;
}

/** The providers a subscription can be declared for. */
export const SUBSCRIPTION_PROVIDERS: readonly AiProvider[] =
  AI_PROVIDERS.filter(
    (provider) => subscriptionProvider(provider) === provider,
  );

export const CURRENCIES: readonly string[] = ["SEK", "USD", "EUR"];

const wholeUnits = new Intl.NumberFormat("en-US");
const wholeKronor = new Intl.NumberFormat("sv-SE");

/**
 * An exact amount such as `1100.00` in its currency: `1,100.00 USD`. Kronor
 * read the Swedish way, `1 100,00 SEK`.
 */
export function formatFee(amount: string, currency: string): string {
  const [units, cents] = amount.split(".");
  return currency === "SEK"
    ? `${wholeKronor.format(Number(units))},${cents} ${currency}`
    : `${wholeUnits.format(Number(units))}.${cents} ${currency}`;
}

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

/** Parses a real calendar day entered as `YYYY-MM-DD`, without a time zone shift. */
export function parseSubscriptionDate(value: string): Date | undefined {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.startsWith("0000")) {
    return undefined;
  }
  const date = parseISO(value);
  return isValid(date) ? date : undefined;
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
  if (!parseSubscriptionDate(draft.validFrom)) {
    return { ok: false, problem: "Enter the first day as YYYY-MM-DD." };
  }
  if (draft.validTo && !parseSubscriptionDate(draft.validTo)) {
    return {
      ok: false,
      problem: "Enter the end day as YYYY-MM-DD, or leave it empty.",
    };
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
