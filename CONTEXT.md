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
A fixed-fee plan a developer declares for one AI provider, such as Claude Max 5x, with a monthly cost in its own currency and an inclusive period of local dates. One user's subscriptions to one provider never overlap.
_Avoid_: seat, license

**Billing mode**:
How a developer's usage of a provider on one local day is billed: by the AI subscription that covers the day, or otherwise as API usage at its estimated cost.
_Avoid_: plan type, tier

**Plan hint**:
A billing plan a provider reported during an hour of uploaded usage, such as Codex `plan_type`. Evidence only: a hint of a paid plan (not `free`) on a day that bills as API usage is a mismatch to review, and declared AI subscriptions stay authoritative.
_Avoid_: detected subscription

**API-equivalent cost**:
What AI usage would cost at the provider's API prices, estimated in USD by token-ledger. Usage on days without an AI subscription bills at it, and on subscription days it weighs how the fee is split across projects. Unpriced usage has an unknown cost, never zero.
_Avoid_: actual cost, spend, bill

**Unassigned usage**:
AI usage that counts for no time-tracking project: its project key is `unattributed`, has no mapping, or has a stale mapping to a project outside the configured company; and all usage while time tracking is not configured, when no mapping resolves. A developer can map an unmapped key in their own usage while time tracking is configured; only an admin can change a mapping. Billing charges it on its own line, and a mapping applies to past months too.
_Avoid_: unmapped usage (it is only one of these cases)

**Stale machine**:
A machine that has not synced AI usage for more than seven days. Its usage since the last sync is missing, not zero.
_Avoid_: inactive machine, offline machine

**Unallocated overhead**:
The pro-rated fee of an AI subscription without usage on any day it covers in a month. It bills without a project rather than as zero.
_Avoid_: unused subscription

**Uploaded day**:
A local day for which a machine's syncs prove its stored usage of a provider is complete: the union of its logged sync intervals, each from the window start to the window end or the report time, whichever is earlier, spans the whole day. Billing a month waits until every machine active in it has uploaded every day for each provider it uses.
_Avoid_: synced day (a sync can end mid-day)
