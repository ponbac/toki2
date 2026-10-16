# Toki

Toki is a provider-agnostic time tracking and development workflow platform.

## Language

**Automation API**:
The curated HTTP surface published to agents through the generated OpenAPI document. Runtime bearer authentication is composed independently and may cover routes outside this catalog.
_Avoid_: public API, MCP API, agent gateway

**Agent operation**:
One HTTP method and path in the Automation API, identified by a stable operation ID. An endpoint becomes an agent operation only by being listed in the curated OpenAPI document.
_Avoid_: tool, endpoint (when referring to this catalog specifically)

**API token**:
An opaque personal credential (`toki_...`) that authenticates a caller as a Toki user over HTTP bearer. The plaintext secret is shown once at issuance; only a hash is stored.
_Avoid_: PAT, access token, session cookie

**Admin power**:
What an admin may do beyond their own data, such as managing everyone's AI subscriptions or project mappings. It takes an admin in a browser session: an API token never carries admin power, not even an admin's, and a request with a token and an admin's session cookie authenticates as the token.
_Avoid_: admin mode, superuser

**Active timer**:
The single in-progress time-tracking interval for a user, if any.
_Avoid_: running entry, current registration

**AI subscription**:
A fixed-fee plan a developer declares for one AI provider, such as Claude Max 5x, with a monthly cost in its own currency and an inclusive period of local dates. One user's subscriptions to one provider never overlap. Pi, which routes its requests to other vendors, has none of its own: a ChatGPT subscription, declared for Codex, also pays for Pi usage on ChatGPT sign-in days.
_Avoid_: seat, license

**Billing mode**:
How a developer's usage of a provider on one local day is billed: by the AI subscription that covers the day, or otherwise as API usage at its estimated cost. Pi usage is billed by the developer's Codex subscription that covers the day when the day is a ChatGPT sign-in day, and otherwise as API usage.
_Avoid_: plan type, tier

**Plan hint**:
A billing plan a provider reported during an hour of uploaded usage, such as Codex `plan_type`. Evidence only: a hint of a paid plan (not `free`) on a day that bills as API usage is a mismatch to review, and declared AI subscriptions stay authoritative. Pi names no plan; its one hint, `openai-codex`, is the route of its ChatGPT sign-in and makes the day a ChatGPT sign-in day, the only way a hint bears on a billing mode.
_Avoid_: detected subscription

**ChatGPT sign-in day**:
A local day on which Pi hinted, in any hour and from any of a developer's machines, that usage ran through its ChatGPT sign-in, a route without an API-key variant. All of the developer's Pi usage that day counts as paid for by a ChatGPT subscription, since a hint names an hour and no single request; whether there is one, and its fee, remain what the developer declared for Codex. Without a covering Codex subscription the day bills as API usage and is a mismatch.
_Avoid_: OAuth day, subscription day

**API-equivalent cost**:
What AI usage would cost at the provider's API prices, estimated in USD by token-ledger. Usage on days without an AI subscription bills at it, and on subscription days it weighs how the fee is split across projects. Unpriced usage has an unknown cost, never zero.
_Avoid_: actual cost, spend, bill

**Billing currency**:
The one currency AI usage is billed in, SEK. Every billable amount is shown in it, converted at the month's exchange rate, next to its original amount and currency. Amounts already in SEK, and zero amounts, need no rate.
_Avoid_: invoice currency, home currency

**Exchange rate**:
How many SEK one unit of another currency is worth for a billing month: the Riksbank's average of the month's daily rates, stored with the month so a bill can be reproduced. An admin's override applies instead; the fetched rate is kept beneath it and applies again when the override is reset. A missing rate is never guessed: amounts that need it have no SEK figure.
_Avoid_: FX rate, conversion rate, spot rate

**Provisional rate**:
An exchange rate that is the average of the days published so far: that of a month in progress, or of a month whose average is not published yet. It is refreshed once a newer day may be published, at most hourly, until the month's final average replaces it.
_Avoid_: estimated rate, temporary rate

**Unassigned usage**:
AI usage that counts for no time-tracking project and is not personal usage: its project key is `unattributed`, has no mapping, or has a stale mapping to a project outside the configured company; and all usage while time tracking is not configured, when no mapping resolves. A developer can map an unmapped key in their own usage while time tracking is configured; only an admin can change a mapping. An unmapped key may name a private repository, so only the developers who use it see its name and only they can map it: admins see its usage as Unassigned, unnamed, until one of them does. Billing charges it on its own line, and a mapping applies to past months too.
_Avoid_: unmapped usage (it is only one of these cases)

**Personal usage**:
A developer's AI usage of a project key they marked as personal or internal: not client work. It is left out of the bill: it bills nothing on API days and takes no share of a subscription's fee, which is split over the developer's other usage. Admins see it only as an API-equivalent estimate per developer, never the key's name. The mark is the developer's own and applies to their usage alone. Mapping the key clears every mark of it: the mapping then decides for everyone, and deleting it leaves the usage unassigned. Shown as "Personal / internal".
_Avoid_: private usage, excluded usage, non-billable usage

**Stale machine**:
A machine that has not synced AI usage for more than seven days. Its usage since the last sync is missing, not zero.
_Avoid_: inactive machine, offline machine

**Unallocated overhead**:
The pro-rated fee of an AI subscription without usage on any day it covers in a month. It bills without a project rather than as zero.
_Avoid_: unused subscription

**Uploaded day**:
A local day for which a machine's syncs prove its stored usage of a provider is complete: the union of its logged sync intervals, each from the window start to the window end or the report time, whichever is earlier, spans the whole day. Billing a month waits until every machine active in it has uploaded every day for each provider it uses.
_Avoid_: synced day (a sync can end mid-day)

**Buddy**:
A pixel-art companion shown next to the timer, of a species that has sprites and battle data. A user enrols by answering whether they want one and choosing its species, and can switch it off without losing it. Each user has a lead and, later, a partner; they level up from the user's Buddy XP and battle colleagues' buddies and the daily trainer.
_Avoid_: pet, avatar, mascot

**Lead**:
A user's first buddy, chosen on enrolment. It always fights, and fights first; its XP is the user's total Buddy XP.
_Avoid_: main buddy, primary buddy

**Partner**:
A user's second buddy, which unlocks when the lead reaches Lv 10. It earns only the Buddy XP gained after it was first set. It can change species or be hidden, never removed, and a hidden partner still fights. When both users have a partner they battle two against two, otherwise lead against lead.
_Avoid_: second buddy, sidekick

**Buddy XP**:
What a user's buddies level up from: 60 per hour logged on a local day, for at most 10 hours a day, from the day the user enrolled, plus 20 for each battle the user starts, whatever its result. Logged hours are project-backed time entries, including time logged directly in the time-tracking provider; absences do not count. Recent days are read again, so later edits count. Changing a buddy's species keeps its XP and level.
_Avoid_: points, experience

**Battle level**:
The level a buddy fights at: 47 plus a fifteenth of its level, from about 47.1 for a new buddy to 53.7 at Lv 100. A veteran buddy has an edge over a new one, never an overwhelming one.
_Avoid_: effective level

**Daily trainer**:
A computer-controlled opponent that everyone shares for one local day: two species drawn by the date, both Lv 25, and a name from a fixed list. Each user may battle it once a day, apart from the hourly limit on battling colleagues.
_Avoid_: NPC, boss, bot
