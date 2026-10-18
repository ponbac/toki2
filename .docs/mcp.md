# MCP Server Notes

Permanent notes for Toki's Model Context Protocol endpoint and the OAuth authorization server MCP clients sign in through. Read this before changing MCP tools, MCP authentication, or the authorization server.

## Overview

- `POST /mcp` serves Streamable HTTP through the official Rust SDK (`rmcp`), in `toki-api/src/adapters/inbound/mcp/`.
- Clients on protocol `2026-07-28` and later are served statelessly. Older clients get an in-memory `rmcp` session, which server-to-client elicitation needs. Sessions hold no identity: every request is authenticated on its own.
- The tools are the curated agent surface of `/openapi.json`, one tool per operation except `uploadAiUsage`, a sync protocol for usage collectors. Each tool calls the same handler as its operation, so access checks and validation are shared. A test fails when an OpenAPI operation has no tool.
- Read-only tools carry `readOnlyHint`. Destructive tools (discarding the timer, editing or deleting time entries, moving work items, editing or deleting AI subscriptions) carry `destructiveHint` and ask the user to confirm through form elicitation when the client supports it: a multi round-trip request on `2026-07-28`, or an `elicitation/create` request in a legacy session. The confirmation's `requestState` is HMAC-sealed and bound to the user, the tool and its exact arguments, and expires after 10 minutes. Clients without elicitation rely on their own approval of destructive tools.
- Note generation is left to the client's model: `get_work_item_context` returns a work item as Markdown, and the server instructions describe the note convention.

## Authentication

- `/mcp` never accepts the browser session cookie.
- A Toki personal API token (`Authorization: Bearer toki_…`) always works. Create one under account settings.
- Otherwise `/mcp` accepts access tokens from Toki's own OAuth 2.1 authorization server (below). Unauthenticated requests get `401` with `WWW-Authenticate: Bearer resource_metadata="{api_url}/.well-known/oauth-protected-resource/mcp", scope="mcp"`. The Protected Resource Metadata names the resource `{api_url}/mcp`, the authorization server `{api_url}`, and the scope `mcp`.
- Neither token kind carries admin power (`AuthMethod::allows_admin_power`).
- With `application.disable_auth`, the authorization server is not mounted and `/mcp` accepts API tokens only.

## Authorization Server

Toki is its own authorization server so that MCP clients connect with just the URL: no client IDs or secrets to hand out. Users sign in with their Spinit account through Toki's ordinary Entra web sign-in. Code: `domain/models/oauth.rs`, `domain/services/oauth.rs`, `adapters/inbound/http/oauth/`, `adapters/outbound/postgres/oauth.rs`, `adapters/outbound/cimd.rs`.

- **Endpoints:** `GET /.well-known/oauth-authorization-server` (RFC 8414), `GET`/`POST /oauth/authorize`, `POST /oauth/token`, `POST /oauth/register` (RFC 7591), `POST /oauth/revoke` (RFC 7009). The issuer is `api_url` without a trailing slash. `/oauth/callback` is unrelated: it is the Entra web sign-in's callback.
- **Clients** are public and prove possession with PKCE (S256 only); client authentication of any kind is refused with `invalid_client`.
  - A client_id that is an HTTPS URL is a Client ID Metadata Document, fetched and cached for an hour. Claude, Claude Code, ChatGPT, Codex and VS Code use these. The fetcher connects only to public addresses (checked after DNS resolution), follows no redirects, uses no proxy, and reads at most 5 KB within 5 seconds. A document may declare another `token_endpoint_auth_method` if its `token_endpoint_auth_methods_supported` includes `none`, as ChatGPT's does.
  - Other client_ids come from dynamic registration, which Cursor and Executor use. Registration is limited to 300 per hour across all callers (an anonymous caller can exhaust it, which blocks only new registrations); registrations that never produced a connection are deleted after 30 days, others are kept, as clients cache their client_id.
