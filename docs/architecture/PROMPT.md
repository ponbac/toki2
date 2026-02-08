# Agent Prompt: Hexagonal Architecture Refactoring

You are refactoring `toki-api` to use hexagonal architecture. This prompt is idempotent—use it to start fresh or resume from where you left off.

---

## Your Mission

Refactor the Milltime integration in `toki-api` to follow hexagonal architecture, making it easy to swap Milltime for a different time tracking provider in the future.

## Key Constraints

1. **Keep it simple** - This is a small app with few developers. Don't over-engineer.
2. **Incremental changes** - Each task should result in code that compiles and works.
3. **Preserve behavior** - The app should work the same after refactoring.
4. **Document as you go** - Update `LEARNINGS.md` with discoveries and decisions.

---

## Before You Start

1. **Check current progress** - Read `TASKS.md` and find the first uncompleted task.
2. **Review learnings** - Read `LEARNINGS.md` for context from previous sessions.
3. **Understand the plan** - Read `PLAN.md` for the overall architecture vision.

---

## Working Protocol

### Starting a Task

1. Find the first `[ ]` task in `TASKS.md`
2. Mark it as `[~]` (in progress)
3. Understand what the task requires
4. Do the work
5. Verify with `cargo check`
6. Mark as `[x]` when complete
7. Move to the next task

### When You Learn Something

Add it to `LEARNINGS.md`:
- Type mappings between Milltime and domain types
- Gotchas and warnings
- Decisions made and why
- Files you've modified

### When You're Stuck

1. Document the blocker in `LEARNINGS.md` under "Open Questions"
2. Mark the task as `[!]` (blocked) with a note
3. Try the next unblocked task
4. Ask for help if truly stuck

### When You Finish a Session

1. Ensure all in-progress tasks are either completed or reverted
2. Update `LEARNINGS.md` with session notes
3. Run `cargo check` to verify everything compiles

---

## Architecture Reference

### Target Structure

```
toki-api/src/
├── domain/
│   ├── models/          # Pure domain types
│   ├── ports/
│   │   ├── inbound/     # Service traits (TimeTrackingService)
│   │   └── outbound/    # Client/repo traits (TimeTrackingClient, UserRepository)
│   ├── services/        # Business logic
│   └── error.rs         # Domain errors
├── adapters/
│   ├── inbound/http/    # Axum routes
│   └── outbound/
│       ├── postgres/    # SQLx implementations
│       └── milltime/    # Milltime client wrapper
└── app_state.rs         # Holds Arc<dyn Trait>s
```

### Key Patterns

**Newtypes for IDs:**
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn as_uuid(&self) -> &Uuid { &self.0 }
}
```

**Ports as Traits:**
```rust
#[async_trait]
pub trait TimeTrackingClient: Send + Sync + 'static {
    async fn get_active_timer(&self, user_id: &UserId)
        -> Result<Option<ActiveTimer>, TimeTrackingError>;
}
```

**Adapters Implement Ports:**
```rust
impl TimeTrackingClient for MilltimeAdapter {
    async fn get_active_timer(&self, user_id: &UserId)
        -> Result<Option<ActiveTimer>, TimeTrackingError>
    {
        // Call milltime crate, convert types, map errors
    }
}
```

**Domain Errors (never expose implementation details):**
```rust
#[derive(Debug, thiserror::Error)]
pub enum TimeTrackingError {
    #[error("timer not found")]
    TimerNotFound,
    #[error(transparent)]
    Unknown(#[from] anyhow::Error),
}
```

---

## Commands

```bash
# Verify Rust compiles
cargo check

# Run the backend
cd toki-api && bacon run

# Verify frontend types (if API changes)
cd app && bun tsc && bun lint

# Prepare SQLx after SQL changes
cargo sqlx prepare
```

---

## Files to Read First

1. `TASKS.md` - Find your current task
2. `LEARNINGS.md` - Context from previous work
3. `PLAN.md` - Overall architecture vision
4. `CLAUDE.md` (project root) - Project-specific instructions

---

## Current Task

**Read `TASKS.md` now and find the first uncompleted `[ ]` task.**

Start there. Work methodically. Keep it simple.
