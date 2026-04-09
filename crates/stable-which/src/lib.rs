//! Evaluate binary path stability and find stable PATH candidates.
//!
//! This library analyzes binary paths, tags them with stability attributes
//! (version manager, build output, ephemeral, shim, etc.), and scores them
//! to find the most stable candidate for use in service registration,
//! configuration files, or scripts.
//!
//! # Quick Start
//!
//! ```no_run
//! use stable_which::{find_candidates, ScoringPolicy};
//! use std::path::Path;
//!
//! let candidates = find_candidates(Path::new("jj"), ScoringPolicy::SameBinary).unwrap();
//! println!("Best: {}", candidates[0].path.display());
//! ```

pub mod candidate;
pub mod version_manager;

// Re-export primary types and functions for convenience
pub use candidate::{
    Candidate, Error, PathTag, ScoringPolicy, find_candidates, find_candidates_with_env,
    resolve_stable_path,
};
pub use version_manager::VersionManagerInfo;
