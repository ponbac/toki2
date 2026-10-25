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
Copilot) is never a subscription mismatch. Such usage bills as API usage at its
estimate. No Copilot subscriptions existed when declaring them was blocked.

## OpenCode

OpenCode (`opencode`) routes requests to other vendors, so like Pi it has no
subscriptions of its own, and declaring one for it is rejected. It relies on the
payer above instead: Token Ledger sets `billingProvider` to `copilot` for its
`github-copilot` route and to `codex` for its `openai` route on a ChatGPT sign-in.
Its Copilot buckets bill at their estimate, at Copilot's rates. Every other OpenCode
bucket has no payer and bills at its API estimate; Pi's ChatGPT sign-in days never
apply to it. OpenCode sends no plan hints, and Toki rejects an upload with one, since
no subscription could resolve its mismatch. Roll out Toki's migration accepting
`opencode` before a Token Ledger release that reads it: even a machine without
OpenCode reports it as `missing` coverage.

The admin daily response includes nullable `billingProvider`; `subscriptionId`
identifies the subscription actually used. Daily labels show **Mixed billing** when
one source used different subscriptions or both API and subscription billing.

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
  and basis. The response is `{ updated, unmatched, beforeFloor }`; `updated` counts
  matches whose values were equal already too. The body limit is 16 MiB, as for
  uploads.

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
(`unmapped`, `unattributed` or null), and the CSV has an `unassigned` column,
last so the other columns keep their places.

## Provider accounts and dispositions (GitHub Copilot)

GitHub bills Copilot per seat and per credit to the GitHub account that used it,
so Copilot's credits can only be billed through accounts. Toki keeps every
account that ever held a Copilot seat in the organization, and an admin decides
whose usage each one is. Nothing here changes a bill yet: the bill's Copilot
lines will read these dispositions.

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

The GitHub usage import will add each account's credits, accounts seen only in
imported usage, and the list of accounts with credits but no seat.
