//! Parsers for individual sections of the script output. Every function
//! accepts arbitrary text and never panics.

pub mod auth;
pub mod containers;
pub mod fs;
pub mod listen;
pub mod packages;
pub mod procfs;
pub mod procs;
pub mod services;
pub mod system;
pub mod updates;
