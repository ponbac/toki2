//! The timer buddy battle engine: a pure, deterministic simulation of seeded
//! battles between teams of one or two buddies.
//!
//! The species, their pre-picked moves and the type chart come from the embedded
//! dex, `toki-api/data/buddy_dex.json`, which `app/scripts/build_buddy_dex.ts`
//! generates from generation 5 data. The engine does no I/O: the same teams and seed
//! always give the same events, so a battle is resolved once and replayed from its
//! stored events.
//!
//! Every rule change must bump `ENGINE_VERSION`, which is stored with each battle.

mod dex;
mod engine;
mod rng;
mod team;
mod trainer;

#[cfg(test)]
mod tests;

pub use dex::is_known_species;
pub use engine::resolve;
pub use team::build_team;
pub use trainer::daily_trainer;

/// The version of the battle rules. Bump it whenever a rule changes.
pub const ENGINE_VERSION: i16 = 1;
