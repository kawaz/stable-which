use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::durability::{self, Durability};
use crate::path_analysis;

/// Errors that can be returned by [`find_candidates`] and related functions.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Binary not found at the specified path
    NotFound(PathBuf),
    /// Path exists but is not a file
    NotAFile(PathBuf),
    /// Cannot determine the file name from the path
    NoFileName(PathBuf),
    /// Command name not found in PATH
    NotInPath(String),
    /// Failed to canonicalize path
    Canonicalize(PathBuf, std::io::Error),
    /// Failed to read file metadata
    Metadata(PathBuf, std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound(p) => write!(f, "'{}' does not exist", p.display()),
            Error::NotAFile(p) => write!(f, "'{}' is not a file", p.display()),
            Error::NoFileName(p) => write!(f, "'{}' has no file name", p.display()),
            Error::NotInPath(name) => write!(f, "'{}' not found in PATH", name),
            Error::Canonicalize(p, e) => {
                write!(f, "cannot canonicalize '{}': {}", p.display(), e)
            }
            Error::Metadata(p, e) => write!(f, "cannot stat '{}': {}", p.display(), e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Canonicalize(_, e) | Error::Metadata(_, e) => Some(e),
            _ => None,
        }
    }
}

/// Observed properties of a candidate path.
///
/// Tags describe how a candidate was found and what role it plays. Multiple
/// tags can be attached to a single candidate. See [`Candidate::tags`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathTag {
    /// The path that was passed as input
    Input,
    /// Found in PATH environment variable (holds discovery order: 0 = first PATH match)
    InPathEnv(usize),
    /// Is a symlink, pointing to the given target
    SymlinkTo(PathBuf),
    /// In a shim directory or symlink target name doesn't match candidate name
    Shim,
    /// Canonical path matches the input binary (same file)
    SameCanonical,
    /// File content matches the input binary byte-for-byte (copied identical binary)
    SameContent,
    /// Relative path (doesn't start with /)
    Relative,
    /// Non-normalized path (contains .. or . components)
    NonNormalized,
    /// Same name but different binary (canonical and content both differ)
    DifferentBinary,
    /// Under a version manager (holds manager name)
    ManagedBy(String),
    /// Inside a build output directory
    BuildOutput,
    /// Inside a temporary/cache directory
    Ephemeral,
    /// Not directly executable (no execute permission, or ambiguous bare relative path)
    NotExecutable,
}

/// A discovered location for a binary, with the observed [`PathTag`]s that
/// describe it.
///
/// Fields are private; use the accessors ([`path`](Self::path),
/// [`canonical`](Self::canonical), [`tags`](Self::tags),
/// [`durability`](Self::durability), [`is_stable`](Self::is_stable)). This
/// hides the internal `Vec<PathTag>` representation so it can change without
/// breaking the public API.
#[derive(Debug, Clone)]
pub struct Candidate {
    path: PathBuf,
    canonical: PathBuf,
    tags: Vec<PathTag>,
}

/// Policy for ranking candidates returned by [`rank_candidates`].
///
/// The two policies differ in whether binary identity or path stability
/// is the primary criterion. See [`rank_candidates`] for usage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScoringPolicy {
    /// Prefer the candidate that points to the **same binary** as the input
    /// (by canonical path or content), then break ties by path stability.
    ///
    /// This is the default and is appropriate for service registration, where
    /// the exact same binary file must be referenced.
    #[default]
    SameBinary,
    /// Prefer the candidate with the most **stable path**, then break ties by
    /// binary identity.
    ///
    /// Useful for configuration files that should survive binary upgrades
    /// (the path keeps working even if the underlying file changes).
    Stable,
}

impl Candidate {
    /// The path of this candidate as discovered (input, PATH entry, symlink
    /// target, etc.). Not necessarily canonicalized.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The canonical path of this candidate.
    ///
    /// When [`std::fs::canonicalize`] succeeds this is the real (symlink- and
    /// `..`-resolved) path. When it fails, it falls back to the candidate's
    /// own path, so this is **not guaranteed** to always be a realpath.
    pub fn canonical(&self) -> &Path {
        &self.canonical
    }

    /// The observed tags describing this candidate.
    ///
    /// Tag order is **not guaranteed** ([`PathTag::Input`] /
    /// [`PathTag::InPathEnv`] happen to lead, but other tags follow in
    /// implementation order). Test membership with
    /// `tags().contains(...)` / `tags().iter().any(...)`, not by position.
    pub fn tags(&self) -> &[PathTag] {
        &self.tags
    }

    /// The [`Durability`] of this candidate's [`path`](Self::path): whether the
    /// path string can be pinned into a service definition and survive
    /// rebuild / upgrade / reboot.
    ///
    /// Judged per candidate, so the reference path and the canonical path of
    /// the same binary can differ (e.g. `/opt/homebrew/bin/git` is
    /// [`Durable`](Durability::Durable) while
    /// `/opt/homebrew/Cellar/git/2.44.0/bin/git` is
    /// [`NotDurable`](Durability::NotDurable)). See [`Durability`] for the
    /// judging model and its known limitations.
    pub fn durability(&self) -> Durability {
        durability::judge(&self.path)
    }

