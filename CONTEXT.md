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
A fixed-fee plan a developer declares for one AI provider, such as Claude Max 5x, with a monthly cost in its own currency and an inclusive period of local dates. One user's subscriptions to one provider never overlap. Pi, which routes its requests to other vendors, has none of its own: a ChatGPT subscription, declared for Codex, also pays for Pi usage on ChatGPT sign-in days. Neither has OpenCode, which routes its requests too: the Codex subscription pays for the usage it uploads with Codex as payer. Copilot takes none: it is a metered payer.
_Avoid_: seat, license

**Billing mode**:
How a developer's usage of a provider on one local day is billed: by the AI subscription that covers the day, or otherwise as API usage at its estimated cost. Pi usage is billed by the developer's Codex subscription that covers the day when the day is a ChatGPT sign-in day, and otherwise as API usage. Usage uploaded with an explicit payer, such as OpenCode's through a ChatGPT sign-in, is billed by the developer's subscription to that payer covering the day, and otherwise as API usage; OpenCode usage without one is API usage. Usage a metered payer pays for is API usage before the metered cutover, and from it on metered: billed from the payer's own credits per UTC month.
_Avoid_: plan type, tier

**Plan hint**:
A billing plan a provider reported during an hour of uploaded usage, such as Codex `plan_type`. Evidence only: a hint of a paid plan (not `free`) on a day that bills as API usage is a mismatch to review, unless a metered payer pays for the day, and declared AI subscriptions stay authoritative. Pi names no plan; its one hint, `openai-codex`, is the route of its ChatGPT sign-in and makes the day a ChatGPT sign-in day, the only way a hint bears on a billing mode.
_Avoid_: detected subscription

**ChatGPT sign-in day**:
A local day on which Pi hinted, in any hour and from any of a developer's machines, that usage ran through its ChatGPT sign-in, a route without an API-key variant. All of the developer's Pi usage that day counts as paid for by a ChatGPT subscription, since a hint names an hour and no single request; whether there is one, and its fee, remain what the developer declared for Codex. Without a covering Codex subscription the day bills as API usage and is a mismatch.
_Avoid_: OAuth day, subscription day

**API-equivalent cost**:
What AI usage would cost its payer, estimated in USD by token-ledger: at Copilot's rates for usage Copilot pays for, and at the vendor's API prices otherwise, with OpenAI's implicit cache writes priced whoever pays. The name predates Copilot pricing. Usage on days without an AI subscription bills at it (a metered payer's usage from the metered cutover on bills from its credits instead), and on subscription days it weighs how the fee is split across projects. Unpriced usage has an unknown cost, never zero.
_Avoid_: actual cost, spend, bill

**Metered payer**:
A provider that bills usage as it is used, so no AI subscription to it can be declared and none is ever missing: GitHub Copilot, billed per credit. Usage it pays for, OpenCode's through Copilot included, is API usage at its API-equivalent cost before the metered cutover, and bills as Copilot lines from it on.
_Avoid_: pay-as-you-go subscription

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
AI usage that counts for no time-tracking project and is not personal usage: its project key is `unattributed`, has no mapping, or has a stale mapping to a project outside the configured company; and all usage while time tracking is not configured, when no mapping resolves. From the metered cutover on, so are code review credits of an unmapped repository. A developer can map an unmapped key in their own usage while time tracking is configured; only an admin can change a mapping. An unmapped key may name a private repository, so only the developers who use it see its name and only they can map it: admins see its usage as Unassigned, unnamed, until one of them does. Billing charges it on its own line, shown by kind (unmapped, unmapped repository, unattributed), and a mapping applies to every open month; a closed month gets an adjustment line on the next open month's bill.
_Avoid_: unmapped usage (it is only one of these cases)

**Personal usage**:
A developer's AI usage of a project key they marked as personal or internal: not client work. It is left out of the bill: it bills nothing on API days and takes no share of a subscription's fee, which is split over the developer's other usage. From the metered cutover on, the Copilot credits it accounts for bill as a personal/internal Copilot line, Spinit's. Admins see it only as an API-equivalent estimate per developer, never the key's name. The mark is the developer's own and applies to their usage alone. Mapping the key clears every mark of it: the mapping then decides for everyone, and deleting it leaves the usage unassigned. Shown as "Personal / internal".
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

