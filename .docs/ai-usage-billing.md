# AI usage source and subscription payer

Token Ledger uploads a source tool as `provider`. Sync payload version 2 additionally
accepts an optional `billingProvider` on an hourly usage bucket. It names the
subscription provider that should pay for those records; it must differ from the
source and support subscriptions (`codex`, `claude`, `grok`, or `copilot`). Version 1
continues to work and rejects non-null overrides so clients cannot silently rely on
metadata an older server ignores.

The bucket key includes the nullable payer. Native Copilot and requests routed
through ChatGPT can therefore share an hour, session, project, and model without
being merged. Coverage, authoritative replacement, reporting, and source labels
still use the source provider. No request IDs, auth modes, or local hint paths are
accepted as billing inputs or stored by this feature.

Billing resolves an explicit payer against the developer's declared subscription
covering the local usage day. If none covers it, the record bills at its API-equivalent
estimate, even when a subscription for the source tool exists. A single subscription
fee is shared across every source it pays for. Existing unoverridden Pi sign-in
routing is unchanged. Missing-subscription warnings for explicit overrides identify
the payer so adding the suggested subscription fixes the right provider.

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
