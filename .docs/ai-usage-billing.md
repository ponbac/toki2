# AI usage source and subscription payer

Token Ledger uploads a source tool as `provider`. Sync payload version 2 additionally
accepts an optional `billingProvider` on an hourly usage bucket. It names the
subscription provider that should pay for those records; it must differ from the
source and support subscriptions (`codex`, `claude`, `grok`, or `copilot`). Version 1
continues to work and rejects non-null overrides so clients cannot silently rely on
metadata an older server ignores. `copilot` stays a valid payer although no
subscription to it can be declared (see below): it is how usage routed through
Copilot is told apart.

The bucket key includes the nullable payer. Native Copilot and requests routed
through ChatGPT can therefore share an hour, session, project, and model without
being merged. Coverage, authoritative replacement, reporting, and source labels
still use the source provider, except that each day row of a developer's usage report
also carries its `payer` (the explicit payer, else the source), which the daily spend
chart stacks by by default. No request IDs, auth modes, or local hint paths are
accepted as billing inputs or stored by this feature.

Billing resolves an explicit payer against the developer's declared subscription
covering the local usage day. If none covers it, the record bills at its estimated
cost, even when a subscription for the source tool exists. A single subscription
fee is shared across every source it pays for. Existing unoverridden Pi sign-in
routing is unchanged. Missing-subscription warnings for explicit overrides identify
the payer so adding the suggested subscription fixes the right provider.

## Cost estimates

Every cost is Token Ledger's USD estimate of what the payer bills. Usage Copilot pays
for (provider `copilot` without a payer, or `billingProvider: copilot`) is priced at
Copilot's own rate card; other usage at the vendor's API prices. Both choose the
long-context tier per request. OpenAI's implicit cache writes are priced whoever pays:
on OpenAI models with a cache-write rate, the uncached input clients report as `input`
is priced at that rate, all but 3 tokens per request, although no `cacheWrite` tokens
are reported. The upload's `costBasis` literal stays `api-equivalent` for
compatibility. Toki stores costs as sent: buckets before `ai_usage.reprice_floor`, or
not yet repriced, keep the method they were priced with, which their pricing basis
tells (below).

## Copilot takes no subscription

GitHub bills Copilot usage per credit, so Copilot is a metered payer
(`METERED_PAYERS`): declaring or updating a subscription to `copilot` is refused, the
subscription form no longer offers it, and a day whose payer is Copilot (Copilot plan
hints, and buckets with `billingProvider: copilot`, such as OpenCode's through
Copilot) is never a subscription mismatch. Before the metered cutover such usage
bills as API usage at its estimate; from it on, it bills from GitHub's own credits
(see "GitHub Copilot lines in the bill"). No Copilot subscriptions existed when
declaring them was blocked.

## OpenCode

OpenCode (`opencode`) routes requests to other vendors, so like Pi it has no
subscriptions of its own, and declaring one for it is rejected. It relies on the
payer above instead: Token Ledger sets `billingProvider` to `copilot` for its
`github-copilot` route and to `codex` for its `openai` route on a ChatGPT sign-in.
Its Copilot buckets bill at their estimate, at Copilot's rates, and from the metered
cutover on from GitHub's credits like any Copilot-paid usage. Every other OpenCode
bucket has no payer and bills at its API estimate; Pi's ChatGPT sign-in days never
apply to it. OpenCode sends no plan hints, and Toki rejects an upload with one, since
no subscription could resolve its mismatch. Roll out Toki's migration accepting
`opencode` before a Token Ledger release that reads it: even a machine without
OpenCode reports it as `missing` coverage.

The admin daily response includes nullable `billingProvider`, `billingMode` (`api`,
`subscription` or `metered`: from a metered payer's credits, see "GitHub Copilot
lines in the bill") and `subscriptionId`, which identifies the subscription
actually used and is non-null exactly on subscription rows. Daily labels show
**Mixed billing** when one source's rows bill in more than one way: different
subscriptions, or any two of API, subscription and metered (`dayBilling` in
`app/src/routes/_layout/admin/-lib/daily.ts`).

For the ChatGPT proxy integration, Token Ledger joins the proxy's incoming
`X-Request-Id` with Copilot's exported `gen_ai.response.id`. The proxy appends:

```json
{"requestId":"incoming-X-Request-Id","billingProvider":"codex","authMode":"chatgpt"}
```

Windows default: `%LOCALAPPDATA%\token-ledger\billing-hints\chatgpt-proxy.jsonl`.
The upstream ChatGPT response ID is not the join key. Hints annotate existing token
records and never create extra usage.

Roll out Toki's migration and version-2 API support **before** publishing/updating
Token Ledger 0.6.0. Clients without matched hints keep uploading version 1. Older
clients can still authoritatively replace a source window, removing any previous
payer splits; keep mixed client versions from syncing the same installation.

Token Ledger's `docs/billing-hints.md` describes platform paths, optional config,
failure isolation, append ordering, and required log retention for backfills.

## Partly priced buckets (version 3)

Versions 1 and 2 require `estimatedCostUsd` to be null whenever a bucket has any
unpriced records, so the known cost of the bucket's priced records was dropped and
Toki under-reported and under-billed that usage. Version 3 sends the priced subtotal
instead: `estimatedCostUsd` is null exactly when every record is unpriced, and
otherwise excludes the `unpricedRecords`, which stay an unknown amount on top. Reports
and billing already sum costs as known subtotals and need no change.

Deploy Toki's migration and version-3 support **before** the client. Clients only
send version 3 when a bucket in the upload is partly priced; other uploads keep
version 1 or 2.

## Copilot next edit suggestions

GitHub bills no Copilot plan for code completions or next edit suggestions (NES).
Token Ledger finds no API price for NES models such as `copilot-nes-lysithea-24`
and uploads them as unpriced, so Toki stores Copilot buckets whose model starts
with `copilot-nes-` or `nes-` at zero cost, unless the bucket names another payer.
VS Code fetches its NES models from Copilot, so new ones are matched by prefix.

## Upload ordering (`snapshotAt`)

Uploads may carry an optional `snapshotAt` in any version: when the client started
reading its history. Toki stores the newest applied one per machine and refuses an
older upload with 409 `SupersededUpload`, changing nothing, so a slow retry or an
overlapping scheduled and manual sync cannot roll back buckets, hints or coverage.
Don't resend a payload refused with 409: the newer upload may cover another window or
providers, so rescan the same window and send that. A snapshot more than 5 minutes
ahead of the server clock is applied but not stored, so a wrong client clock cannot
lock its machine out. Coverage logged for such an upload ends at its snapshot.
Clients that omit it are applied in arrival order, as before.

## Pricing basis and reprice-only uploads

Uploads may carry an optional `pricing.basis` in any version: 1 to 128 characters
naming the pricing method and rates behind every cost in the upload, such as
Token Ledger's rate card versions. Toki treats it as opaque and only compares it
for equality. It is stored on every bucket the upload writes, in
`ai_usage_buckets.pricing_basis`; an upload without one stores NULL. The latest
`pricing.source` per machine and provider stays in the machine coverage as before,
but any ordinary upload overwrites it, so only the per-bucket basis tells which
stored costs an older method priced.

Each machine also keeps its **current pricing basis**, in
`ai_usage_machines.pricing_basis`: the latest non-null basis of its uploads and
repricings, in the order they were applied. Every applied upload with a basis sets
it, and every repricing sets it to the repricing's basis, also when no bucket in
it lies at or after the floor. An upload without a basis keeps it: Token Ledger
before 0.10.0 sends none, and 0.10.0 sends none for a run that priced without
vendor rates (`unavailable`), so such a run never claims the basis a complete run
does. Its buckets are stored with NULL and read as old on their own. A refused
upload (409) or another user's repricing changes nothing. The hour a bucket covers
says when its usage happened, not when it was priced, so the newest bucket's basis
cannot stand in for it: an upload of an older window or a repricing can move older
buckets to a new basis while newer ones keep the old. Machines that uploaded
before the column existed started from their newest bucket's non-null basis.

When the client's basis changes, it reprices stored history through two
operations on the upload's bearer auth. Neither is an MCP tool:

- `GET /ai-usage/machines/{machineId}/repricing?basis=…` (`getAiUsageRepricing`)
  returns `{ floor, providers: [{ provider, from, staleBuckets }] }`: one entry per
  provider with stored buckets of the caller's machine, ordered by provider name.
  `from` is the earliest hour at or after the floor whose bucket's basis differs
  from `basis` (NULL counts as different), or null, and `staleBuckets` counts those
  buckets. A machine the caller has not registered returns no providers. A missing,
  empty or longer basis is a 400. Percent-encode the basis in the query: a bare `+`
  reads as a space.
- `POST /ai-usage/machines/{machineId}/repricing` (`repriceAiUsage`) takes
  `{ version: 1, clientVersion, pricing: { status, fetchedAt, source, basis },
  buckets }`, with `basis` required and at most 20,000 buckets in the upload's
  bucket shape and validation (version 3 costs, unique keys, no window). In one
  transaction, each bucket at or after the floor updates the cost, unpriced records
  and basis of the stored bucket of the caller's machine with the same key (payer
  compared null-safely), the same four token counts and the same record count, after
  the upload's next edit suggestion rule. It never inserts or deletes usage and
  changes no coverage, hints, snapshot or sync time, so history the machine has
  pruned, and usage it now attributes or routes differently, keeps its stored cost
  and basis. It does record `basis` as the machine's current pricing basis. The
  response is `{ updated, unmatched, beforeFloor }`; `updated` counts matches whose
  values were equal already too. The body limit is 16 MiB, as for uploads.

