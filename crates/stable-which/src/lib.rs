pub mod candidate;
pub mod version_manager;

// Re-export primary types and functions for convenience
pub use candidate::{
    Candidate, Error, PathTag, ScoringPolicy, find_candidates, find_candidates_with_env,
    resolve_stable_path,
};
pub use version_manager::VersionManagerInfo;
