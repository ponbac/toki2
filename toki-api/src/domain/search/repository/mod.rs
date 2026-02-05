//! Search repository implementations.

#[cfg(test)]
mod mock;
mod postgres;

#[cfg(test)]
pub use mock::MockSearchRepository;
pub use postgres::PgSearchRepository;
