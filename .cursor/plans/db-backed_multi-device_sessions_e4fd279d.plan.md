---
name: DB-backed multi-device sessions
overview: Persist sessions to Postgres (optionally cache later) and remove the implicit “single session per user” invalidation so one user can stay logged in from multiple devices concurrently.
todos:
  - id: add-db-session-store
    content: Replace in-memory `MemoryStore` with `tower-sessions-sqlx-store` `PostgresStore` and run store migration + expired-session cleanup task.
    status: pending
  - id: multi-device-auth-hash
    content: Decouple `User::session_auth_hash()` from `access_token` by adding `users.session_auth_hash` and using it for session validation (so logins don’t invalidate other devices).
    status: pending
  - id: verify-multi-session
    content: Verify two concurrent logins remain valid and survive backend restart.
    status: pending
isProject: false
---

## Current state (what’s causing single-session)

- Sessions are stored in-memory via `MemoryStore`, so they’re lost on restart and don’t work for multi-instance deployments.
- The “one session per user” behavior is _implicit_: `User::session_auth_hash()` uses `users.access_token`, and `upsert_user()` overwrites `access_token` on every login, invalidating existing sessions.

Key code today:

```74:94:/home/ponbac/dev/toki2/toki-api/src/router.rs
fn new_auth_layer(
    connection_pool: PgPool,
    config: Settings,
) -> AuthManagerLayer<AuthBackend, MemoryStore> {
    // ...
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // todo: explore production values
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    let backend = AuthBackend::new(connection_pool, client);
    AuthManagerLayerBuilder::new(backend, session_layer).build()
}
```

```56:66:/home/ponbac/dev/toki2/toki-api/src/domain/user.rs
impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id.into()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.access_token.as_bytes()
    }
}
```

## Proposed design

- **DB-backed session storage**: replace `MemoryStore` with `tower-sessions-sqlx-store`’s `PostgresStore` (and run `session_store.migrate().await?` at startup, per your choice).
- **Multiple sessions per user**: decouple `session_auth_hash` from `access_token` by adding a stable per-user `session_auth_hash` field in `users`.
  - Keep it stable across logins so a new login doesn’t log out other devices.
  - (Future-friendly) If you later want “log out all devices”, you can rotate this field.
- **Optional in-memory cache (later)**: if DB read pressure becomes an issue, wrap `PostgresStore` using `tower_sessions::CachingSessionStore` (or add `tower-sessions-moka-store` for a bounded cache). Not needed to deliver multi-device logins.

## Implementation outline

- Add `tower-sessions-sqlx-store` dependency in `[toki-api/Cargo.toml](/home/ponbac/dev/toki2/toki-api/Cargo.toml)`.
- Initialize session store when auth is enabled:
  - Create `PostgresStore::new(connection_pool.clone())`.
  - Run `session_store.migrate().await?`.
  - Spawn `continuously_delete_expired(...)` background cleanup (prevents table bloat).
- Update auth layer wiring in `[toki-api/src/router.rs](/home/ponbac/dev/toki2/toki-api/src/router.rs)`:
  - Swap `MemoryStore` → `PostgresStore` (and update the layer’s concrete type).
  - Keep cookie settings (expiry, samesite) unchanged initially.
- Make sessions multi-device by fixing auth hash semantics:
  - Add a SQLx migration in `[toki-api/migrations/](/home/ponbac/dev/toki2/toki-api/migrations)` to add `users.session_auth_hash` with a random default (recommended: enable `pgcrypto` and use `gen_random_uuid()`; store as `TEXT`).
  - Update `DbUser` selects/returns and `User` struct in:
    - `[toki-api/src/repositories/user_repo.rs](/home/ponbac/dev/toki2/toki-api/src/repositories/user_repo.rs)`
    - `[toki-api/src/domain/user.rs](/home/ponbac/dev/toki2/toki-api/src/domain/user.rs)`
  - Change `User::session_auth_hash()` to return `self.session_auth_hash.as_bytes()`.
  - Ensure `upsert_user()` does **not** overwrite `session_auth_hash` on conflict.

## Verification

- Start backend, login in two separate browser profiles/devices.
- Confirm both can call `GET /auth/me` successfully, and logging in on one does not invalidate the other.
- Restart backend and confirm sessions persist (cookie still valid and `GET /auth/me` works).

## Notes / follow-ups (not required for the goal)

- Consider making `.with_secure(...)` conditional on environment/HTTPS to avoid sending auth cookies over plaintext in production.
