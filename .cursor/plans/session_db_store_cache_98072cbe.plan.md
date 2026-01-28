---
name: Session DB store cache
overview: Add an in-memory Moka cache in front of the existing `PostgresStore` using `tower_sessions::CachingSessionStore`, reducing DB reads for hot sessions while keeping Postgres as the source of truth.
todos:
  - id: add-moka-cache-deps
    content: Add `tower-sessions` and `tower-sessions-moka-store` dependencies compatible with current tower-sessions version.
    status: pending
  - id: wrap-store-with-caching-session-store
    content: Wrap existing `PostgresStore` with `CachingSessionStore<MokaStore, PostgresStore>` in `toki-api/src/router.rs`, keeping expired-session cleanup on the Postgres store.
    status: pending
  - id: verify-no-behavior-regression
    content: Verify login/logout and session persistence still work; optionally confirm fewer DB session loads after warmup.
    status: pending
isProject: false
---

## Starting point (already in repo)

- Sessions are currently persisted in Postgres via `tower_sessions_sqlx_store::PostgresStore`, initialized in `[toki-api/src/router.rs](/home/ponbac/dev/toki2/toki-api/src/router.rs)`.

```87:110:/home/ponbac/dev/toki2/toki-api/src/router.rs
    // Use PostgresStore for DB-backed sessions that persist across restarts
    let session_store = PostgresStore::new(connection_pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to run session store migration");

    // Spawn background task to clean up expired sessions
    let deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
    );
    // Detach the task so it runs independently
    drop(deletion_task);

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // todo: explore production values
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));
```

## Proposed caching approach

- Use `tower_sessions::CachingSessionStore<Cache, Store>`.
- Use `tower_sessions_moka_store::MokaStore` as the cache with **max_capacity = 2,000** (your choice).
  - MokaStore automatically expires cache entries based on each session’s expiry, so expired sessions don’t linger in memory.
- Keep `PostgresStore` as the backing store, including:
  - `session_store.migrate().await` (table ensured)
  - the `continuously_delete_expired(...)` cleanup task (prevents DB bloat)

## Implementation steps

- Add dependencies in `[toki-api/Cargo.toml](/home/ponbac/dev/toki2/toki-api/Cargo.toml)`:
  - `tower-sessions = "0.13"` (so we can use `tower_sessions::CachingSessionStore` explicitly)
  - `tower-sessions-moka-store = "0.13"`
- Update `[toki-api/src/router.rs](/home/ponbac/dev/toki2/toki-api/src/router.rs)`:
  - Import `tower_sessions::CachingSessionStore` and `tower_sessions_moka_store::MokaStore`.
  - Rename the existing `session_store` variable to `db_store` for clarity.
  - Spawn `continuously_delete_expired` using `db_store.clone()` **before** moving it into the caching wrapper.
  - Create the cache store:
    - `let cache_store = MokaStore::new(Some(2_000));`
  - Wrap stores:
    - `let session_store = CachingSessionStore::new(cache_store, db_store);`
  - Pass `session_store` to `SessionManagerLayer::new(session_store)`.
  - Update `new_auth_layer`’s return type from `AuthManagerLayer<AuthBackend, PostgresStore>` to:
    - either `AuthManagerLayer<AuthBackend, CachingSessionStore<MokaStore, PostgresStore>>`, or
    - define a local type alias (cleaner):
      - `type SessionStore = CachingSessionStore<MokaStore, PostgresStore>;`
      - `-> AuthManagerLayer<AuthBackend, SessionStore>`

## Verification

- Run the backend and perform normal login flow.
- Confirm no behavior change:
  - sessions still persist across restarts
  - `GET /me` works
  - logout still works
- (Optional) Add debug logging around store calls or enable tracing to confirm load hits drop after first request.

## Important caveat (multi-instance)

`CachingSessionStore` caches **per server instance**. If you later add “server-side session revocation by deleting session rows”, another instance could temporarily accept a now-deleted session until its cache entry expires. A robust future revocation approach is to rotate `users.session_auth_hash` (global invalidation) rather than relying on per-session deletions.
