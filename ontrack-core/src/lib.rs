//! ontrack-core — shared route optimization logic
//! Used by both the CLI and GUI crates.

pub mod config;
pub mod exporter;
pub mod geocoder;
pub mod matrix;
pub mod parser;
pub mod solver;
#[cfg(feature = "voice")]
pub mod voice;

// Re-export the most commonly used types at the crate root
pub use geocoder::Location;
pub use solver::{RouteResult, SolverBackend};
pub use matrix::MatrixBackend;