**Provider account**:
An account at a metered payer that usage is billed to: for GitHub Copilot, a GitHub user, identified by GitHub's numeric id, which survives renames, and shown by its latest login. Toki keeps every account that ever held a seat, so usage on ended seats always has one.
_Avoid_: GitHub user, seat, login (a login can change)

**Seat**:
One assignment of a paid seat to a provider account, from the payer's assignment instant until a complete listing of the organization's seats omits it. An account can hold several seats over time, one at a time.
_Avoid_: license, subscription

**Account disposition**:
An admin's decision about whose usage a provider account's is, on UTC calendar days from a first through a last day (both included, open while ongoing): a Toki user's, or no Toki user's (an external account). One account's dispositions never share a day, nor do one user's for one payer.
_Avoid_: binding, mapping, link (a link is only the user kind)

**External account**:
A provider account whose disposition says it is no Toki user's: its usage is Spinit overhead. It is a decision, not a gap.
_Avoid_: unknown account, non-Toki user

**Unlinked account**:
A provider account without a disposition on some day it held a seat or used credits. Its usage on such days is nobody's yet, so it needs an admin's look.
_Avoid_: unmapped account, orphan account

**Metered usage import**:
Toki's copy of what a metered payer billed: each provider account's credits per UTC day and model, the organization's totals per model, its rows per SKU and repository, and the month summary. Re-importing unchanged data changes nothing.
_Avoid_: GitHub sync

**Gross credits (G)**:
A provider account's credits at the payer's gross quantity, priced at the $0.01 list rate before the pool discount. Customers are charged only their attributed part.
_Avoid_: net, cost, invoice amount

**Tracked Copilot (T)**:
Toki's Copilot-priced estimate of the Copilot-paid usage of the user a provider account is linked to, in credits per UTC day and model. Unpriced usage carries no weight, and neither do free models (next edit suggestions, gpt-4o-mini): whatever Toki priced them at stays out of T.
_Avoid_: actual usage

**Reconciliation**:
Comparing G with T per provider account, UTC day and group of (SKU, model): what the T/G rule attributes. From the metered cutover on, the bill's Copilot lines follow it, so both agree on G, T, attributed and untracked for every account.
_Avoid_: matching, audit

**Attributed Copilot**:
min(G, T) per UTC day and (SKU, model) group on days an account is linked to a user: what the T/G rule bills to that user's destinations, each at most its own tracked cost.
_Avoid_: billed Copilot

**Untracked Copilot**:
G minus attributed Copilot on days an account is linked to a user: usage GitHub billed that Toki did not track, such as the cloud agent or untracked tools. It bills as the user's untracked Copilot lines, Spinit overhead, never spread over customers.
_Avoid_: unbilled, missing usage

**Unknown account**:
The per-day residual: the organization's credits minus the sum over every provider account Toki imports, on days whose import is complete or still settling. Usage of accounts Toki never listed. It settles only once the organization's stored month equals the payer's month total. From the metered cutover on, a positive month residual bills as one Spinit "unknown account" line, code review left out (it is billed per repository), and while the residual is not settled the month's Copilot lines as a whole are provisional, with or without that line.
_Avoid_: external account, unlinked account

**Payer check**:
The latest comparison of a provider account's, or the organization's, stored days with the payer's month total: equal, different, or failed. An account's mismatch flags the account; the organization's keeps every residual day from settling, and a failed one makes them an import gap.
_Avoid_: sync check

**Import freshness**:
Whether the metered usage import is recent and complete enough to trust: fresh, or stale with the first reason (a failed run, an old success, coverage behind, an old summary, a seated account that failed).
_Avoid_: sync status

**Settle lag**:
How many days after a UTC day ended the payer still changed its numbers, as observed by the import. Only a change in what the payer sent counts, never Toki relabelling or a login rename. Days within the settle lag are provisional, and only settled days raise the flags that compare G with T.
_Avoid_: delay

**IDE aggregate**:
The organization usage row of AI credits without a repository: the sum of every account's IDE credits. Shown, never added to per-account credits. Code review GitHub billed without a repository is inside it and bills on no line; it never blocks month close.
_Avoid_: unassigned repository

**Metered cutover**:
The first UTC month (`ai_usage.metered_copilot_from`) whose bill takes a metered payer's lines from its own credits: from that month's first instant in UTC, usage the payer pays for leaves the local-day reads. Earlier months keep estimates; unsetting it is the rollback.
_Avoid_: go-live, switch date