    /// Whether this candidate is *durable-to-pin*: `true` iff
    /// [`durability`](Self::durability) is [`Durability::Durable`].
    ///
    /// [`Durability::Unknown`] is treated as not stable (safe side).
    pub fn is_stable(&self) -> bool {
        matches!(self.durability(), Durability::Durable)
    }

    /// Ranking score for a [`ScoringPolicy`]. Internal: the absolute value is
    /// policy-dependent and not a stable ordering across policies, so it is not
    /// part of the public API. Public ranking is expressed by
    /// [`rank_candidates`].
    pub(crate) fn score(&self, policy: ScoringPolicy) -> i32 {
        let binary_score = if self.tags.contains(&PathTag::SameCanonical) {
            3
        } else if self.tags.contains(&PathTag::SameContent) {
            2
        } else {
            0
        };

        let has_build_output = self.tags.contains(&PathTag::BuildOutput);
        let has_ephemeral = self.tags.contains(&PathTag::Ephemeral);
        let has_managed = self.tags.iter().any(|t| matches!(t, PathTag::ManagedBy(_)));
        let has_shim = self.tags.contains(&PathTag::Shim);

        // Preference tier within a score band: this is *not* durability
        // (DR-016). It expresses how preferred a candidate is among same-band
        // peers, distinct from "can this path be baked in" (`durability()`).
        //
        // Tiers are 0 / 1 / 3; tier 2 is intentionally left as a gap reserved
        // for a future intermediate category, so existing tiers keep their
        // relative spacing without a re-numbering churn.
        let preference_tier = if has_build_output || has_ephemeral {
            0
        } else if has_managed || has_shim {
            1
        } else {
            3
        };

        let in_path_bonus = if self.tags.iter().any(|t| matches!(t, PathTag::InPathEnv(_))) {
            5
        } else {
            0
        };

        let mut penalty = 0i32;
        if self.tags.contains(&PathTag::NotExecutable) {
            penalty -= 10000;
        }
        if self.tags.contains(&PathTag::Relative) {
            penalty -= 3;
        }
        if self.tags.contains(&PathTag::NonNormalized) {
            penalty -= 2;
        }

        match policy {
            ScoringPolicy::SameBinary => {
                binary_score * 1000 + preference_tier * 10 + in_path_bonus + penalty
            }
            ScoringPolicy::Stable => {
                preference_tier * 1000 + binary_score * 10 + in_path_bonus + penalty
            }
        }
    }

    /// PATH discovery order from the [`PathTag::InPathEnv`] tag, or
    /// [`usize::MAX`] if this candidate did not come from PATH. Internal: used
    /// for deterministic tie-breaking in [`rank_candidates`].
    pub(crate) fn path_order(&self) -> usize {
        self.tags
            .iter()
            .find_map(|t| match t {
                PathTag::InPathEnv(order) => Some(*order),
                _ => None,
            })
            .unwrap_or(usize::MAX)
    }

    /// Construct a candidate from its parts. Internal only.
    fn new(path: PathBuf, canonical: PathBuf, tags: Vec<PathTag>) -> Self {
        Candidate {
            path,
            canonical,
            tags,
        }
    }
}

#[cfg(test)]
impl Candidate {
    /// Test-only constructor for building candidates from tags directly.
    pub(crate) fn for_test(path: PathBuf, canonical: PathBuf, tags: Vec<PathTag>) -> Self {
        Candidate::new(path, canonical, tags)
    }
}

/// Normalize a path for deduplication (case insensitive on Windows)
fn normalize_for_dedup(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Normalize a path by resolving `.` and `..` components without touching symlinks.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}

