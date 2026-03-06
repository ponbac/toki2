# toki-tui version + status commands Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `toki-tui version` and `toki-tui status` CLI subcommands and bump the crate version to 0.2.0.

**Architecture:** Two new variants added to the existing `Commands` enum in `cli.rs`, handled in the `match` block in `main.rs`. No new files, no network calls. Version is read at compile time via `env!("CARGO_PKG_VERSION")`. Status is purely local — reads session and Milltime cookie files from disk.

**Tech Stack:** clap 4 (derive), Rust std, existing `session_store` module.

---

### Task 1: Bump crate version to 0.2.0

**Files:**
- Modify: `toki-tui/Cargo.toml:3`

**Step 1: Edit version field**

Change:
```toml
version = "0.1.0"
```
To:
```toml
version = "0.2.0"
```

**Step 2: Verify build**

```bash
SQLX_OFFLINE=true just check
```
Expected: `Finished` with no errors.

---

### Task 2: Add `Version` subcommand

**Files:**
- Modify: `toki-tui/src/cli.rs`
- Modify: `toki-tui/src/main.rs`

**Step 1: Add variant to Commands enum in `cli.rs`**

```rust
/// Print the current version
Version,
```

**Step 2: Handle in `main.rs` match block**

```rust
Commands::Version => {
    println!("{}", env!("CARGO_PKG_VERSION"));
}
```

**Step 3: Verify build**

```bash
SQLX_OFFLINE=true just check
```
Expected: `Finished` with no errors.

---

### Task 3: Add `Status` subcommand

**Files:**
- Modify: `toki-tui/src/cli.rs`
- Modify: `toki-tui/src/main.rs`

**Step 1: Add variant to Commands enum in `cli.rs`**

```rust
/// Show current login and Milltime session status
Status,
```

**Step 2: Handle in `main.rs` match block**

```rust
Commands::Status => {
    let session = session_store::load_session()?;
    let mt_cookies = session_store::load_mt_cookies()?;
    let session_status = if session.is_some() { "logged in" } else { "not logged in" };
    let mt_status = if !mt_cookies.is_empty() { "authenticated" } else { "no cookies" };
    println!("Session:  {}", session_status);
    println!("Milltime: {}", mt_status);
}
```

**Step 3: Verify build**

```bash
SQLX_OFFLINE=true just check
```
Expected: `Finished` with no errors.

---

### Task 4: Add justfile recipes

**Files:**
- Modify: `justfile`

**Step 1: Add two recipes after existing tui-* recipes**

```just
# Print toki-tui version
tui-version:
    cd toki-tui && cargo run -- version

# Show toki-tui session status
tui-status:
    cd toki-tui && cargo run -- status
```

---

### Task 5: Commit

```bash
git add toki-tui/Cargo.toml toki-tui/src/cli.rs toki-tui/src/main.rs justfile docs/plans/2026-03-02-tui-version-status-commands.md
git commit -m "feat(tui): add version and status subcommands, bump to v0.2.0"
```
