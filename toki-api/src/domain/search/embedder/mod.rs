//! Embedding generation implementations.

mod gemini;
#[cfg(test)]
mod mock;

pub use gemini::GeminiEmbedder;
#[cfg(test)]
pub use mock::MockEmbedder;