fn tag_path(path: &Path, input_canonical: &Path, input_path: &Path) -> (Vec<PathTag>, PathBuf) {
    let mut tags = Vec::new();

    // Relative
    if !path.is_absolute() {
        tags.push(PathTag::Relative);
    }

    // NonNormalized
    let path_str = path.to_string_lossy();
    if path_str.contains("/./")
        || path_str.contains("/../")
        || path_str.contains("\\.\\")
        || path_str.contains("\\..\\")
        || path.components().any(|c| {
            matches!(
                c,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        tags.push(PathTag::NonNormalized);
    }

    // SymlinkTo
    let symlink_target = fs::read_link(path).ok();
    if let Some(ref target) = symlink_target {
        tags.push(PathTag::SymlinkTo(target.clone()));
    }

    // Shim
    if path_analysis::is_shim_path(path) {
        tags.push(PathTag::Shim);
    } else if let Some(ref target) = symlink_target {
        let candidate_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target_name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !candidate_name.is_empty()
            && !target_name.is_empty()
            && path_analysis::is_shim_by_name(&candidate_name, &target_name)
        {
            tags.push(PathTag::Shim);
        }
    }

    // Canonical
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    // SameCanonical / SameContent / DifferentBinary
    if canonical == input_canonical {
        tags.push(PathTag::SameCanonical);
    } else if path_analysis::files_have_same_content(path, input_path) {
        tags.push(PathTag::SameContent);
    } else {
        tags.push(PathTag::DifferentBinary);
    }

    // ManagedBy
    if let Some(info) = path_analysis::detect_version_manager(path) {
        tags.push(PathTag::ManagedBy(info.name));
    }

    // BuildOutput
    if path_analysis::is_build_output(path) {
        tags.push(PathTag::BuildOutput);
    }

    // Ephemeral
    if path_analysis::is_ephemeral(path) {
        tags.push(PathTag::Ephemeral);
    }

    // NotExecutable: no execute permission, or bare relative path (no ./ or / prefix)
    if !path_analysis::is_executable(path) {
        tags.push(PathTag::NotExecutable);
    } else if !path.is_absolute() {
        let path_str = path.to_string_lossy();
        if !path_str.starts_with("./")
            && !path_str.starts_with("../")
            && !path_str.starts_with(".\\")
            && !path_str.starts_with("..\\")
        {
            tags.push(PathTag::NotExecutable);
        }
    }

    (tags, canonical)
}

/// Discover all candidate locations for `binary`, using `path_env` as the PATH
/// to search.
///
/// This is *discovery only*: candidates are returned in deterministic order
/// (input-derived variants first, then PATH matches in discovery order). Use
/// [`rank_candidates`] to order them by a [`ScoringPolicy`].
///
/// On success the returned `Vec` is **never empty**: the input itself is always
/// emitted as a candidate. An empty result is impossible — failures surface as
/// [`Err`] ([`Error::NotFound`] / [`Error::NotAFile`] / [`Error::NotInPath`]
/// etc.) instead.
///
/// # `path_env`
///
/// `path_env` is the PATH-equivalent search list, not a full environment.
///
/// - `None` does **not** mean "skip PATH search". If `binary` is a bare command
///   name (no path separator) and `path_env` is `None`, resolution fails with
///   [`Error::NotInPath`]. If `binary` is an explicit path, input-derived
///   candidates are still returned with `None`.
/// - On Windows, `PATHEXT` is **not** taken from this argument; it is read
///   directly from the process environment via [`std::env::var`]. See the
///   Design rationale in the source. Full env injection (PATHEXT as an
///   argument) is intentionally deferred (YAGNI).
pub fn find_candidates_with_path_env(
    binary: &Path,
    path_env: Option<OsString>,
) -> Result<Vec<Candidate>, Error> {
    // Command name resolution: if no path separator, look up in PATH
    let resolved_binary;
    let binary = if !binary.to_string_lossy().contains(std::path::MAIN_SEPARATOR)
        && !binary.to_string_lossy().contains('/')
    {
        let name = binary
            .file_name()
            .ok_or_else(|| Error::NoFileName(binary.to_path_buf()))?;
        resolved_binary = path_env
            .as_ref()
            .and_then(|pv| {
                env::split_paths(pv).find_map(|dir| {
                    // Try exact name first
                    let exact = dir.join(name);
                    if path_analysis::is_executable(&exact) {
                        return Some(exact);
                    }
                    // On Windows, also try with PATHEXT extensions.
                    // Design rationale: PATHEXT is read directly from the
                    // process environment rather than passed as an argument.
                    // `find_candidates_with_path_env` only injects PATH; full
                    // env injection (PATHEXT as a parameter) is deferred as
                    // YAGNI (DR-015 Decision 7).
                    #[cfg(windows)]
                    {
                        let pathext = std::env::var("PATHEXT")
                            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
                        for ext in pathext.split(';') {
                            let with_ext = dir.join(format!(
                                "{}{}",
                                name.to_string_lossy(),
                                ext.to_lowercase()
                            ));
                            if path_analysis::is_executable(&with_ext) {
                                return Some(with_ext);
                            }
                        }
                    }
                    None
                })
            })
            .ok_or_else(|| Error::NotInPath(binary.display().to_string()))?;
        resolved_binary.as_path()
    } else {
        binary
    };

    // Verify input exists and is a file
    if !binary.exists() {
        return Err(Error::NotFound(binary.to_path_buf()));
    }
    let metadata = fs::metadata(binary).map_err(|e| Error::Metadata(binary.to_path_buf(), e))?;
    if !metadata.is_file() {
        return Err(Error::NotAFile(binary.to_path_buf()));
    }

    // Compute input canonical path
    let input_canonical =
        fs::canonicalize(binary).map_err(|e| Error::Canonicalize(binary.to_path_buf(), e))?;

    let file_name = binary
        .file_name()
        .ok_or_else(|| Error::NoFileName(binary.to_path_buf()))?;

    let mut candidates = Vec::new();
    let mut path_order: usize = 0;
    let mut seen_paths = HashSet::new();

    // Generate candidates from the input path itself:
    // 1. Raw input (as-is)
    {
        let (mut tags, canonical) = tag_path(binary, &input_canonical, binary);
        tags.insert(0, PathTag::Input);
        seen_paths.insert(normalize_for_dedup(binary));
        candidates.push(Candidate::new(binary.to_path_buf(), canonical, tags));
    }

    // 2. ./ or .\ prefixed (bare relative path -> explicit relative path, if file exists)
    if !binary.is_absolute() {
        let path_str = binary.to_string_lossy();
        let has_explicit_prefix = path_str.starts_with("./")
            || path_str.starts_with("../")
            || path_str.starts_with(".\\")
            || path_str.starts_with("..\\");
        let has_separator = path_str.contains('/') || path_str.contains('\\');
        if !has_explicit_prefix && has_separator {
            let prefix = if cfg!(windows) { ".\\" } else { "./" };
            let dot_prefixed = PathBuf::from(format!("{prefix}{path_str}"));
            let dedup_key = normalize_for_dedup(&dot_prefixed);
            if dot_prefixed.exists() && !seen_paths.contains(&dedup_key) {
                seen_paths.insert(dedup_key);
                let (tags, canonical) = tag_path(&dot_prefixed, &input_canonical, binary);
                candidates.push(Candidate::new(dot_prefixed, canonical, tags));
            }
        }
    }

    // 3. Absolutized (relative -> absolute, without resolving symlinks)
    if !binary.is_absolute()
        && let Ok(cwd) = env::current_dir()
    {
        let absolute = normalize_path(&cwd.join(binary));
        let dedup_key = normalize_for_dedup(&absolute);
        if !seen_paths.contains(&dedup_key) {
            seen_paths.insert(dedup_key);
            let (tags, canonical) = tag_path(&absolute, &input_canonical, binary);
            candidates.push(Candidate::new(absolute, canonical, tags));
        }
    }

    // 4. Canonical (all symlinks resolved)
    {
        let dedup_key = normalize_for_dedup(&input_canonical);
        if !seen_paths.contains(&dedup_key) {
            seen_paths.insert(dedup_key);
            let (tags, canonical) = tag_path(&input_canonical, &input_canonical, binary);
            candidates.push(Candidate::new(input_canonical.clone(), canonical, tags));
        }
    }

    // 5. Symlink target (one level of resolution, if input is a symlink)
    if let Ok(link_target) = fs::read_link(binary) {
        let link_target = if link_target.is_relative() {
            // Join against the symlink's directory, then normalize away `.`/`..`
            // (without touching symlinks) so the result is consistent with the
            // absolutized candidate path (variant 3) and dedups correctly. A
            // raw join can leave `..` components that would otherwise add a
            // spurious NonNormalized tag and miss dedup.
            let joined = binary
                .parent()
                .map(|p| p.join(&link_target))
                .unwrap_or(link_target);
            normalize_path(&joined)
        } else {
            link_target
        };
        let dedup_key = normalize_for_dedup(&link_target);
        if !seen_paths.contains(&dedup_key) {
            seen_paths.insert(dedup_key);
            let (tags, canonical) = tag_path(&link_target, &input_canonical, binary);
            candidates.push(Candidate::new(link_target, canonical, tags));
        }
    }

    // Search PATH for same-name binaries
    if let Some(ref path_var) = path_env {
        for dir in env::split_paths(path_var) {
            #[allow(unused_mut)]
            let mut candidates_in_dir = vec![dir.join(file_name)];

            // On Windows, also try PATHEXT extensions
            #[cfg(windows)]
            {
                let pathext =
                    std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
                let name_str = file_name.to_string_lossy();
                for ext in pathext.split(';') {
                    candidates_in_dir.push(dir.join(format!("{}{}", name_str, ext.to_lowercase())));
                }
            }

            for candidate_path in candidates_in_dir {
                if candidate_path == binary {
                    continue;
                }
                if !path_analysis::is_executable(&candidate_path) {
                    continue;
                }
                // Skip duplicate PATH entries (same directory appearing multiple times in PATH)
                if !seen_paths.insert(normalize_for_dedup(&candidate_path)) {
                    continue;
                }
                let (mut tags, canonical) = tag_path(&candidate_path, &input_canonical, binary);
                tags.insert(0, PathTag::InPathEnv(path_order));
                candidates.push(Candidate::new(candidate_path, canonical, tags));
                path_order += 1;
            }
        }
    }

    Ok(candidates)
}

/// Discover all candidate locations for `binary`, searching the process `PATH`.
///
/// This is *discovery only* and returns candidates in deterministic
/// (PATH discovery) order. Use [`rank_candidates`] to order them by a
/// [`ScoringPolicy`]. A thin wrapper over [`find_candidates_with_path_env`]
/// that reads `PATH` from the process environment.
///
/// On success the returned `Vec` is **never empty** (see
/// [`find_candidates_with_path_env`]).
pub fn find_candidates(binary: &Path) -> Result<Vec<Candidate>, Error> {
    find_candidates_with_path_env(binary, env::var_os("PATH"))
}

/// Sort `candidates` in place by score descending, with PATH discovery order
/// ascending as a deterministic tie-breaker.
///
/// `score` is policy-dependent and internal; ranking is expressed solely
/// through this ordering. Takes `&mut [Candidate]` (Rust slice-sort idiom) so
/// the caller keeps ownership of its `Vec`.
pub fn rank_candidates(candidates: &mut [Candidate], policy: ScoringPolicy) {
    candidates.sort_by(|a, b| {
        let score_cmp = b.score(policy).cmp(&a.score(policy));
        score_cmp.then(a.path_order().cmp(&b.path_order()))
    });
}

/// Resolve the single best candidate for `binary` under `policy`.
///
/// Convenience composition of [`find_candidates`] + [`rank_candidates`]:
/// returns the top-ranked candidate. Because `find_candidates` is non-empty on
/// success, this always returns a tagged candidate when it returns [`Ok`]
/// (never a fabricated tagless one).
pub fn resolve_stable_path(binary: &Path, policy: ScoringPolicy) -> Result<Candidate, Error> {
    let mut candidates = find_candidates(binary)?;
    rank_candidates(&mut candidates, policy);
    // `find_candidates` guarantees a non-empty Vec on success (the input is
    // always emitted), so the first element exists.
    Ok(candidates
        .into_iter()
        .next()
        .expect("find_candidates returns a non-empty Vec on success"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    // --- score() tests ---

    fn make_candidate(tags: Vec<PathTag>) -> Candidate {
        Candidate::for_test(PathBuf::from("/dummy"), PathBuf::from("/dummy"), tags)
    }

    #[test]
    fn test_score_same_binary_policy_highest() {
        // SameCanonical + InPathEnv should be the highest score in SameBinary policy
        let c = make_candidate(vec![PathTag::SameCanonical, PathTag::InPathEnv(0)]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=3, in_path=5 => 3*1000 + 3*10 + 5 = 3035
        assert_eq!(score, 3035);
    }

    #[test]
    fn test_score_same_content_in_path() {
        let c = make_candidate(vec![PathTag::SameContent, PathTag::InPathEnv(0)]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=2, stability=3, in_path=5 => 2*1000 + 3*10 + 5 = 2035
        assert_eq!(score, 2035);
    }

    #[test]
    fn test_score_different_binary() {
        let c = make_candidate(vec![PathTag::DifferentBinary, PathTag::InPathEnv(0)]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=0, stability=3, in_path=5 => 0 + 30 + 5 = 35
        assert_eq!(score, 35);
    }

    #[test]
    fn test_score_stable_policy_stable_path_wins() {
        // In Stable policy, a DifferentBinary on stable path beats SameCanonical on unstable
        let stable_different =
            make_candidate(vec![PathTag::DifferentBinary, PathTag::InPathEnv(0)]);
        let unstable_same = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::BuildOutput,
        ]);
        let stable_score = stable_different.score(ScoringPolicy::Stable);
        let unstable_score = unstable_same.score(ScoringPolicy::Stable);
        // stable_different: stability=3, binary=0 => 3*1000 + 0*10 + 5 = 3005
        // unstable_same: stability=0, binary=3 => 0*1000 + 3*10 + 5 = 35
        assert_eq!(stable_score, 3005);
        assert_eq!(unstable_score, 35);
        assert!(stable_score > unstable_score);
    }

    #[test]
    fn test_score_relative_penalty() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::Relative,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // 3*1000 + 3*10 + 5 - 3 = 3032
        assert_eq!(score, 3032);
    }

    #[test]
    fn test_score_non_normalized_penalty() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::NonNormalized,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // 3*1000 + 3*10 + 5 - 2 = 3033
        assert_eq!(score, 3033);
    }

    #[test]
    fn test_score_both_penalties_accumulate() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::Relative,
            PathTag::NonNormalized,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // 3*1000 + 3*10 + 5 - 3 - 2 = 3030
        assert_eq!(score, 3030);
    }

    #[test]
    fn test_score_build_output_zero_stability() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::BuildOutput,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=0, in_path=5 => 3*1000 + 0 + 5 = 3005
        assert_eq!(score, 3005);
    }

    #[test]
    fn test_score_ephemeral_zero_stability() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::Ephemeral,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=0, in_path=5 => 3*1000 + 0 + 5 = 3005
        assert_eq!(score, 3005);
    }

    #[test]
    fn test_score_managed_by_stability() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::ManagedBy("mise".to_string()),
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=1, in_path=5 => 3*1000 + 1*10 + 5 = 3015
        assert_eq!(score, 3015);
    }

    #[test]
    fn test_score_shim_stability() {
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::Shim,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=1, in_path=5 => 3*1000 + 1*10 + 5 = 3015
        assert_eq!(score, 3015);
    }

    #[test]
    fn test_score_managed_and_shim_both_present() {
        // Both ManagedBy and Shim: stability = 1 (same as having just one)
        let c = make_candidate(vec![
            PathTag::SameCanonical,
            PathTag::InPathEnv(0),
            PathTag::ManagedBy("mise".to_string()),
            PathTag::Shim,
        ]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=1, in_path=5 => 3*1000 + 1*10 + 5 = 3015
        assert_eq!(score, 3015);
    }

    #[test]
    fn test_score_no_in_path_bonus() {
        let c = make_candidate(vec![PathTag::SameCanonical]);
        let score = c.score(ScoringPolicy::SameBinary);
        // binary=3, stability=3, in_path=0 => 3*1000 + 3*10 + 0 = 3030
        assert_eq!(score, 3030);
    }

    // --- find_candidates_with_path_env tests ---

    #[cfg(unix)]
    struct TestFixture {
        _tmpdir: tempfile::TempDir,
        real_binary: PathBuf,
        stable_link: PathBuf,
        stable_dir: PathBuf,
        other_binary: PathBuf,
        other_dir: PathBuf,
    }

    #[cfg(unix)]
    impl TestFixture {
        fn new() -> Self {
            let tmpdir = tempfile::tempdir().unwrap();
            let base = tmpdir.path();

            let real_dir = base.join("real");
            let stable_dir = base.join("stable_dir");
            let other_dir = base.join("other_dir");

            fs::create_dir_all(&real_dir).unwrap();
            fs::create_dir_all(&stable_dir).unwrap();
            fs::create_dir_all(&other_dir).unwrap();

            let real_binary = real_dir.join("mybinary");
            fs::write(&real_binary, "real-content").unwrap();
            fs::set_permissions(&real_binary, fs::Permissions::from_mode(0o755)).unwrap();

            let stable_link = stable_dir.join("mybinary");
            symlink(&real_binary, &stable_link).unwrap();

            let other_binary = other_dir.join("mybinary");
            fs::write(&other_binary, "other-content").unwrap();
            fs::set_permissions(&other_binary, fs::Permissions::from_mode(0o755)).unwrap();

            TestFixture {
                _tmpdir: tmpdir,
                real_binary,
                stable_link,
                stable_dir,
                other_binary,
                other_dir,
            }
        }

        fn make_path(&self, dirs: &[&Path]) -> OsString {
            env::join_paths(dirs).unwrap()
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_same_canonical() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        // Should have at least 2 candidates: input (+ derived) + symlink from PATH
        assert!(candidates.len() >= 2);

        // The symlink candidate should have SameCanonical tag
        let symlink_cand = candidates.iter().find(|c| c.path == f.stable_link).unwrap();
        assert!(symlink_cand.tags.contains(&PathTag::SameCanonical));
        assert!(
            symlink_cand
                .tags
                .iter()
                .any(|t| matches!(t, PathTag::InPathEnv(_)))
        );
        assert!(
            symlink_cand
                .tags
                .iter()
                .any(|t| matches!(t, PathTag::SymlinkTo(_)))
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_different_binary_tagged() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.other_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        let other_cand = candidates
            .iter()
            .find(|c| c.path == f.other_binary)
            .unwrap();
        assert!(other_cand.tags.contains(&PathTag::DifferentBinary));
    }

    #[cfg(unix)]
    #[test]
    fn test_no_path_matches_returns_input_only() {
        let f = TestFixture::new();
        let empty_dir = f._tmpdir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();
        let path_env = f.make_path(&[&empty_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        // At least the input candidate (+ possibly canonical variant)
        assert!(!candidates.is_empty());
        assert!(candidates.iter().any(|c| c.tags.contains(&PathTag::Input)));
    }

    #[cfg(unix)]
    #[test]
    fn test_input_tag_present() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        let input_cand = candidates.iter().find(|c| c.path == f.real_binary).unwrap();
        assert!(input_cand.tags.contains(&PathTag::Input));
    }

    #[cfg(unix)]
    #[test]
    fn test_in_path_env_tag_present() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        let path_cand = candidates.iter().find(|c| c.path == f.stable_link).unwrap();
        assert!(
            path_cand
                .tags
                .iter()
                .any(|t| matches!(t, PathTag::InPathEnv(_)))
        );
        // Input candidate should NOT have InPathEnv
        let input_cand = candidates.iter().find(|c| c.path == f.real_binary).unwrap();
        assert!(
            !input_cand
                .tags
                .iter()
                .any(|t| matches!(t, PathTag::InPathEnv(_)))
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_to_tag_present() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        let symlink_cand = candidates.iter().find(|c| c.path == f.stable_link).unwrap();
        let has_symlink_tag = symlink_cand
            .tags
            .iter()
            .any(|t| matches!(t, PathTag::SymlinkTo(_)));
        assert!(has_symlink_tag);
    }

    #[cfg(unix)]
    #[test]
    fn test_same_content_detected() {
        // Create a copy (not symlink) with identical content
        let tmpdir = tempfile::tempdir().unwrap();
        let base = tmpdir.path();

        let dir_a = base.join("a");
        let dir_b = base.join("b");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();

        let binary_a = dir_a.join("mybin");
        let binary_b = dir_b.join("mybin");
        fs::write(&binary_a, "identical-content").unwrap();
        fs::set_permissions(&binary_a, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&binary_b, "identical-content").unwrap();
        fs::set_permissions(&binary_b, fs::Permissions::from_mode(0o755)).unwrap();

        let path_env = env::join_paths([&dir_b]).unwrap();
        let candidates = find_candidates_with_path_env(&binary_a, Some(path_env)).unwrap();

        let copy_cand = candidates.iter().find(|c| c.path == binary_b).unwrap();
        assert!(copy_cand.tags.contains(&PathTag::SameContent));
        // Should NOT have SameCanonical since they are different files
        assert!(!copy_cand.tags.contains(&PathTag::SameCanonical));
    }

    #[cfg(unix)]
    #[test]
    fn test_rank_candidates_sorts_by_score_descending() {
        let f = TestFixture::new();
        // stable_dir has symlink (SameCanonical), other_dir has different binary
        let path_env = f.make_path(&[&f.other_dir, &f.stable_dir]);

        // find_candidates_with_path_env is discovery-only (unsorted); ranking
        // is a separate step.
        let mut candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();
        rank_candidates(&mut candidates, ScoringPolicy::SameBinary);

        // Verify scores are in descending order after ranking
        let scores: Vec<i32> = candidates
            .iter()
            .map(|c| c.score(ScoringPolicy::SameBinary))
            .collect();
        for w in scores.windows(2) {
            assert!(w[0] >= w[1], "scores not descending: {:?}", scores);
        }
    }

    #[test]
    fn test_nonexistent_binary_error() {
        let result = find_candidates_with_path_env(Path::new("/nonexistent/binary"), None);
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_duplicate_input_path_skipped() {
        // When input path is in PATH, it should not appear twice
        let f = TestFixture::new();
        let real_dir = f.real_binary.parent().unwrap();
        let path_env = f.make_path(&[real_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        let count = candidates
            .iter()
            .filter(|c| c.path == f.real_binary)
            .count();
        assert_eq!(count, 1, "input path should appear exactly once");
    }

    #[cfg(unix)]
    #[test]
    fn test_duplicate_path_directory_deduped() {
        let f = TestFixture::new();
        // Same directory appears twice in PATH
        let path_env = f.make_path(&[&f.stable_dir, &f.stable_dir]);

        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();

        let stable_count = candidates
            .iter()
            .filter(|c| c.path == f.stable_link)
            .count();
        assert_eq!(
            stable_count, 1,
            "duplicate PATH directory should be deduped"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_command_name_lookup() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let candidates =
            find_candidates_with_path_env(Path::new("mybinary"), Some(path_env)).unwrap();

        assert!(!candidates.is_empty());
        assert!(candidates[0].tags.contains(&PathTag::Input));
    }

    #[test]
    fn test_command_name_not_found() {
        let tmpdir = tempfile::tempdir().unwrap();
        let empty_dir = tmpdir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();
        let path_env = env::join_paths([&empty_dir]).unwrap();

        let result = find_candidates_with_path_env(Path::new("nonexistent"), Some(path_env));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotInPath(_)));
    }

    #[test]
    fn test_stable_policy_prefers_stable_different_binary_over_unstable_same() {
        // Stable policy: stable DifferentBinary > unstable SameCanonical
        let stable = make_candidate(vec![PathTag::DifferentBinary, PathTag::InPathEnv(0)]);
        let unstable = make_candidate(vec![PathTag::SameCanonical, PathTag::BuildOutput]);
        assert!(stable.score(ScoringPolicy::Stable) > unstable.score(ScoringPolicy::Stable));
        // But SameBinary policy reverses this
        assert!(
            unstable.score(ScoringPolicy::SameBinary) > stable.score(ScoringPolicy::SameBinary)
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_shim_directory_detected() {
        let tmpdir = tempfile::tempdir().unwrap();
        let shim_dir = tmpdir.path().join(".mise").join("shims");
        fs::create_dir_all(&shim_dir).unwrap();
        let shim_binary = shim_dir.join("mybin");
        fs::write(&shim_binary, "shim-content").unwrap();
        fs::set_permissions(&shim_binary, fs::Permissions::from_mode(0o755)).unwrap();

        let other_dir = tmpdir.path().join("other");
        fs::create_dir_all(&other_dir).unwrap();
        let other_binary = other_dir.join("mybin");
        fs::write(&other_binary, "other-content").unwrap();
        fs::set_permissions(&other_binary, fs::Permissions::from_mode(0o755)).unwrap();

        let path_env = env::join_paths([&shim_dir, &other_dir]).unwrap();
        let candidates = find_candidates_with_path_env(&other_binary, Some(path_env)).unwrap();

        let shim_cand = candidates.iter().find(|c| c.path == shim_binary).unwrap();
        assert!(shim_cand.tags.contains(&PathTag::Shim));
    }

    #[cfg(unix)]
    #[test]
    fn test_shim_by_symlink_name_mismatch() {
        let tmpdir = tempfile::tempdir().unwrap();
        let base = tmpdir.path();

        let real_dir = base.join("real");
        let shim_dir = base.join("bin");
        fs::create_dir_all(&real_dir).unwrap();
        fs::create_dir_all(&shim_dir).unwrap();

        // Real binary named "jj-worktree"
        let real_binary = real_dir.join("jj-worktree");
        fs::write(&real_binary, "binary-content").unwrap();
        fs::set_permissions(&real_binary, fs::Permissions::from_mode(0o755)).unwrap();

        // Symlink "git" -> "jj-worktree" (name mismatch = shim)
        let shim_link = shim_dir.join("git");
        symlink(&real_binary, &shim_link).unwrap();

        // Also create a real "git" binary for input
        let input_dir = base.join("input");
        fs::create_dir_all(&input_dir).unwrap();
        let input_binary = input_dir.join("git");
        fs::write(&input_binary, "real-git-content").unwrap();
        fs::set_permissions(&input_binary, fs::Permissions::from_mode(0o755)).unwrap();

        let path_env = env::join_paths([&shim_dir]).unwrap();
        let candidates = find_candidates_with_path_env(&input_binary, Some(path_env)).unwrap();

        let shim_cand = candidates.iter().find(|c| c.path == shim_link).unwrap();
        assert!(shim_cand.tags.contains(&PathTag::Shim));
    }

    #[cfg(unix)]
    #[test]
    fn test_version_suffix_symlink_not_shim() {
        let tmpdir = tempfile::tempdir().unwrap();
        let base = tmpdir.path();

        let real_dir = base.join("real");
        let link_dir = base.join("bin");
        fs::create_dir_all(&real_dir).unwrap();
        fs::create_dir_all(&link_dir).unwrap();

        let real_binary = real_dir.join("python3.12");
        fs::write(&real_binary, "python-content").unwrap();
        fs::set_permissions(&real_binary, fs::Permissions::from_mode(0o755)).unwrap();

        // "python3" -> "python3.12" (prefix match = NOT shim)
        let link = link_dir.join("python3");
        symlink(&real_binary, &link).unwrap();

        let input_dir = base.join("input");
        fs::create_dir_all(&input_dir).unwrap();
        let input_binary = input_dir.join("python3");
        fs::write(&input_binary, "python-content").unwrap();
        fs::set_permissions(&input_binary, fs::Permissions::from_mode(0o755)).unwrap();

        let path_env = env::join_paths([&link_dir]).unwrap();
        let candidates = find_candidates_with_path_env(&input_binary, Some(path_env)).unwrap();

        let link_cand = candidates.iter().find(|c| c.path == link).unwrap();
        assert!(!link_cand.tags.contains(&PathTag::Shim));
    }

    // --- accessors / durability / 3-layer API ---

    #[test]
    fn test_durability_accessor_per_candidate() {
        // Reference surface is durable; versioned canonical is not.
        let durable = Candidate::for_test(
            PathBuf::from("/opt/homebrew/bin/git"),
            PathBuf::from("/opt/homebrew/Cellar/git/2.44.0/bin/git"),
            vec![PathTag::Input],
        );
        assert_eq!(durable.durability(), Durability::Durable);
        assert!(durable.is_stable());

        let versioned = Candidate::for_test(
            PathBuf::from("/opt/homebrew/Cellar/git/2.44.0/bin/git"),
            PathBuf::from("/opt/homebrew/Cellar/git/2.44.0/bin/git"),
            vec![PathTag::SameCanonical],
        );
        assert_eq!(versioned.durability(), Durability::NotDurable);
        assert!(!versioned.is_stable());
    }

    #[test]
    fn test_is_stable_false_for_unknown() {
        let c = Candidate::for_test(
            PathBuf::from("/home/u/.local/bin/jupyter"),
            PathBuf::from("/home/u/.local/bin/jupyter"),
            vec![PathTag::Input],
        );
        assert_eq!(c.durability(), Durability::Unknown);
        assert!(!c.is_stable());
    }

    #[test]
    fn test_accessors_return_internal_fields() {
        let c = Candidate::for_test(
            PathBuf::from("/a/b"),
            PathBuf::from("/c/d"),
            vec![PathTag::Input, PathTag::SameCanonical],
        );
        assert_eq!(c.path(), Path::new("/a/b"));
        assert_eq!(c.canonical(), Path::new("/c/d"));
        assert_eq!(c.tags(), &[PathTag::Input, PathTag::SameCanonical]);
    }

    #[cfg(unix)]
    #[test]
    fn test_find_candidates_with_path_env_is_unsorted_discovery() {
        // Discovery order is input-derived first, then PATH order. Not sorted
        // by score. The input candidate stays at index 0.
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.other_dir, &f.stable_dir]);
        let candidates = find_candidates_with_path_env(&f.real_binary, Some(path_env)).unwrap();
        assert!(candidates[0].tags().contains(&PathTag::Input));
    }

    #[cfg(unix)]
    #[test]
    fn test_find_candidates_non_empty_on_success() {
        let f = TestFixture::new();
        // Explicit path with empty PATH search: still returns the input.
        let candidates = find_candidates_with_path_env(&f.real_binary, None).unwrap();
        assert!(!candidates.is_empty());
        assert!(
            candidates
                .iter()
                .any(|c| c.tags().contains(&PathTag::Input))
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_stable_path_returns_tagged_candidate() {
        let f = TestFixture::new();
        // resolve_stable_path uses the process PATH; the input is an explicit
        // path so a tagged candidate is always returned.
        let best = resolve_stable_path(&f.real_binary, ScoringPolicy::SameBinary).unwrap();
        assert!(!best.tags().is_empty());
    }
}
