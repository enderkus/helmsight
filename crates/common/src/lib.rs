//! Configuration, secrets and types shared by all crates.

pub mod config;
pub mod duration;
mod locate;
pub mod role;
pub mod rules;
pub mod secrets;
pub mod selector;
pub mod sshconfig;

pub use config::{Config, ConfigError, ConfigErrors};
pub use duration::Dur;
pub use role::Role;

/// Product name used in user-visible text, cookie names and default paths.
/// Renaming the project only requires changing this constant, the binary
/// crate name and `web/src/lib/brand.ts`.
pub const PRODUCT_NAME: &str = "helmsight";

/// Version of the running binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
