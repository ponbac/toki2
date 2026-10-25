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
theirs. Deploy Toki before a Token Ledger release that sends a basis: older servers
ignore the field, but have no repricing operations.

## Unassigned usage by kind

Usage that no mapping to the configured company resolves bills to Unassigned. The
bill shows it in two kinds, from `unattributed` and from every other key:

- **unmapped**: repositories nobody has mapped, and keys mapped to another
  company's project. Mapping them on Admin > Project mappings assigns them,
  as long as time tracking is configured. A key with surrounding whitespace
  cannot be mapped.
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
(`unmapped`, `unattributed` or null), and the CSV has an `unassigned` column
after the original columns, so those keep their places. The metered columns
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
  removes a disposition as if it never existed. Edits apply retroactively until
  month close restricts them.
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
(below); repository mappings will map their repository rows, and month close will
read their coverage.

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
  organization's) stored month with GitHub's month total (`payer_gross_quantity`).
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
- Nightly, from 02:00 UTC until one succeeds (`Full`): every day of the current
  month, and of the previous month for 30 days after it ended, catching GitHub's late
  corrections.
- Months holding a slice never fetched join any run in `Changes` mode, so only the
  scopes with gaps fetch days: the first run backfills, and an account listed later
  costs one month total per month plus its own days.
- Admins can press Import now (`Changes`) or Full refresh for a month. Unattended
  runs happen in release builds only; local debug builds, whose database is usually
  a production copy, never import on their own.
- One run at a time: an in-process lock and a Postgres advisory lock
  (`ai_provider_usage_import:copilot`). Holding the lock proves no other run is
  alive, so starting a run marks leftover `running` runs interrupted. A run shows
  as running only while some session holds the lock (read from `pg_locks`), so a
  run whose process died mid-import (a deploy, a crash) neither shows a spinner
  nor disables Import now; the next run marks it interrupted.
- A rate limit, rejected credentials, a refusal, any organization-scope error, three
  `Unavailable` answers in a row or 4,000 requests stop a run. A source error on the
  organization's credits also becomes the organization's check error, so the
  residual never rests on days the run may have left half replaced. An account's
  other errors (not found, an unreadable answer) become its check error while the
  other accounts continue.
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
a seat in the month could not be imported. Errors of accounts
without a seat that month (departed people, such as a deleted login GitHub answers
404 for) are listed but never make the import stale: that account's slices of that
month are not expected, so they neither hold coverage back nor make the month a gap
that every run imports again. Stale imports show on the reconciliation tab and the
developer card and log a warning each hour; nothing else is notified. The reason
reaches the page as data (`staleReason`: a `kind` with its instant, UTC day,
source, login, month or error), which the page phrases with instants in the
browser's time zone. Account import errors are listed per account and month.

**Reconciliation**, per UTC month (`domain/models/ai_metered_reconciliation.rs`):

- `T`, tracked Copilot, is the priced estimate of usage Copilot paid for
  (`COALESCE(billing_provider, provider) = 'copilot'`, personal keys included,
  Copilot routed to Codex excluded) of the user an account's disposition links on
  each UTC day; credits are dollars × 100. Unpriced usage carries no weight,
  and neither do free models: whatever Toki priced them at is left out of T, the
  old pricing basis and tracked without a linked account, and their groups are
  listed with `T = 0`.