- **Redirect URIs** are `https`, `http` on `localhost`, `127.0.0.1` or `[::1]` (any port matches a registered loopback URI, RFC 8252), or Cursor's `cursor://` callback. The last goes beyond the MCP specification, which allows only HTTPS and localhost, because Cursor registers it alongside its other callbacks and fails if any is rejected. Other private-use schemes are refused: operating systems route many of them to handlers of their own (`ms-settings:`), so native app schemes are allowlisted one by one.
- **Sign-in and consent:** `/oauth/authorize` validates the client and redirect URI first; until both are trusted, errors are shown as a page and never redirected. It then requires an Entra sign-in from the last 15 minutes, starting one otherwise (usually a silent bounce through Entra), so every connection follows a fresh Entra check of the account. The sign-in continues back to the authorization request through the session's login continuation, which any later sign-in start replaces. The consent page names the client (its metadata document's host is verified; a registered client's name is its own claim) and where the response goes, with a warning for apps on the user's computer. Pending consents are stored in `oauth_authorization_requests`, bound to the user, and decided at most once.
- **Errors** in a request whose client and redirect URI are valid go back to an app on the user's computer (a loopback or `cursor://` redirect) at once. A web redirect URI gets an error page with a link back instead: anyone can register one, and redirecting there before the user did anything would make Toki an open redirector (RFC 9700, section 4.11.2). The consent decision always redirects.
- **Responses** always carry `iss`. The `resource` must be `{api_url}/mcp` when sent; `scope` is accepted and ignored, as the only grant is `mcp`.
- **Tokens** are opaque random values stored as SHA-256 hashes: codes `tmcp_ac_…` (5 minutes, single use), access tokens `tmcp_at_…` (1 hour), refresh tokens `tmcp_rt_…` (30 days idle). Only `/mcp` accepts access tokens; the REST API's bearer middleware accepts only API tokens.
- **Connections** (`oauth_grants`) last at most 90 days, then the user connects again. Refresh tokens rotate on every use. A rotated refresh token presented again within 30 seconds is refused without consequence (one client refreshing twice at once); later, its reuse revokes the connection, even after the token itself expired: rotated refresh tokens are kept as long as their connection. A replayed authorization code revokes the connection it produced. Code redemption and refresh each run in one locked transaction.
- **Revocation:** clients revoke through `/oauth/revoke` with their `client_id`, which revokes the whole connection. Users see and disconnect their connections under account settings ("Connected apps", `GET`/`DELETE /users/me/oauth-grants`), which lists connections that can still refresh; "last active" is the last token refresh. Deleting a Toki user deletes its connections. A disabled Entra account cannot start a new connection, but existing ones last until they expire or are revoked.
- An hourly task deletes expired consents, codes and tokens, and connections revoked or expired over 30 days ago.

## Client Setup

Every OAuth client needs only the URL `https://toki-api.spinit.se/mcp`; the user then signs in and allows access in the browser.

- claude.ai, Claude Desktop and mobile: Customize → Connectors → Add custom connector with the URL.
- ChatGPT: Apps → Add → Add custom MCP server with the URL and OAuth.
- Claude Code: `claude mcp add --transport http toki https://toki-api.spinit.se/mcp`, then `/mcp` to sign in. With an API token instead: add `--header "Authorization: Bearer toki_…"`.
- Codex: `codex mcp add toki --url https://toki-api.spinit.se/mcp`, then `codex mcp login toki`. With an API token instead: set `bearer_token_env_var` on `[mcp_servers.toki]` in `config.toml`.
- VS Code (desktop): add the URL as an HTTP server. The browser-hosted vscode.dev is not supported.
- Cursor: add the URL as a remote MCP server.
- Executor: add the URL as an MCP integration with OAuth, or with a Bearer header authentication method and a Toki API token.
- Clients that cannot do OAuth (such as Copilot CLI with a static header) use an API token.

### Moving from the Entra setup

Connections set up before Toki had its own authorization server used Entra client IDs (`b68eecf8-03eb-49e4-bc66-657185c9d167`, or `9dacddfe-b9d4-4d28-8d2a-f3e431824c9d` with a secret). Toki does not know those clients, so such a connection cannot sign in again: remove it and add Toki again with just its URL.

- Claude Code: `claude mcp remove toki`, then add it without `--client-id` and `--callback-port`.
- Codex: `codex mcp remove toki`, then add it without `--oauth-client-id`, and `codex mcp login toki`.
- claude.ai: remove the custom connector and add it again without "Use your own OAuth client".
- ChatGPT: delete the app and add the MCP server again without a user-defined OAuth client.
- VS Code: remove the server and add it again; sign in with Toki instead of the Microsoft account.

## Retired Entra Setup

Until the switch to Toki's own authorization server, `/mcp` validated Entra access tokens and MCP clients used two Entra app registrations in the Spinit tenant, `Toki MCP (native)` (`b68eecf8-03eb-49e4-bc66-657185c9d167`) and `Toki MCP (hosted)` (`9dacddfe-b9d4-4d28-8d2a-f3e431824c9d`, with per-service secrets), pre-authorized for the `mcp` scope that `t2m-stats` (`6494e716-bbf2-4164-a1f0-cd682313085b`) exposes under the Application ID URI `https://toki-api.spinit.se/mcp`. Nothing uses them now; they can be deleted, along with the `mcp` scope and its pre-authorizations. Leave `t2m-stats` itself alone: it is the web sign-in's app registration.