The floor is config `ai_usage.reprice_floor` (RFC 3339, `2026-10-01T00:00:00Z` in
`base.yaml`): costs before it are never repriced, so already invoiced months keep
theirs. With month closes (see "Month close, snapshots and adjustments") the floor is
the later of that and the start of the first open month: the earliest month without an
active close, at or after the month the configured floor falls in (the earlier of its
UTC and its `ai_usage.time_zone` month), from the earlier of its UTC and its local
midnight (`open_from`), so that month stays repriceable in both bases from its first
hour on. Every month from the configured floor's month up to it is closed. A closed
month after the first open one stays repriceable: closing November while October is
open leaves both open to repricing. A reprice of a closed month, like one of its last
hours in its other basis or one that races its close, changes only its recompute, and
the next open month after it nets the difference as an adjustment line, so each
correction bills exactly once.
Discovery reports that floor, repricings count buckets before it in `beforeFloor`, and
the bill, reconciliation and close readiness (`pricing`) read the old pricing basis
from it. A month that is never invoiced, such as October 2026 before the first close,
holds the floor back until it is closed or the configured floor moves past it. Deploy
Toki before a Token Ledger release that sends a basis: older servers ignore the field,
but have no repricing operations.

## Unassigned usage by kind

Usage that no mapping to the configured company resolves bills to Unassigned. The
bill shows it in three kinds, from `unattributed`, from every other key, and from
a metered payer's repository credits:

- **unmapped**: repositories nobody has mapped, and keys mapped to another
  company's project. Mapping them on Admin > Project mappings assigns them,
  as long as time tracking is configured. A key with surrounding whitespace
  cannot be mapped.
- **unmapped repository**: code review credits GitHub billed to a repository
  that no repository mapping to the configured company resolves (see
  "Repository mappings"). Mapping the repository under Project mappings >
  GitHub repositories assigns them; until then they block month close (readiness
  `repositories`).
- **unattributed**: usage outside any repository, which no mapping can assign.

Unassigned still bills as one project: a subscription fee is split over it and
the projects as before, and an API line rounds to cents once. Splitting it into
two projects of the fee split could move a cent to a mapped project, and two API
lines could change the totals by a cent. `AiBillingLine::parts` then splits the
Unassigned line's amount, and its converted amount, between the kinds by the
largest-remainder method, weighted like a fee split, with ties to unmapped.
The kinds therefore sum exactly to the Unassigned amount in both currencies,
but a kind's cents need not be its own estimate rounded, and its SEK amount need
not be its own amount converted. For example, two kinds of $0.004 each bill
$0.01 and $0.00, and at 10 SEK per USD, 0.05 SEK each.

`daily_usage` keeps the kinds apart in SQL but rounds their costs as one group:
each kind gets the rounded running total through it, less the one before it.
Unmapped key names are never returned. The HTTP lines carry `unassigned`
(`unmapped`, `unmappedRepository`, `unattributed` or null), and the CSV has an
`unassigned` column (`unmapped`, `unmapped_repository` or `unattributed`) after
the original columns, so those keep their places. A metered line carries one
kind, so its Unassigned part is the line itself. The metered columns
follow it (see **CSV** under "GitHub Copilot lines in the bill").

## Provider accounts and dispositions (GitHub Copilot)

GitHub bills Copilot per seat and per credit to the GitHub account that used it,
so Copilot's credits can only be billed through accounts. Toki keeps every
account that ever held a Copilot seat in the organization, and an admin decides
whose usage each one is. From the metered cutover on, the bill's Copilot lines
read these dispositions: a linked account's credits bill to its user's
destinations, and an unlinked or external account's to a Spinit line.

**Accounts and seats.**

- An account is a GitHub user at a metered payer, keyed by `(provider,
  account_id)`: GitHub's numeric user id, which survives renames. Its login is
  the latest one seen (`ai_provider_accounts`).
- Toki reads the organization's seats and each account's public profile from a
  GitHub App (`ProviderAccountDirectory`, `GithubAccountDirectory`). A refresh
  runs once a UTC day in release builds (hourly checks that refresh only when no
  refresh succeeded that UTC day and the last attempt is at least 55 minutes
  old) and when an admin presses Refresh. Local debug builds never refresh
  unattended, since their default database is a production copy.
- `ai_provider_seats` holds one row per seat assignment, keyed by GitHub's own
  assignment instant (`created_at`, compared as an instant whatever offset GitHub
  renders it in). Each row records when Toki first and last saw it listed. A seat
  ends (`unlisted_at`) only when a **complete** listing omits its account, or
  when its account is listed with another assignment instant. A listing is
  complete when pagination ended on a short page (an empty one included) and it
  holds at least as many accounts as GitHub's `total_seats`; an incomplete one
  ends nothing, and the admin page says so. A seat's last day is its pending
  cancellation day when that lies between the day it was last listed and the
  day it was found unlisted, else the day it was last listed. Accounts and seats
  are never deleted, so credits on ended seats always have an account.
- `ai_provider_directory_status` keeps the last attempt, the last success, the
  last error and the listed and reported seat counts.

