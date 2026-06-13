#![warn(missing_docs)]
//! Evaluate binary path stability and find stable PATH candidates.
//!
//! This library analyzes binary paths, tags them with observed attributes
//! (version manager, build output, ephemeral, shim, etc.), judges their
//! [`Durability`] (whether a path can be pinned into a service definition),
//! and ranks them to find the most suitable candidate for use in service
//! registration, configuration files, or scripts.
//!
//! # Quick Start
//!
//! ```no_run
//! use stable_which::{find_candidates, rank_candidates, ScoringPolicy};
//! use std::path::Path;
//!
//! let mut candidates = find_candidates(Path::new("jj")).unwrap();
//! rank_candidates(&mut candidates, ScoringPolicy::SameBinary);
//! println!("Best: {}", candidates[0].path().display());
//! ```
//!
//! # API layering
//!
//! The public surface separates three concerns:
//!
//! - [`find_candidates`] / [`find_candidates_with_path_env`] — *discovery only*,
//!   returns candidates in deterministic (PATH discovery) order.
//! - [`rank_candidates`] — sorts a candidate slice in place by a
//!   [`ScoringPolicy`].
//! - [`resolve_stable_path`] — the convenience composition of find + rank that
//!   returns the single best candidate.

mod candidate;
mod durability;
mod path_analysis;

// Re-export the primary types and functions. The module structure itself is an
// implementation detail; consumers reference everything from the crate root.
pub use candidate::{
    Candidate, Error, PathTag, ScoringPolicy, find_candidates, find_candidates_with_path_env,
    rank_candidates, resolve_stable_path,
};
pub use durability::Durability;