- *On an old pricing basis* is the part of T from buckets at or after
  `ai_usage.reprice_floor` whose basis is NULL or differs from their own machine's
  current pricing basis (`ai_usage_machines.pricing_basis`, the latest non-null basis
  of its uploads and repricings in the order they were applied, over all of the
  machine's providers): what the reprice discovery with that basis would report as
  stale. Buckets before the floor are never repriced and are reported apart. An
  upload without a basis leaves the machine's current basis alone, so it never turns
  repriced history old; only its own NULL-basis buckets count as old. A machine that
  never sent a basis has none, so all of its usage from the floor on reads as old. A
  machine that never upgraded its rates is compared with its own current basis; a
  fleet-wide basis requirement is left to month close.
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
  account that held a seat.
- Repositories: the organization's rows summed per product, SKU, unit and
  repository, with the IDE aggregate flagged and the code review check (code review
  credits per account against those per repository).
- The per-day residual is the organization's credits minus every account's, per day
  and per (SKU, model), zero within 100 nanodollars: unknown account. It rests on
  the organization's stored days, so the organization's check counts: until its
  stored month equals GitHub's month total (a usage post can land between the month
  call and the day calls), and before its first check, no day settles; a failed
  check makes every day of the month an import gap. A day missing the
  organization's slice or an expected account slice, or any day of a month with a
  failed seated account, is an import gap too; a day not yet complete or within the
  settle days is provisional. A negative residual (accounts above the organization)
  is flagged. The month's response carries the organization's check (`orgCheck`),
  shown with the residual.
- The pool card reads the month summary: credits with the pool discount (gross,
  discount, net), seats, GitHub's bill so far (every net amount) and the day the pool
  ran out, the first UTC day whose organization credits have a net quantity.

**Routes**, browser-only under `/ai-usage/admin`, admin power, off the agent OpenAPI
document and MCP: `GET /metered-usage/{month}` (the month, every account),
`GET /metered-usage/{month}/developers/{userId}` (one user's linked days, no
residual, organization check, repositories or pool; 404 for an unknown user) and `POST
/metered-usage/{month}/import` with `{ "mode": "changes" | "full" }`, which answers
`202` with the run id and runs on its own task. A malformed month or body is 400, a
month before the backfill's or after today's 400, a running import 409, a source that
is off or misconfigured 503, GitHub's rate limit 429 and other GitHub failures 502.
Credits are strings with two decimals (hundredths of a credit); only the residual
and the unknown-account totals can be negative.

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
repository-billed) are left out of the account lines: GitHub bills them per
repository too, and repository mappings will bill them from the organization's rows.
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
month has no slice or an incomplete one, or days before the backfill; excused
accounts, whose import failed without a seat, aside), `notSettled` (a day lies within
the settle days, or the account's month total is absent, failed or different),
`oldPricingBasis` (a contributing bucket in a group with credits is on an old pricing
basis, as the reconciliation tab reads it: NULL or another basis than its machine's
current pricing basis), `provisionalModel` (a model the catalog marks provisional
with priced tracked usage in a group with credits; G is GitHub's number, so a
provisional rate only moves a split) and `unrecognized` (a credits row priced
otherwise than $0.01 a credit, or a model outside the catalog). The unknown-account
line is provisional while the month is in progress, a day is an import gap, or a day
is provisional. The bill's `metered` block is provisional when a line is, when
tracked usage is not imported, when an account day is missing, or when the unknown
account's residual is provisional even though it bills no line
(`AiMeteredAllocation::unknown_account`): a failed organization check, or an
organization slice that is missing, leaves its days out of the residual, and a
residual not computed yet is no settled zero. A developer's `metered` block leaves
the unknown account out, as it is no developer's.

**Subjects and charge kinds.** A billing line's subject is a user, a provider account
(`{ provider, accountId, login }`: unlinked or external), the unknown account, or a
repository (repository mappings add those lines). A metered line's kind is `project`,
`personal`, `untracked`, `unlinkedAccount`, `externalAccount`, `unknownAccount` or
`repository`.

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
`subscription` or `github`), `credits`, `github_login`, `repository` (repository
mappings fill it), `sku`, `period_basis` (`UTC` for metered rows, the configured zone
otherwise), `adjusts_month` and `snapshot_id` (month close fills them) and
`provisional`. Spinit's metered rows name their project `Untracked Copilot`, `Personal
/ internal Copilot`, `Unlinked account`, `External account` or `Unknown account`.

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
change), with the same subject, project, kind, currency and provider; a diff key must
sum such lines or include the account.