**UTC billing month**:
The month a Copilot line belongs to: the UTC month of the payer's billing day. It shares the month key and exchange rate with the local month every other line follows.
_Avoid_: GitHub month, calendar month (ambiguous)

**Copilot line**:
A bill line of credits a metered payer billed, at $0.01 a credit: a project's or Unassigned share by the T/G rule, one of Spinit's (personal/internal, untracked Copilot, an unlinked, external or unknown account), or a repository's code review, to its mapped project or Unassigned, rounded once per repository and UTC month. Each account's lines sum exactly to its gross credits less code review, which bills once, per repository.
_Avoid_: GitHub line, credit line

**Billing subject**:
Whom a bill line charges for: a user, a provider account no user owns on the billed days, the unknown account, or a repository.
_Avoid_: owner, payer

**Repository mapping**:
A team-wide mapping from a repository a metered payer bills credits to (GitHub's bare repository name, compared case-insensitively) to a time-tracking project, which its code review credits bill to. It points at the project directly, has no validity dates and applies to every open month, like a project-key mapping (a closed month gets adjustment lines on the next open month's bill); only one to the configured company resolves. Suggestions come only from resolving project-key mappings `github.com/{owner}/{name}`, never from unmapped keys.
_Avoid_: repository key (a project key names a repository too, but is another mapping)

**Unmapped repository**:
A repository with code review credits on the bill that no repository mapping to the configured company resolves. Its credits bill as Unassigned usage of their own kind and block month close until it is mapped. Only credits that bill a line count: a month from the metered cutover on whose credits come to at least one credit once rounded. Credits before the cutover, or under one credit for the month, bill nothing and hold nothing back. Not the IDE aggregate, which has no repository.
_Avoid_: unassigned repository, unmapped key

**Code review check**:
Per UTC day, the code review credits GitHub billed per repository minus those it billed per account: the same credits, which the bill takes once, per repository. A day whose accounts exceed its repositories (a negative difference) is code review billed without a repository, inside the IDE aggregate, or in an unrecognized row naming a repository: it bills on no line. A day whose repositories exceed its accounts (a positive difference) is code review by an unknown account, whose per-account credits Toki never imports: it bills once, through its repository line, and the bill needs nothing. Informational: it never blocks or makes a line provisional.
_Avoid_: code review residual

**Provisional Copilot line**:
A Copilot line that may still change, with its reasons: the month is in progress, a day's import is missing or incomplete, a day may still change, contributing usage is on an old pricing basis, a provisional model contributed, or a row is unrecognized (for a repository line, an organization row naming a repository).
_Avoid_: draft line, estimate

**Month close**:
An admin freezing a month's whole AI bill (API lines, subscription fee shares and overhead, Copilot lines, and the adjustment lines posted on it) once the month is over in UTC and the billing time zone and every close readiness check passes or is waived. A month has at most one active close; re-opening keeps it as history and is refused while a later close holds adjustments to the month. Closing the first open month moves the reprice floor to the next open month; a closed month after an open one stays repriceable, and its reprice posts as an adjustment line.
_Avoid_: lock, finalize

**Snapshot**:
What a month close stores: every bill line as its JSON, its CSV cells and its diff key, plus the rest of the overview, each developer's bill part, the readiness and the CSV header. A closed month is served from it, with the same values and CSV bytes as when it closed.
_Avoid_: archive, backup

**Adjustment line**:
A change to a closed month since it closed, billed on the first open month after it, per diff key (subject, project, kind, currency, and the provider or subscription): the month recomputed now, less its snapshot, less the adjustments to it frozen in active closes. Its SEK amount is the difference of the SEK amounts at the closed month's rate, so a rate change alone gives an SEK-only line. Each correction bills exactly once. Kept apart from the month's own lines, with signed amounts.
_Avoid_: correction line, credit note

**Close readiness**:
The checks the server evaluates before a month closes: the metered import complete and settled, every account with credits disposed, the unknown account's residual zero, every repository mapped, no billed usage on an old pricing basis or a provisional model, final exchange rates, and complete uploads (in metered months including the UTC hours outside the local month).
_Avoid_: preflight, checklist (the UI's word for showing it)

**Waiver**:
An admin's recorded reason to close a month although one readiness check failed. It applies to that check only, and the close keeps it with the check's problems; whatever it let through is corrected later by an adjustment line.
_Avoid_: override, exception
