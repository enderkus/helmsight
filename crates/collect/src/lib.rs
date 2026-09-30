//! Agentless collection: the POSIX shell script executed on monitored hosts
//! and the parsers that turn its output into typed data.
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod script;
pub mod sections;

pub use script::{Group, ScriptRequest};
pub use sections::Sections;