**Dispositions** (`ai_provider_account_dispositions`) say whose an account's
usage is on UTC calendar days (GitHub's billing days) from `valid_from` through
`valid_to`, both inclusive, `valid_to` NULL while ongoing. The rules are those of
AI subscription periods:

- `user`: a Toki user's. `external`: no Toki user's, so Spinit overhead. An
  External is an explicit decision, unlike having no disposition (an unlinked
  account), and it can be dated, for example External until the person joins.
- One account's dispositions never share a day, and neither do one user's for one
  payer. Both are exclusion constraints, so they hold between concurrent requests;
  Externals never conflict with each other. A deleted user's dispositions go with
  them, leaving the account unlinked.
- A change of owner is "set a last day, then create from the next day". Deleting
  removes a disposition as if it never existed. Edits apply to every open month; a
  closed month keeps its snapshot and gets adjustment lines on the next open month's
  bill.
- A new disposition defaults to the days that need one: every day the account
  held a seat, and every day after today while it holds one. It starts the first
  uncovered stretch of those days that reaches into the review window (below),
  else the first, reaching back over earlier stretches that no disposition
  separates from it, so without dispositions it starts on the first seat day. It
  ends the day before the next disposition starts; when none follows, it is
  ongoing while a seat is listed and ends on the last seated day once every seat
  has ended. When every such day is covered there is nothing to set, and the
  account gets no suggestions.

**Suggestions** compare an account's public profile and login with Toki users
(`domain/models/ai_account_suggestion.rs`). Names are folded first: lower case,
ø → o, æ → ae, ß → ss, ł → l, đ → d, þ → th, œ → oe, accents dropped. Per family,
in this order, the first matching rule is evidence:

| Family | Evidence | Rule |
|---|---|---|
| email | `email` | the public email is the user's |
| name | `name` | ≥ 2 name tokens, the user's in order or as the same multiset |
| name | `nameFirstLast` | ≥ 2 tokens, the user's first and last |
| name | `nameInitial` | ≥ 2 tokens, the user's first, then the last's initial |
| name | `nameFirstOnly` | 1 token, a first name no other user has |
| login | `loginFullName` | the login's core is first+last, last+first or every token |
| login | `loginInitialSurname` | ≥ 6 characters: the first's initial, then a prefix of the last |
| login | `loginFirstSurnamePrefix` | the first, then ≥ 2 leading characters of the last |
| login | `loginFirstOnly` | a first name no other user has |

A login's core drops digit runs and the organization's own words (`spinit` for
Spinit-AB) and strips them once from either end. A candidate is **verified** only
with `email` or `name` evidence, as the account's only such candidate, when no
other account has such evidence for the same user; everything else, login
evidence and usage activity included, is **probable**. Users already linked
during the account's default period are left out. Candidates rank by confidence,
then best evidence (email, name, nameFirstLast, loginFullName, nameInitial,
loginInitialSurname, loginFirstSurnamePrefix, nameFirstOnly, loginFirstOnly), then
the user's Copilot-paid cost, records and active days in the review window. "Accept
all verified" links only accounts without dispositions whose top candidate is
verified and still the picked user, from the default period; the server
recomputes the suggestions and skips every other pick with a reason. Probable
matches need a confirmation per row.

The committed tests use invented people. To check the rules against real data,
put `users.tsv` (id, full name and email, tab separated, exported with `BEGIN READ
ONLY`), `profiles/*.json` (GitHub `GET /user/{id}` bodies) and `mapping.json`
(`{ "<userId>": { "login", "status" } }`) in a local directory, never committed,
and run:

```bash
TOKI_SUGGESTION_ORACLE_DIR=/path/to/oracle SQLX_OFFLINE=true \
  cargo test -p toki-api -- --ignored --nocapture suggestions_reproduce_the_local_oracle
```

**What needs a look.** The review window runs from the first day of the previous
UTC month through today. *Accounts without a disposition* held a seat on window
days that no disposition covers. *Users with Copilot-paid usage but no link* have
usage Copilot paid for (`COALESCE(billing_provider, provider) = 'copilot'`,
personal keys included, Copilot routed to Codex excluded) on window days that none
of their `user` dispositions covers.

**The GitHub App.** "Spinit Toki Billing" (`spinit-toki-billing`, App ID 5459019,
client ID `Iv23liEuEJDWE4ifGVp8`), installed on Spinit-AB as installation 174922684,
with the organization permissions GitHub Copilot Business: read (seats) and
Administration: read (billing usage, for the GitHub usage import). Config `github {
organization, api_base, app_client_id, installation_id }` is in `base.yaml`; the
private key comes only from `TOKI_GITHUB__APP_PRIVATE_KEY`, as PEM (PKCS#1 as GitHub
generates it, or PKCS#8), multi-line or with literal `\n`. Without it the directory
is off and Toki starts normally; a key that cannot be used marks it misconfigured,
shown on the page, without stopping startup. Toki signs a 10-minute RS256 JWT with
aws-lc-rs (never the `rsa` crate), exchanges it for an installation token and caches
that until 5 minutes before it expires; a `401` drops the token and retries once.
Rotate the key in the App's settings, update the secret, then delete the old key
there. A refresh costs one seat page per 100 seats and one profile call per stored
account, plus at most one token an hour: some 40 calls a day against the
installation's limit of 5,000 or more an hour.

**API.** The routes are browser-only under `/ai-usage/admin/provider-accounts`,
behind admin power like the rest of admin billing, and off the agent OpenAPI
document and MCP: `GET` (the overview), `POST /refresh`, `POST /dispositions`,
`PUT|DELETE /dispositions/{id}` and `POST /accept-suggestions`. Overlaps are 409,
a directory that is off or misconfigured 503, GitHub's rate limit 429 and other
GitHub failures 502.

With the GitHub usage import (below), each account row also shows its credits
this UTC month, the review window's days with credits and no disposition (which
put the account under *Accounts without a disposition*, seat or no seat: every
account with credits needs a disposition), and how many of those credit days
fall outside every seat. Once every seat day is covered, or for an account that
never held a seat, a new disposition defaults to those credit days instead: from
the first through the day before the next disposition, or through the last when
none follows. So an account flagged for its credits alone can always be set and
gets suggestions.

## GitHub usage import and reconciliation

Toki imports what GitHub billed for Copilot, per account, UTC day and model, and
reconciles it with Toki's own Copilot-priced usage. From
`ai_usage.metered_copilot_from` on, the bill's Copilot lines read these tables
(below), repository mappings bill their repository rows (see "Repository
mappings"), and month close reads their coverage (readiness `import`).

**Port and adapter.** The domain reads a `MeteredUsageSource` (credits of an
account or the organization for a UTC month or day, the organization's usage rows,
and the month summary). `GithubMeteredUsageSource` implements it over the GitHub
App of the seat directory: every GitHub adapter shares one `GithubApp`, so one HTTP
client, one installation token cache and one pacer that starts at most 10 requests
a second (under GitHub's secondary limit of 900 points a minute for the whole REST
API).

| Endpoint (organization from `github.organization`) | Used for |
|---|---|
| `GET /organizations/{org}/settings/billing/ai_credit/usage?year&month&user` | An account's month total: change detection |
| `…/ai_credit/usage?year&month&day&user` | An account's day |
| `…/ai_credit/usage?year&month[&day]` (no `user`) | The organization's credits per model: the residual |
| `GET /organizations/{org}/settings/billing/usage?year&month` | The organization's rows per SKU and repository for the whole month, each dated (the day form exists too) |
| `GET /organizations/{org}/settings/billing/usage/summary?year&month&product=copilot` | Seats and the pool discount |

Numbers are read from the JSON text exactly (`serde_json` `RawValue`, exponents such
as `1e-05` included), never through floating point. Every response is checked
against its request (the period, `user` compared case-insensitively, the
organization and each usage row's `organizationName`, and each row's `date` within
the period); a mismatch, a negative or non-numeric amount, or two rows of one key
that cannot be merged fails the slice as an unexpected response. GitHub's `403` names
the permission Administration: read; a `404` is "no Copilot usage for {login}" for an
account and "not installed" for the organization.

**Storage** (`20261027120000_add_ai_provider_usage.sql`):

- `ai_provider_account_usage`: an account's credits per UTC day, product, SKU and
  model label, keyed to `ai_provider_accounts`. `ai_provider_org_credits`: the same
  for the whole organization. `ai_provider_org_usage`: the organization's rows per
  day, product, SKU, unit and bare repository name. `ai_provider_usage_summaries`:
  the month summary.
- Quantities, unit prices and gross/discount/net amounts are stored exactly as GitHub
  gives them (NUMERIC). `G`, the gross credits in dollars, is `grossQuantity` ×
  $0.01, rounded half up to nanodollars per row at import (`gross_usd_nanos`);
  GitHub's `grossAmount` is kept but is not exactly that product. Credit rows priced
  otherwise than 0.01 per AI credit are unrecognized: stored and flagged, never
  tracked.
- Each fetched slice (an account's day, the organization's day, a month's summary)
  is replaced in one transaction: its rows are read `FOR UPDATE`, compared with what
  GitHub answered (every key and value), and deleted and inserted again only when
  they differ. Rows GitHub restates under another label therefore never linger, and
  re-importing unchanged data changes nothing, not even `fetched_at`. An empty month
  total proves every day of the account empty, without day calls.
- Only what GitHub sent (keys, units, unit prices and amounts) can change a slice.
  Toki's own fields (the normalized model, auto-routing, the kind and the login the
  rows were requested for) are rewritten in place when they differ, as after a new
  label override or a login rename, keeping the rows' `fetched_at` and leaving the
  slice unchanged: neither is a late change, so neither inflates the settle lag.
- `ai_provider_usage_slices` records each slice's fetches: `complete_at` is the first
  fetch after its period ended, `late_changed_at` the latest fetch that found other
  content from GitHub although the fetch before it was already complete.
  `ai_provider_usage_checks` keeps the latest comparison of each account's (and the
  organization's) stored month with GitHub's month total (`payer_gross_quantity`),
  and the run importing that month, if it has not finished the scope yet
  (`importing_run_id`).
  `ai_provider_import_runs` and `ai_provider_import_run_sources` record every run:
  kind, months, requests, slices fetched and changed, errors, and per source the
  coverage ("imported through UTC day X") and the observed settle lag.

**Labels.** GitHub's labels map to the model names of Toki's buckets, so G and T join
on them: "Auto: X" counts as X (flagged auto-routed; its gross already includes
Auto's discount), then an override table holds special labels ("Code Review model"
is model `code-review`, repository-billed), and any other label is lower-cased with
each run of whitespace replaced by `-` ("GPT-6.1 Sol" → `gpt-6.1-sol`). Raw labels are
kept. Unknown labels are flagged, never dropped. A catalog in
`domain/models/ai_metered_usage.rs` mirrors Token Ledger's `copilot-rates.json`:
models are confirmed, provisional (no billing evidence yet) or free (`copilot-nes-*`,
`nes-*`, `gpt-4o-mini*`, which carry no weight). SKUs: "Copilot AI Credits" is
trackable, "Copilot Cloud Agent" untrackable (Toki cannot see the cloud agent), and
anything else unrecognized. In the organization's rows, AI credits with a repository
are that repository's (code review); AI credits without one are the IDE aggregate,
the sum of every account's IDE credits, shown flagged and never added to per-account
rows; Actions is compute (never billed on); Copilot Business is seats.

**Accounts.** Every account item 3 keeps is imported, from `backfill_from`, not from
its first seat day: seat history starts when Toki first listed the account, and an
account can have usage under an earlier seat. Toki cannot ask GitHub about an
account it never listed (the endpoints need a login), so such usage appears only as
the residual, unknown account. This replaces item 3's earlier plan to add accounts
seen only in imported usage, which the header of
`20261026120000_add_ai_provider_accounts.sql` still mentions.

**Schedule.** `ai_usage.metered_import.backfill_from` (`2026-09-01`; local runs can
shorten it with `TOKI_AI_USAGE__METERED_IMPORT__BACKFILL_FROM`) is the first UTC day
imported; earlier days are never fetched. A shortened backfill in the middle of a
month therefore cannot match GitHub's month totals, and the checks show it.

- Hourly, change detection (`Changes`): the current month, plus the previous month
  while it is within the settle days. Per scope, days never fetched are fetched; then,
  when GitHub's month total differs from the stored one, the open days (from today
  minus the settle days through today); then, if it still differs, every day.
  Settle days are the larger of 3 and the observed settle lag of account credits.
- Nightly, from 02:00 UTC until one runs to the end (`Full`): every day of the current
  month, and of the previous month for 30 days after it ended, catching GitHub's late
  corrections, including usage moved between days without changing a month total,
  which change detection cannot see. A nightly run that stopped (below) did not
  finish the refresh, even when it is partial, so the next scheduled run is nightly
  again. Account errors that let the other accounts continue still count as finishing
  it. The failed account's month is tried again by the next run that imports the
  month: hourly change detection for the current month, and for the previous month
  within the settle days; otherwise the next night. An account that held a seat makes
  the import stale while its error is from the latest run. A stop that recurs makes
  every run from 02:00 UTC to midnight a nightly one. Runs import their months oldest
  first, so a recurring stop in the previous month leaves the current month to the
  hourly runs before 02:00 UTC, which first import the interrupted previous month in
  `Changes` mode (the import goes stale once the current month's summary is more than
  3 hours old). Likewise, once a nightly run needs more than 4,000 requests, every
  retry stops near the same point and leaves what follows to those runs.
- Months holding a slice never fetched, or an interrupted check (below), join any
  run in `Changes` mode, so only the scopes with gaps or changes fetch days: the first
  run backfills, and an account listed later costs one month total per month plus
  its own days.
- Admins can press Import now (`Changes`) or Full refresh for a month. Unattended
  runs happen in release builds only; local debug builds, whose database is usually
  a production copy, never import on their own.
- One run at a time: an in-process lock and a Postgres advisory lock
  (`ai_provider_usage_import:copilot`, transaction-scoped: a lease is an open
  transaction on its own pooled connection, and committing or dropping it frees
  the lock). Holding the lock proves no other run is alive, so starting a run marks
  leftover `running` runs interrupted. A run shows as running only while some
  session holds the lock exclusively (read from `pg_locks`), so a run whose process
  died mid-import (a deploy, a crash) neither shows a spinner nor disables Import
  now; the next run marks it interrupted.
- A month close holds imports off while it reads and stores a metered month
  (`try_hold_imports`: the same key, shared, so it never shows as a running run).
  Meanwhile Import now is `409` ("an import is already running, or a month close
  is reading the usage") and an unattended run skips its tick; the next hourly
  tick runs what is due.
- A rate limit, rejected credentials, a refusal, any organization-scope error, a
  storage error, three `Unavailable` answers in a row or 4,000 requests stop a run,
  which records the reason. An account's other errors (not found, an unreadable
  answer) become its check error while the other accounts continue.
- Before touching a month's days, a run marks every stored check of the month
  (`importing_run_id`); each scope's new check, success or the account's own error,
  clears its mark. A check still marked by a run that is no longer importing is
  *interrupted*: the run stopped, or its process died, before finishing that scope,
  halfway through or not yet reached. It reads as failed ("the run stopped before
  finishing this month: " and the run's error, `interrupted` for a dead process),
  dated when that run started. So no check from before vouches for days the run may
  have left half replaced, or for an account's old days against the organization's
  refreshed ones. While the run is still importing (the run that holds the lock),
  its marks keep the checks they marked, so the page does not flicker; planning and
  coverage, which run under the lock, count every mark left. Months the run never
  started keep their checks; scopes never checked before stay unchecked.
- An interrupted check is no answer of the account's own, so it counts whatever the
  account's seat: the account is flagged and stays expected, every day of the month
  is an import gap, and the import is stale while the interrupting run is the latest
  one. Months holding an interrupted check (the organization's or a stored
  account's) join every run in `Changes` mode, as gaps do, until a run finishes them;
  if nothing changed that costs the month's N + 3 calls.
- Calls, with N stored accounts, A accounts with usage in a month and D days: an
  hourly run costs N + 3 per month, plus the open days of each account whose month
  changed; a nightly run N + 3 + A × D + D per month; the first backfill a nightly
  run's worth per month.

**Coverage and freshness.** A source is imported through the latest UTC day whose
expected slices, and every earlier day's, were fetched after their day ended. The
settle lag is the longest late change, in days after a slice's period ended, over the
last 62 days. The import is *stale*, with the first reason, when the latest finished
run failed, the last success is more than 3 hours old, account credits, organization
credits or organization usage are imported through a day more than two days before
today, the current month's summary is more than 3 hours old, or an account that held
a seat in the month could not be imported, or a run stopped before finishing an
account's month (interrupted, above). An account's own errors without a seat that
month (departed people, such as a deleted login GitHub answers 404 for) are listed
but never make the import stale: that account's slices of that month are not
expected, so they neither hold coverage back nor make the month a gap that every run
imports again. Account errors count while they are from the latest finished run, or
from an interrupted run started since; a later run that imports other months ends
their staleness, not their flag or import gap. Stale imports show on the
reconciliation tab and the developer card and log a warning each hour; nothing else
is notified. The reason
reaches the page as data (`staleReason`: a `kind` with its instant, UTC day,
source, login, month or error), which the page phrases with instants in the
browser's time zone. Account import errors are listed per account and month, with
whether the account held a seat and whether a stopped run left them (`seated`,
`interrupted`).

**Reconciliation**, per UTC month (`domain/models/ai_metered_reconciliation.rs`):

- `T`, tracked Copilot, is the priced estimate of usage Copilot paid for
  (`COALESCE(billing_provider, provider) = 'copilot'`, personal keys included,
  Copilot routed to Codex excluded) of the user an account's disposition links on
  each UTC day; credits are dollars × 100. Unpriced usage carries no weight,
  and neither do free models: whatever Toki priced them at is left out of T, the
  old pricing basis and tracked without a linked account, and their groups are
  listed with `T = 0`.
- *On an old pricing basis* is the part of T from buckets at or after the reprice
  floor (`ai_usage.reprice_floor`, raised to the start of the first open month, as for
  the bill and for discovery) whose basis is NULL or
  differs from their own machine's
  current pricing basis (`ai_usage_machines.pricing_basis`, the latest non-null basis
  of its uploads and repricings in the order they were applied, over all of the
  machine's providers): what the reprice discovery with that basis would report as
  stale. Buckets before the floor are never repriced and are reported apart. An
  upload without a basis leaves the machine's current basis alone, so it never turns
  repriced history old; only its own NULL-basis buckets count as old. A machine that
  never sent a basis has none, so all of its usage from the floor on reads as old. A
  machine that never upgraded its rates is compared with its own current basis.
  Month close requires no contributing bucket on an old basis as read here, and no
  billed estimate on one (readiness `pricing`); bases are opaque, so machines are not
  compared with each other.
- Per account, UTC day and group of (SKU, model) on linked days, `attributed =
  min(G, T)` and `untracked = G − attributed`. Untrackable and unrecognized credits
  have `T = 0`; code review credits are left out (repository-billed). Credits on days
  without a disposition are unlinked, on External days external, and a user's T on
  days none of their dispositions covers is listed as tracked without a linked
  account. `gross = repository-billed + attributed + untracked + unlinked +
  external` per account. The tracked share is attributed over trackable G on linked
  days (cloud agent and code review left out).
- An account day whose slice was never fetched (the first hour of a UTC day, an
  account listed after the last run, a run that stopped early) has no G to compare
  with: it counts as a day not imported, and the user's T on it is left out
  rather than read as GitHub billing nothing.
- Flags: *tracked, GitHub billed nothing* (T of day-groups with G = 0, at least
  100 credits in the month; free models have no T), *G/T below 0.85* (any day-group
  with G > 0 and `G × 100 < 85 × T`, however small; a group used only through Auto sits
  at 0.9 and is not flagged); both weigh settled days only, whose account slice is
  complete and which lie before the settle days, so a day GitHub is still posting
  never raises them. Then old pricing basis, unknown model (an unrecognized row,
  or a model outside the catalog with credits or tracked cost), unlinked, external,
  a stored month that differs from GitHub's month total, and a failed import of an
  account that held a seat, or an interrupted one of any account.
- Repositories: the organization's rows summed per product, SKU, unit and
  repository, with the IDE aggregate flagged, and the code review check: per
  reconciled UTC day, code review credits per repository (the organization's
  repository credit rows) minus those per account (repository-billed groups),
  zero within 100 nanodollars, with the month's totals (`codeReview: {
  byAccounts, byRepositories, days: [{ day, byAccounts, byRepositories,
  difference, status }] }`). A day's status is `incomplete` when the
  organization's usage slice or an expected account slice is missing, a
  seated account's import failed, or a run stopped before finishing an
  account's month (interrupted, whatever its seat); `provisional` while a
  slice is not complete or the day is within the settle days; else `settled`.
  A negative difference (accounts exceed repositories) is code review GitHub
  billed without a repository, inside the IDE aggregate, which bills on no
  line, or code review in an unrecognized row naming a repository (a SKU or
  unit Toki does not know). A positive difference (repositories exceed
  accounts) is code review by accounts Toki does not import (unknown
  accounts): their "Code Review model" credits are never imported and stay out
  of the unknown account's line, but their repository rows bill it once, so
  nothing is wrong with the bill. On an `incomplete` day it may also be an
  account whose slice is missing or whose month a run left half imported. The
  check is informational: it never makes a line provisional or blocks anything.
- The per-day residual is the organization's credits minus every account's, per day
  and per (SKU, model), zero within 100 nanodollars: unknown account. It rests on
  the organization's stored days, so the organization's check counts: until its
  stored month equals GitHub's month total (a usage post can land between the month
  call and the day calls), and before its first check, no day settles; a failed
  check makes every day of the month an import gap. A day missing the
  organization's slice or an expected account slice, or any day of a month with a
  failed seated account or an interrupted account, is an import gap too; a day not
  yet complete or within the settle days is provisional. A negative residual
  (accounts above the organization) is flagged. The month's response carries the
  organization's check (`orgCheck`), shown with the residual.
- The pool card reads the month summary: credits with the pool discount (gross,
  discount, net), seats, GitHub's bill so far (every net amount) and the day the pool
  ran out, the first UTC day whose organization credits have a net quantity.

**Routes**, browser-only under `/ai-usage/admin`, admin power, off the agent OpenAPI
document and MCP: `GET /metered-usage/{month}` (the month, every account),
`GET /metered-usage/{month}/developers/{userId}` (one user's linked days, no
residual, organization check, repositories or pool; 404 for an unknown user) and `POST
/metered-usage/{month}/import` with `{ "mode": "changes" | "full" }`, which answers
`202` with the run id and runs on its own task. A malformed month or body is 400, a
month before the backfill's or after today's 400, a running import 409 (and so is a
month close reading the usage), a source that is off or misconfigured 503, GitHub's
rate limit 429 and other GitHub failures 502. Credits are strings with two decimals
(hundredths of a credit); only the residual and the unknown-account totals can be
negative.

**Repository names** come from GitHub's billing, which admins see in GitHub; they are
not Toki project keys, so the rule that unmapped project keys are never shown does
not apply to them.

**Checking against saved responses.** To check the parsing against real GitHub
answers, save them in a local directory, never committed (`<login>_d<day>.json`,
`<login>_month.json`, `org_d<day>.json`, `org_month.json`, `org_usage_d<day>.json`),
and run the oracle, which prints booleans and counts only:

```bash
TOKI_METERED_ORACLE_DIR=/path/to/responses SQLX_OFFLINE=true \
  cargo test -p toki-api -- --ignored --nocapture metered_import_reproduces_the_local_raw_files
```

## GitHub Copilot lines in the bill

From the metered cutover on, the bill takes Copilot's lines from GitHub's own
credits, $0.01 each, split by what Toki tracked (`domain/models/ai_metered_billing.rs`,
`allocate_metered_credits`, pure). Earlier months keep Toki's API-estimate lines.

**Cutover and rollback.** `ai_usage.metered_copilot_from` (`YYYY-MM`, `2026-10` in
`base.yaml`) is the first month billed from credits. Unset or empty keeps every month
on estimates: remove the key, or set `TOKI_AI_USAGE__METERED_COPILOT_FROM=""` (the
config crate keeps empty environment values), to roll back. Any other value that is
not `YYYY-MM` stops startup. A cutover whose first day precedes
`ai_usage.metered_import.backfill_from` only logs a warning: its days before the
backfill show as not imported. Deploy after the import has backfilled the cutover
month; until then its Copilot usage is "not imported" (provisional).

**Metered buckets.** A bucket is metered when Copilot pays for it
(`COALESCE(billing_provider, provider) = 'copilot'`: native Copilot, and OpenCode or
Pi routed through Copilot) and its hour starts at or after the cutover month's first
day at 00:00 UTC. Copilot routed through ChatGPT (paid by Codex) is never metered.
The local-day reads (`daily_usage`, `personal_usage`, and the local rows of
`developer_usage`) leave metered buckets out, and the UTC read (`tracked_usage`)
feeds the bill of every month key from the cutover on, so every bucket bills exactly
once. The cutover is an instant, not "the month's buckets": with a 2026-10 cutover,
Copilot hours of Sept 30 22:00–24:00 UTC fall in Stockholm's October but UTC's
September, which is not metered, so they keep their API-estimate line in October's
bill. Without a cutover the reads are exactly today's.

**Months.** A Copilot line belongs to the UTC month of GitHub's day; every other
provider keeps the months of `ai_usage.time_zone`. Both share the month key
(`YYYY-MM`) and with it the month's exchange rate. The period response reports both
bases: `timeZone` and `meteredPeriod: { provider, from, applies, timeZone: "UTC" }`
(null without a cutover).

**The T/G rule**, per account (through its disposition), UTC day and group of (SKU,
normalized model), "Auto: X" counting as X (`split_group`): `G` is the group's gross
credits in dollars and `T_d` Toki's Copilot-priced estimate per destination.

| Case | Result |
|---|---|
| T ≥ G | G is split across destinations by T's weights (`allocate_largest_remainder` in nanodollars, ties to the earlier destination) |
| T < G | Each destination gets its T_d; G − T is untracked Copilot |
| G = 0 | Nothing is billed |
| Only unpriced records, or free models | No weight: all of G is untracked. No token or record fallback |

No destination is ever charged more than its own T_d. A settled day-group with
`G × 100 < 85 × T` flags its lines (`lowRatio`) instead of being scaled silently; a
group used only through Auto sits at exactly 0.9.

**Groups that never use Toki's weights.** Code review credits ("Code Review model",
repository-billed) are left out of the account lines: GitHub bills the same
credits per repository too, and the bill takes them from the organization's rows,
per repository (see "Repository mappings"), so they bill once.
The cloud agent SKU and unrecognized credits have T = 0, so they bill as the user's
untracked Copilot on their SKU. Credits on days without a disposition bill as the
account's "unlinked account" line, and on External days as its "external account"
line; linking the account moves the amount.

**Destinations.** A user's tracked usage goes to the project its key resolves to (as
for the bill: only mappings to the configured company), to Unassigned (unmapped keys,
and `unattributed`), or to personal/internal (keys the user marked, included and
shown apart). Project and Unassigned destinations bill as project lines, Unassigned
as today in the Unassigned entries, which mapping fixes for unmapped keys; the
statement counts unattributed Copilot as Spinit's. Personal/internal bills as a
Spinit line with an amount, untracked Copilot as Spinit overhead next to unallocated
overhead, and unlinked, external and unknown accounts as Spinit lines.
`tracked_usage` rounds a user's day and model as one: each destination gets the
rounded running total through it less the one before it, so the destinations sum to
the model's rounded cost and reconciliation keeps its figures. Key names never leave
the adapter.

**Rounding.** Per account and month, the billed G (gross less code review) is rounded
half up to cents once and split over the account's lines with
`allocate_largest_remainder`, weighted by their nanodollar allocations, in a fixed
order (projects by name, then Unassigned unmapped, unattributed; personal; untracked;
unlinked; external; each by user and SKU). Each account's lines therefore sum exactly
to its GitHub gross. A project or personal line is then capped at its tracked cost
over groups with credits, rounded down to cents; the excess cent moves to the same
user's untracked line on the destination's SKU. Rounding is Spinit's, so
`charge_d ≤ T_d` holds in cents too, for every destination. SEK conversion is per line, as for API lines.

**Unknown account.** The month's residual of non-repository credits (the
organization's credits minus every account's, per UTC day, as on the reconciliation
tab) over settled and provisional days bills as one "unknown account" line when it is
positive; zero or less gives no line. Code review is left out on both sides, so the
line may differ from the tab's unknown-account total by the code review residual. The
tab already flags the days.

**Provisional.** All of an account's lines share its reasons (`provisionalReasons`):
`monthInProgress` (the UTC month has not ended), `importIncomplete` (a day of the
month has no slice or an incomplete one, a run stopped before finishing the account's
month, whatever its seat, so its days may be half replaced, or days before the
backfill; excused accounts, whose own import failed without a seat, aside),
`notSettled` (a day lies within the settle days, or the account's month total is
absent, failed, interrupted or different), `oldPricingBasis` (a contributing bucket
in a group with credits is on an old pricing basis, as the reconciliation tab reads
it: NULL or another basis than its machine's current pricing basis),
`provisionalModel` (a model the catalog marks provisional with priced tracked usage
in a group with credits; G is GitHub's number, so a provisional rate only moves a
split) and `unrecognized` (a credits row priced otherwise than $0.01 a credit, or a
model outside the catalog). The unknown-account line is provisional while the month
is in progress, a day is an import gap, or a day is provisional. The bill's `metered`
block is provisional when a line is, when an account has reasons though it bills no
line (a day never imported, a month total that differs, days before the backfill),
when tracked usage is not imported, when an account day is missing, or when the
unknown account's residual is provisional even though it bills no line
(`AiMeteredAllocation::unknown_account`): a failed organization check, a failed
seated or interrupted account check, or an organization slice that is missing,
leaves its days out of the residual, and credits not imported or a residual not
computed yet are no settled zero. The same holds for repository credits
(`repository_provisional`, see "Repository mappings"). A developer's `metered` block leaves the unknown
account out, as it is no developer's, and is provisional over what is theirs: a line
of theirs, a day of theirs not imported, their tracked usage not imported, or an
account linked to them on a day of the month that has reasons, whether or not they
have a line of it (GitHub may still bill a linked account Toki tracked nothing for,
such as the cloud agent's). So a developer's month is provisional only when the
overview's is.

**Subjects and charge kinds.** A billing line's subject is a user, a provider account
(`{ provider, accountId, login }`: unlinked or external), the unknown account, or a
repository (`{ provider, name }`: code review). A metered line's kind is `project`,
`personal`, `untracked`, `unlinkedAccount`, `externalAccount`, `unknownAccount` or
`repository`. Lines follow users, then provider accounts by login, the unknown
account, and repositories by name, case-insensitively.

**HTTP.** Lines carry `subject` (instead of `userId`), `billingMode` (`api`,
`subscription` or `metered`) and `metered` (non-null exactly on metered lines: `kind`,
`credits` as whole credits in hundredths, `"1235.00"`, `account`, `sku`,
`provisional`, `provisionalReasons`, `lowRatio`). Totals add `meteredBilledUsd` and
`converted.metered`; `apiBilledUsd` stays API lines only. The bill's `metered` block
(`provider`, `provisional`, `daysNotImported`, `lowRatioGroups`, and `unbilled`
usage: `trackedUnlinked`, `notImported`, `freeModels`) is null unless the month is
billed from credits; a developer's month has the developer's figures. The routes stay
browser-only under `/ai-usage/admin`, off the agent OpenAPI document and MCP.

**CSV.** These columns follow `unassigned`, in order: `source` (`estimate`,
`subscription` or `github`), `credits`, `github_login`, `repository` (a repository
line's name), `sku`, `period_basis` (`UTC` for metered rows, the configured zone
otherwise), `adjusts_month` and `snapshot_id` (an adjustment row's corrected month
and that month's close; a closed month's own rows carry its own close in
`snapshot_id`; see "Month close, snapshots and adjustments") and `provisional`. Spinit's metered rows name their project `Untracked Copilot`, `Personal
/ internal Copilot`, `Unlinked account`, `External account` or `Unknown account`. A
repository row has its project, or `Unassigned` with `unassigned =
unmapped_repository` and the warning "repository without a project: map it under
Project mappings > GitHub repositories".

**The developer page** allocates every account and keeps the developer's lines
(`for_user`), so it agrees with the overview to the cent. Its daily rows add
`billingMode`; metered rows are the developer's metered buckets on UTC days, after the
local rows of the same date, personal keys left out as before.

**Unbilled usage.** Every tracked row of the UTC month lands in exactly one place: a
free model's goes to `freeModels`; a day outside the import's coverage, or a linked
day whose account slice is missing, to `notImported`; a day no disposition links to
`trackedUnlinked`; any other is a line's usage.

**Agreement with reconciliation.** The bill and the reconciliation tab walk the month
once per account and day through the same code (`walk`, `split_group` in
`ai_metered_reconciliation.rs`), so they agree on G, T, attributed and untracked
(and unlinked and external) for every account. The allocation tests check it on the
lines themselves: per account, the nanodollars of its project and personal lines are
the tab's attributed, its untracked, unlinked and external lines the tab's figures,
and its credits the tab's gross less code review, rounded once. The tab says, from
the cutover month on, that the bill's Copilot lines follow its attribution.

**In the admin UI** (`app/src/routes/_layout/admin/ai-billing/`, contracts in
`app/src/lib/api/contracts/ai-admin.ts`, strict, so backend and frontend ship
together). The statement adds a "GitHub Copilot" figure (`converted.metered`, its
credits at $0.01 and the UTC month, a provisional mark) beside "API usage", which keeps
API lines only, and notes tracked usage on no line yet (`notImported`,
`trackedUnlinked`). Its sentence counts Spinit's credits apart: untracked, personal,
account lines, and metered Unassigned usage outside any repository, which the ledger
keeps under Unassigned. The ledger groups Spinit's lines after unallocated overhead
(`spinit:untracked`, `spinit:personal`, `spinit:accounts`, by subject: an account's
rows show its login and link to GitHub logins), and the strip draws untracked Copilot
as one solid muted segment and the rest of Spinit's merged, the credits outside any
repository included; the other usage outside any repository stays a hatched
Unassigned segment. Sentence and strip split Unassigned the same way
(`statementUnassigned`), so their figures agree. Metered lines read
"GitHub Copilot · N credits", naming the SKU only when no project line bills it (the
cloud agent's). People show "Copilot credits" under Paid by and, in months billed
from credits, an Untracked column: the untracked Copilot within a developer's Billed,
which is Spinit's. The developer page's chart stacks by payer (`stackBy=payer`, the default)
or tool, and labels metered days "GitHub Copilot credits"; a day and tool whose rows
bill in more than one way stays "Mixed billing" whatever order its rows come in. A
developer with synced usage but no line yet (Copilot usage whose credits are not
imported, or on days no account is linked to them) still gets the usage by day, with
a note on what it waits for. "How billing works" and the GitHub Copilot tab read
`meteredPeriod`.

**For month close.** One user can have lines of two accounts in a month (an owner
change), with the same subject, project, kind, currency and provider. Month close's
diff key sums such lines (see "Month close, snapshots and adjustments").

## Repository mappings (code review credits)

GitHub bills Copilot code review twice: per account, under the model label "Code
Review model", and per repository, in the organization's "Copilot AI Credits" rows.
Both are the same credits. From the metered cutover on, the bill takes them once, per
repository: the per-account groups stay out of the account lines and out of the
unknown account's residual (above), and each repository's credits bill to the
project its repository mapping names, or to Unassigned (unmapped repository).

**Mappings** (`ai_provider_repository_mappings`,
`20261029120000_add_ai_provider_repository_mappings.sql`): one per payer and
repository, pointing directly at a time-tracking project, stored as project-key
mappings store theirs (the project's time-tracking provider and company, its id, and
its name when last saved) with their creator and last editor. So an admin can map a
repository nobody mapped as a project key. Names are GitHub's bare repository names,
1 to 200 characters, kept as GitHub spelled them and compared case-insensitively
(a unique index on `(provider, lower(repository_name))`); the stored spelling is the
payer's latest. Mappings have no validity dates: they resolve whenever a bill is read
and are never written into usage, so a change applies to every open month, as key
mappings do; a closed month keeps its snapshot and gets adjustment lines on the next
open month's bill. Only mappings to the configured company resolve; a
stale or unconfigured one bills as unmapped. GitHub names a repository without its
owner, so two repositories of one name under different owners are one repository to
its rows: one line and one mapping.

**Suggestions** come only from project-key mappings that resolve
(`AiMappingResolver::resolved_keys`): a key `{host}/{owner}/{name}`, exactly three
segments with any non-empty owner, whose host is the GitHub adapter's
`REPOSITORY_KEY_HOST` (`github.com`) and whose name is the repository, both compared
case-insensitively, suggests its project. They never come from unmapped keys, which
may name a private repository, nor from usage or personal marks: the only key data
read is the stored mappings. Suggestions are grouped by project, most keys first,
and left out once the repository's mapping resolves.

**Lines** (`repository_lines` in `domain/models/ai_metered_billing.rs`):

- Only the organization's rows classified as repository credits bill: "Copilot AI
  Credits" at $0.01 a credit with a non-empty repository. The IDE aggregate (no
  repository), the cloud agent's rows, Actions (compute, imported for
  reconciliation only) and unrecognized rows never do. Code review GitHub billed
  without a repository is inside the IDE aggregate and bills on no line; the daily
  code review check shows it as accounts exceeding repositories. The other way
  round, repositories exceeding accounts, is code review by accounts Toki does not
  import: their per-account credits are never imported, and their repository rows
  bill it once, here. An unrecognized row naming a repository may be code
  review under a SKU or unit Toki does not know, so it is flagged, never dropped
  silently: it makes repository credits provisional (`unrecognized`).
- Per repository and UTC month (the month of GitHub's day), the rows' credits are
  summed and rounded half up to cents once. A repository whose credits round to no
  credit gets no line, so nothing waits for its mapping. The mapping overview marks
  each month the same way (`AiRepositoryMonthCredits::billed`).
- A line's subject is `repository` (`{ provider, name }`, the latest spelling), its
  metered kind `repository`, its SKU the rows', with no account and no tracked
  usage. It bills to the mapped project, or to Unassigned with `unassigned:
  unmappedRepository`. On the bill it reads "Copilot code review · {name}".
- Lines exist only from the cutover month on, where `allocate_metered_credits` runs.
- Provisional reasons, shared by every repository line and kept without lines
  (`AiMeteredAllocation::repository_provisional`): `monthInProgress`,
  `importIncomplete` (a day without the organization's usage slice, or with one not
  fetched after the day ended, or days before the backfill), `notSettled` (a day
  within the settle days) and `unrecognized` (an organization row of the UTC month
  naming a repository is unrecognized). A missing usage slice is no settled zero: it keeps the
  bill's `metered` block provisional even without repository rows. They rest on the
  organization's usage slices alone, which a run replaces a month at a time, so no
  check bears on them: a month a run stopped importing (an interrupted check, the
  organization's or an account's) leaves them as they are and keeps the `metered`
  block provisional through the unknown account's residual instead. One developer's
  part has no repository lines.
- `AiMeteredAllocation::unmapped_repositories()` names the repositories billing to
  Unassigned, with their credits: what month close refuses (readiness
  `repositories`). An empty repository name never blocks.

**Port and adapter.** `AiRepositoryMappingService` (inbound) and
`AiRepositoryMappingRepository` (outbound, `PostgresAiRepositoryMappingRepository`)
manage mappings; the bill reads them through
`AiMeteredUsageRepository::repository_mappings` and resolves them with
`AiRepositoryResolver`, loaded by `AiMeteredMonths` with the rest of the month.

**Routes**, browser-only under `/ai-usage/admin`, admin power, off the agent OpenAPI
document and MCP:

- `GET /repository-mappings`: `{ provider, billedFrom, months, repositories }`, the
  previous and current UTC month, and a row for every repository GitHub ever billed
  and every mapping: `{ repository, months: [{ month, credits, billed }],
  lastBilledOn, mapping, suggestions: [{ project, projectKeys }] }`. `billed` says
  whether the month's credits bill a line, as `repository_lines` decides: the month
  is on or after the cutover (`billedFrom`) and its credits, rounded half up to
  cents once, come to at least one credit. A mapping is `{ repository, project,
  status (resolves | stale | unconfigured), createdBy, createdAt, updatedBy,
  updatedAt }`. Rows with a billed month and no resolving mapping
  (`AiRepositoryMappingRow::needs_mapping`, exactly the repositories
  `unmapped_repositories()` names for that month) come first, then by name.
  Credits before the cutover, or under one credit for the month, bill nothing and
  never put a row first.
- `PUT /repository-mappings` with `{ repository, projectId }`: creates or replaces
  the mapping, matched case-insensitively. A new mapping needs a repository GitHub
  billed at least once (else 404); an existing one can always be changed.
- `DELETE /repository-mappings` with `{ repository }`: 204, or 404 without one.
- An invalid name, an unknown field or an unknown project is 400; time tracking not
  configured or its projects unavailable 503.

**In the admin UI**, Project mappings has a "GitHub repositories" section
(`#github-repositories`, `admin/-components/repository-mappings.tsx`, contract
`app/src/lib/api/contracts/ai-repository-mappings.ts`, strict) with each repository's
credits per month (greyed, with the reason as their title, where they bill no line:
before the cutover or under one credit for the month), its project and status ("Not
mapped" is a warning only while a billed month bills as Unassigned), suggestions to
accept ("Map to {project}", titled with the mapped keys; only those of an active
project, since a key mapping keeps its project once archived and a repository maps to
active projects only) and a project picker; a link with that hash scrolls to it once
the page has loaded. Repository lines read "Copilot code review · {name}" on the bill
and link to the section; unmapped ones sit in the ledger's Unassigned entry
"code review of unmapped repositories", between unmapped keys and usage outside any
repository, with a "Map GitHub repositories" button, and the statement counts them as
having no project yet, never as Spinit's. The ledger's People column counts people,
accounts and repositories apart (`subjectsLabel`). The export's Projects check needs
attention for them, as for unmapped keys. The GitHub Copilot tab's code review check
reads "Code review adds up" when every day's difference is zero, else "Code review
differs on N days" with those days, their status and a note for each direction
present: a day where accounts exceed repositories is code review billed without a
repository, or, when an unrecognized row names a repository, may be code review in
those rows; a day where repositories exceed accounts is code review by accounts Toki
does not import, which bills once, through its repository line. Only the first is a
warning; days that differ only the second way are marked informational.

## Month close, snapshots and adjustments

An admin closes a month's **whole AI bill** (API lines, subscription fee shares and
overhead, metered Copilot lines, and the adjustment lines posted on it) once the month
is over, and from then on the month is served as it was invoiced. A later change to a
closed month bills exactly once, as **adjustment lines** on the next open month.

**Closes** (`ai_billing_month_closes`, `20261030120000_add_ai_billing_month_closes.sql`):
one row per close with scope `ai_bill` (`AI_BILL_SCOPE`), who closed it and when, the
readiness as evaluated (`readiness_json`, with each waiver's reason), the snapshot's
JSON and CSV header, and `projection_version`. A month has at most one *active* close
(a partial unique index). Re-opening keeps the row as history (`reopened_at`,
`reopened_by`); every computation reads active closes only.

- **Closable:** the month is over in UTC and in `ai_usage.time_zone`, by the store's
  clock, and has no active close. Otherwise `409` (not over yet, with when it will be;
  already closed). There is no order: any month that is over can close, and the
  netting below stays exact whichever months are closed or re-opened.
- **Re-open** is refused (`409`, `heldBy`) while an active close holds adjustments to
  the month (`adjusts_month`). Re-opening a month whose snapshot froze adjustments to
  earlier months makes those live again, on the new first open month after them.
- One close or re-open runs at a time (an advisory lock); a second one meanwhile is
  `409`.
- A close of a metered month (on or after the cutover) holds the payer's imports off
  from its first read of the month to the stored snapshot
  (`AiMeteredUsageRepository::try_hold_imports`, the import lock taken shared), and
  is refused (`409`, "a usage import is running: try again when it finishes",
  whatever is waived) while any import of the payer holds the lock, whatever month
  it imports: the month and the closed months posting on it all read the payer's
  usage, and an import that started after the close's first read could otherwise
  replace days between its reads, so readiness and the snapshot would judge and
  freeze a half-replaced month. The hold is taken after the close lock and only
  once the month can close (not over or already closed answer first). Both locks
  are tried, never waited for, so no order can deadlock; a close holds two pooled
  connections in open transactions (the two leases) besides its reads. Re-open
  reads no usage, so it takes no hold, and nor does an open month's live bill or
  its adjustment lines: nothing they read is kept, and the next read sees the
  finished import.

**Readiness** (`domain/models/ai_close_readiness.rs`), evaluated by the server in `GET`
and again in `POST …/close`. Each check passes, fails, waits for nothing it does not
apply to (`notApplicable`), or is waived: an admin waives a failed check with a reason
(trimmed, 1 to 500 characters), kept in `readiness_json` with every check's problems.
A waiver of a check that did not fail is dropped. Anything waived wrongly is still
corrected later, by an adjustment. The one problem no waiver covers is `importRunning`
(`AiCloseProblem::waivable`): a waiver of an `import` check holding it is dropped, so
the check stays failed until the run finishes, since a close meanwhile would freeze
days the run may have half replaced. Every other check, and the `import` check once
the run is over, stays waivable. `GET` shows it for a run whose plan holds the month;
`POST` never gets that far while any import runs, since the close is refused before
its first read (above), and the readiness it evaluates under its hold on imports never
sees a live run.

1. `import` (metered months): judged per account and month from the import's stored
   slices and checks, never from a run's errors, which only describe that run. No live
   run is importing the month (`importRunning`: the running run's plan holds it, so it
   may be replacing the month's days right now; it cannot be waived); the month's last
   day is before today less the settle days (`AiImportStatus::settle_days`, the measured
   lag, at least 3); every day of the month is imported (days before the backfill fail)
   with every account's, the organization's credits and the organization's usage slices
   complete; and the latest check of every account that held a seat, of every account a
   run left interrupted (whatever its seat: its days may be half replaced, its credits
   included), of every other account with credits or lines that is not excused (its own
   import failed without a seat), and of the organization is a success that matches
   GitHub's month total, made on or after the day the month settled. Checks are read as
   the bill reads them (`AiMeteredMonths::load` passes the live run to `checks`): a mark
   of the live run keeps the check it marked, which `importRunning` fails instead, while
   a mark a stopped or dead run left is an interrupted check (outcome `interrupted`, with
   its error, beside the import's own `failed`). Passing it means no metered line,
   unknown account or repository line may still change for the import's sake
   (`monthInProgress`, `importIncomplete`, `notSettled`).
2. `dispositions` (metered): no credits bill to an unlinked account; problems summed per
   account.
3. `unknownAccount` (metered): the unknown account's residual is settled
   (`AiMeteredAllocation::unknown_account` empty) and zero on every settled or
   provisional day (`unknown_residual`, signed: a negative day is accounts above the
   organization), code review left out on both sides as for the bill.
4. `repositories` (metered): no repository bills to Unassigned
   (`unmapped_repositories()`), and no organization row naming a repository is of a kind
   Toki does not recognize (it may be code review on no line).
5. `pricing`: no metered account's lines rest on an old pricing basis or a provisional
   model (per account), and no bucket the local-day bill reads (the same days, metered
   exclusion and personal keys as `daily_usage`) from the reprice floor in force on (the
   configured one, raised to the start of the first open month) is on an old basis (per machine and
   provider, with local days and its estimate): a bucket no reprice may change is taken
   as it is. Closing
   freezes the estimates API lines bill and fees are split by, which a later reprice no
   longer reaches. Unrecognized models never block: GitHub's credits are its own number.
6. `exchangeRates`: every currency the bill converts has a stored rate that is not
   being fetched and is final (an admin's override is final), and every adjustment
   line's corrected month has a final rate and a known SEK amount.
7. `uploads`: every machine active in the month uploaded all of it (completeness), and
   in metered months also the UTC hours of the month outside its local days (after the
   local month east of UTC, before it west of UTC), for the machine and each expected
   provider. Problems name the span, never when a machine synced. Inactive machines and
   developers without machines never block.

**Snapshot** (`ai_billing_snapshot_lines`): one row per line as its readers saw it (one
per Unassigned kind, as `AiBillingLine::parts`), own lines first, then the adjustment
lines posted on the month, each with its diff key (text columns), display names,
`amount`, `sek_amount`, `rate`, `credits`, the line's JSON (`line_json`), its CSV cells
(`csv_row`, raw, guarded when served) and CSV position. The close row adds the
overview without lines, adjustment lines and close (`bill_json`, with `adjustedMonths`),
each developer's bill part as the developer page shows it (`developer_json`:
`subscriptions`, `totals`, `metered`, and their own `missingRates` and `pendingRates`,
per user id, for every developer with lines, subscriptions, adjustment lines, metered
usage on no line, or a metered part kept provisional by a linked account that may still
change (`AiMeteredAllocation::user_ids`); any other developer is served an empty,
settled part, as the live page shows it), the CSV header and the readiness.
The projection comes from the HTTP adapter through `AiBillProjector`, which the close
handler passes to the service, so the domain never imports the adapter.

- **Serving a closed month:** the overview is `bill_json` with the stored lines and
  adjustment lines and a non-null `close` (`closeId`, `closedAt`, `closedBy`,
  `adjustmentsPostOn`, `currentExchangeRates`: the month's rates as stored now, while
  `exchangeRates` keeps the ones it closed with). The developer page takes period and
  rates from `bill_json`, its bill part from `developer_json` (an empty one without
  any), its own and adjustment lines from the snapshot, its personal usage from the
  developer's entry of `bill_json`'s `personalUsage` (null without one), as the overview
  serves it, and only its usage by day and machines live: a key marked personal after
  close leaves the usage by day, while the bill and the personal usage stay as closed
  and the change posts as an adjustment. The CSV is the stored header and rows. A closed
  month serves the same JSON values (JSONB may reorder keys) and the same CSV bytes as
  when it closed; its `pendingRates` are frozen, so the page need not poll.
- **Versioning:** `projection_version` is `AI_BILL_PROJECTION_VERSION` (1) when stored.
  A change to the served overview, line, adjustment line or developer JSON bumps it and
  adds an upgrade step from the previous version in the adapter (`upgrade_snapshot`),
  with a test, so the strict frontend contracts keep reading old closes. A stored
  version newer than the code is a `500` that logs it.

**Diff key** (`AiAdjustmentKey`): subject, project, kind, currency and payer.

| Part | Text | From |
|---|---|---|
| subject | `user:{id}`, `account:{provider}:{accountId}`, `unknown:{provider}`, `repository:{provider}:{lower-cased name}` | the line's subject, by what survives renames |
| project | `project:{id}`, `unassigned:{unmapped\|unmapped_repository\|unattributed}`, `none` | the part's project or Unassigned kind; `none` for overhead and Spinit's metered lines |
| kind | `api`, `subscription`, `overhead`, `metered_project`, `metered_personal`, `metered_untracked`, `metered_unlinked_account`, `metered_external_account`, `metered_unknown_account`, `metered_repository` | the charge |
| currency | ISO code | the billable currency (USD for API and metered lines) |
| payer | the subscription for `subscription` and `overhead`, else the line's provider | "provider or subscription" |

Lines and parts that share a key are summed: one user's lines from two accounts in a
month (an owner change), a user's untracked lines of two SKUs, the Codex and Pi shares
of one subscription (named after the subscription's provider).

**Adjustments** (`domain/models/ai_month_close.rs`). For the open month M and each
closed month K whose first open month after it is M, per key:

`adjustment = R − S − F`

where R is K's own bill recomputed now (the local-day bill plus metered lines from the
cutover on, converted at K's current rates), S is K's snapshot (its own lines) and F the
adjustments to K frozen in **every active close**. An unknown amount counts as zero. A
line posts unless both its amount and its SEK difference are zero.

- **SEK at K's rate:** an adjustment's SEK amount is `R.sek − S.sek − F.sek`, the
  difference of what is invoiced in SEK, never its amount converted; it carries K's
  current rate, bypassing `convert_bill`'s same-month filter. A rate change alone (an
  admin's override on closed K) changes only `R.sek`: an SEK-only line, `billableAmount
  "0.00"`, `sekOnly: true`. SEK can also move alone at an unchanged rate, as a key's SEK
  is a sum of per-line conversions or a fee's own SEK split: `rateChanged` says whether
  K's current rate differs from the one the key was last invoiced at (its latest frozen
  adjustment's, else its snapshot's), and only then do the CSV and the page name a rate
  change. A stored row without an SEK amount counts as nothing
  invoiced in SEK; a recomputed part without a rate leaves the difference unknown
  (`missingRate`), which readiness `exchangeRates` catches. SEK keys have no rate.
- **Why every active close, not the plan's "closed months before M":** that wording
  double-counts once a month between K and M is re-opened (K and K+1 closed, K+2 holding
  A to K: re-opening K+1 makes it the first open month, and no close before it holds A).
  What is invoiced for K is always `S + F + A_live = R`: closing M moves `A_live` into F
  unchanged, and re-opening a month removes its rows from F, which reappear live. So
  each correction bills exactly once, through any sequence of closes and re-opens.
- **Display:** subject, project and provider come from the latest source (R, then S,
  then F); a subscription's plan from the subscription while it exists. Adjustments
  have no usage.
- **Cost:** an open month's overview, CSV, developer page and close status recompute
  every closed month that posts on it, sequentially (`ai_billing.adjustments` span);
  normally one month.
- **Config changes re-bill closed months:** changing `ai_usage.metered_copilot_from` or
  `ai_usage.time_zone` after a close changes the recompute, so the difference posts as
  adjustments on the next open month.

**On the bill**, adjustment lines are kept apart from `lines` end to end:
`AiMonthBill::adjustments`, JSON `adjustmentLines` (`adjustsMonth`, `snapshotId`,
`subject`, `provider`, `project`, `unassigned`, `kind` of `api`, `subscription`,
`overhead` or `metered` with `meteredKind`, `subscriptionId`, `plan`, signed
`billableAmount` in `billableCurrency`, signed `convertedAmount` with `exchangeRate` and
`missingRate`, signed `credits` for metered kinds, `sekOnly`, `rateChanged`), ordered by
corrected month, then project as the CSV ranks them, project name, Unassigned kind,
subject, provider, kind and payer. `adjustedMonths` lists the closed months posting on the month,
with or without lines. `totals.adjustments` sums them per currency; `totals.converted`
adds `adjustments` and `invoiced` (`billed` plus `adjustments`; null when either is).
Every other total, and `billed`, stays the month's own lines. A developer's month
carries the developer's adjustment lines.

**CSV:** adjustment rows follow the month's own rows: `billing_mode = adjustment`,
`source` as the corrected kind's (`estimate`, `subscription` or `github`), the project
columns as for that kind, the carried plan, signed `billable_amount` and
`billable_sek`, the corrected month's rate, zero usage columns, signed `credits` for
metered kinds, the developer of a user subject, the `github_login` of an account subject
and the repository of a repository subject, `period_basis` `UTC` for metered kinds,
`adjusts_month` and `snapshot_id` (the corrected month's close), and a warning naming the
corrected month, plus a missing rate, "SEK only" with why (the corrected month's rate
changed, or its SEK amounts round differently at the same rate), or a rate that changed
beside the amount. Own rows of a closed
month carry its close in `snapshot_id`; an open month's own rows leave both empty. Own
rows sum to the per-mode totals and adjustment rows to `totals.adjustments`;
`billable_sek = billable × rate` need not hold for adjustments. A cell that is wholly a
number, negative included (`-1.50`), is not formula-guarded; `-1+2` still is.

**Signed amounts.** `AiFeeAmount` is the signed amount type: negative only on adjustment
lines and their totals (the snapshot's checks refuse negative own rows). `to_decimal`
formats the sign (`-1.50`, `-0.05`), `AiExchangeRateValue::convert` rounds half away from
zero and is symmetric, and `allocate_largest_remainder` splits a negative total as the
negation of its magnitude's split. `AiUsdNanos` stays non-negative: adjustments carry no
usage or estimate. In the frontend, `signedDecimalSchema` (`-?\d+\.\d{2}`, never `-0.00`)
is used only by adjustment lines and the adjustment totals, so a negative own line still
fails the strict contract; `formatFee` shows the sign as a minus (`−1.50 USD`,
`−1 234,05 SEK`), and `hundredths`, `decimal` and `formatLineCredits` are sign-aware.

**On the admin page** (`/admin/ai-billing`): under the export checks, the month close panel
shows when a month in progress can close, the server's checklist for a month that is
over (each failed check with its problems, where to fix them and a waiver with its
reason; closing asks to confirm), or, for a closed month, who closed it and when, the
frozen readiness with its waivers, the re-open (disabled with the months in `heldBy`)
and earlier closes. While the readiness holds `importRunning`, the import check has no
waiver switch ("A running import cannot be waived") and the close button is disabled,
saying the import is running and to close the month once it finishes; a close refused
while any import runs says to try again when it finishes (the server's message). The
close status reloads while readiness waits for a rate being fetched (`ratePending`),
and once a running import of the month finishes (with the bill). A closed month hides
the export checks, says it is the bill as invoiced, and reads no rate as pending (its
snapshot's pending rates never resolve; the query layer clears them,
`withoutFrozenFetches`), and its exchange rates show the invoiced rate beside the
current one, which an override edits; the override's toast says the month keeps its
invoiced rate and the difference posts on the next open month.
Developers with only adjustment lines are on the People tab, and adjustment lines link a
developer to their page. The statement's headline is
`converted.invoiced`, with an Adjustments figure; an Adjustments tab lists the lines by
the project they correct (with `adjustedMonths` that have none), and a developer's page
lists theirs under the bill.

**Reprice floor:** the floor is the start of the first open month, never before the
configured one (see "Pricing basis and reprice-only uploads"): closing the first open
month moves it past every closed month that follows, and re-opening a month moves it
back. Close readiness of an open month therefore always reads all of the month's usage
from the configured floor on. A closed month after an open one stays repriceable; a
reprice of it posts on the next open month after it as an adjustment line, through the
same netting as any other change. The repricing discovery, the bill (and the recompute
of a closed month), reconciliation and readiness all read one floor (`open_from`).

**Routes**, browser-only under `/ai-usage/admin`, admin power, off the agent OpenAPI
document and MCP:

- `GET /billing/{month}/close`: `{ month, state (open | closed), timeZone, overAt,
  readiness, adjusts, close, history }`. An open month that is not over has `overAt`;
  one that is over has `readiness` (`{ evaluatedAt, ready, checks: [{ id, status
  (passed | failed | waived | notApplicable), problems, waiver }] }`, problems tagged by
  `kind`, credits as signed hundredths strings, instants RFC 3339, days `YYYY-MM-DD`)
  and `adjusts`, the closed months posting on it. A closed month has `close` (`closeId`,
  `closedAt`, `closedBy`, the frozen `readiness`, `adjustmentsPostOn`, `heldBy`).
  `history` lists every close of the month, re-opened ones included, newest first.
- `POST /billing/{month}/close` with `{ "waivers"?: [{ "check", "reason" }] }`: `201`
  with the close status. `409` while not over, already closed or busy, and for a
  metered month while any usage import runs, whatever is waived; `409` `{ error,
  readiness }` when a failed check is not waived; `400` for an unknown or repeated
  check, an empty or long reason, or an unknown field.
- `DELETE /billing/{month}/close`: re-opens, `200` with the status; `409` `{ error,
  heldBy }` while a close holds adjustments to it; `404` when it is not closed.

**Ports and adapters.** `AiMonthCloseRepository` (outbound;
`PostgresAiMonthCloseRepository`) stores closes and snapshots; `AiBillingRepository`
adds `coverage_intervals` and `old_basis_usage` for readiness, and `open_from` (on
`AiMonthCloseRepository`, `AiUsageRepository` and `AiMeteredUsageRepository`) gives the
start of the first open month for the reprice floor (`reprice_floor`), which repricing,
metered tracked usage on an old basis (the bill and reconciliation, through
`AiMeteredMonths::load`) and readiness all apply. `AiBillingService` gains
`close_status`, `close_month` and `reopen_month`; `month_overview` returns an open
overview or a closed month (`AiMonthView`), and the developer month a live or a closed
bill (`AiDeveloperBill`). Netting and readiness are pure.

**Out of scope:** closing a month from an exported CSV, the plan's fallback for invoicing
a month before month close existed; it is not needed now that close exists before the
first month to close.
