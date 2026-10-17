# MCP Server Notes

Permanent notes for Toki's Model Context Protocol endpoint. Read this before changing MCP tools, MCP authentication, or the Entra app registrations behind them.

## Overview

- `POST /mcp` serves Streamable HTTP through the official Rust SDK (`rmcp`), in `toki-api/src/adapters/inbound/mcp/`.
- Clients on protocol `2026-07-28` and later are served statelessly. Older clients get an in-memory `rmcp` session, which server-to-client elicitation needs. Sessions hold no identity: every request is authenticated on its own.
- The tools are the curated agent surface of `/openapi.json`, one tool per operation except `uploadAiUsage`, a sync protocol for usage collectors. Each tool calls the same handler as its operation, so access checks and validation are shared. A test fails when an OpenAPI operation has no tool.
- Read-only tools carry `readOnlyHint`. Destructive tools (discarding the timer, editing or deleting time entries, moving work items, editing or deleting AI subscriptions) carry `destructiveHint` and ask the user to confirm through form elicitation when the client supports it: a multi round-trip request on `2026-07-28`, or an `elicitation/create` request in a legacy session. The confirmation's `requestState` is HMAC-sealed and bound to the user, the tool and its exact arguments, and expires after 10 minutes. Clients without elicitation rely on their own approval of destructive tools.
- Note generation is left to the client's model: `get_work_item_context` returns a work item as Markdown, and the server instructions describe the note convention.

## Authentication

- `/mcp` never accepts the browser session cookie.
- A Toki personal API token (`Authorization: Bearer toki_…`) always works. Create one under account settings.
- With `mcp.oauth` configured, `/mcp` is also an OAuth resource server for Microsoft Entra ID:
  - Unauthenticated requests get `401` with `WWW-Authenticate: Bearer resource_metadata="{api_url}/.well-known/oauth-protected-resource/mcp", scope="{api_url}/mcp/{scope} offline_access"`.
  - The metadata names `https://login.microsoftonline.com/{tenant_id}/v2.0` as the authorization server and `{api_url}/mcp` as the resource. Its scopes include `offline_access`, because clients request the scopes listed there and Entra issues refresh tokens only for that scope.
  - Tokens must be v2 access tokens signed by the tenant (keys from its JWKS), with `iss` the v2 issuer, `aud` = `auth.client_id`, `tid` = `tenant_id`, and `scope` in `scp`.
  - The token's `preferred_username`, then its `email`, is matched, ignoring case, to an existing Toki user's email, which the web sign-in recorded. Users must have signed in to Toki in the browser once.
- Neither token kind carries admin power (`AuthMethod::allows_admin_power`).
- Settings:
  - `mcp.oauth.tenant_id` / `TOKI_MCP__OAUTH__TENANT_ID`
  - `mcp.oauth.scope` / `TOKI_MCP__OAUTH__SCOPE`, the scope name, such as `mcp`
- Entra issues no Dynamic Client Registration and no Client ID Metadata Documents, so every OAuth client must be configured with a pre-registered client ID.

## Entra App Registrations

- The resource is Toki's existing app registration (`auth.client_id`). It needs `api.requestedAccessTokenVersion = 2`, the Application ID URI `{api_url}/mcp` (an HTTPS URI on the tenant's verified domain), and the `mcp` delegated scope.
- MCP clients use separate client apps, pre-authorized for the scope on the resource so users see no consent prompt:
  - A public client for native clients (Claude Code, Codex, Cursor) with loopback redirect URIs. Entra ignores the port of loopback redirects.
  - A confidential client for hosted clients (claude.ai, Executor) with their web redirect URIs and one labelled secret per hosted service.
- Entra's discovery document does not advertise `code_challenge_methods_supported`, though it supports S256 PKCE. A strictly conforming client may refuse it; fall back to an API token.

### Spinit tenant (production)

- Resource: `t2m-stats`, client ID `6494e716-bbf2-4164-a1f0-cd682313085b`, scope `https://toki-api.spinit.se/mcp/mcp` (scope ID `34560bbf-e1e3-4762-a69e-d0e7ab255502`).
- `Toki MCP (native)`: client ID `b68eecf8-03eb-49e4-bc66-657185c9d167`. Redirects `http://localhost/callback`, `http://127.0.0.1/callback`.
- `Toki MCP (hosted)`: client ID `9dacddfe-b9d4-4d28-8d2a-f3e431824c9d`. Redirect `https://claude.ai/api/mcp/auth_callback`. Add each hosted service's redirect and its own secret when connecting it.
- Pre-authorized for `mcp`: both client apps and VS Code (`aebc6443-996d-45c2-90f0-388ff96faa56`).

## Client Setup

`<native-client-id>` is `b68eecf8-03eb-49e4-bc66-657185c9d167` and `<hosted-client-id>` is `9dacddfe-b9d4-4d28-8d2a-f3e431824c9d` in production.

- Claude Code, OAuth: `claude mcp add --transport http --client-id <native-client-id> --callback-port 8080 toki https://toki-api.spinit.se/mcp`, then `/mcp` to sign in.
- Claude Code, API token: `claude mcp add --transport http toki https://toki-api.spinit.se/mcp --header "Authorization: Bearer toki_…"`.
- Codex, OAuth: `codex mcp add toki --url https://toki-api.spinit.se/mcp --oauth-client-id <native-client-id>`, then `codex mcp login toki`. Codex prints its callback URL; register that exact URL on the native client.
- Codex, API token: set `bearer_token_env_var` on `[mcp_servers.toki]` in `config.toml`.
- claude.ai, Claude Desktop and mobile: add a custom connector with the URL and, under Advanced, `<hosted-client-id>` and its secret. Redirect URI: `https://claude.ai/api/mcp/auth_callback`.
- VS Code: add the URL as an HTTP server. It signs in through its built-in Microsoft account provider, whose client ID is pre-authorized on the resource.
- Executor: add the URL as an MCP source and connect an OAuth account with `<hosted-client-id>` and its secret. Register the redirect URL its dialog shows on the hosted client.
