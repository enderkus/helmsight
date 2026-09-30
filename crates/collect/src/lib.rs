//! Agentless collection: the POSIX shell script executed on monitored hosts
//! and the parsers that turn its output into typed data.
//!
//! Everything in this crate treats remote output as untrusted input. Parsers
//! never panic, cap the amount of data they keep, and degrade to
//! [`model::Probe::Na`] when something is missing.
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod model;
pub mod parse;
pub mod sampler;
pub mod script;
pub mod sections;
mod util;

pub use sampler::{Collection, Sampler};
pub use script::{Group, ScriptOptions, ScriptRequest};
pub use sections::Sections;

/// Maximum number of bytes accepted from a single remote execution.
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
