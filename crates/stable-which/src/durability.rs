//! Durability judging: can a path be pinned into a service definition?
//!
//! See `DR-016` (durable-to-pin model) in the repository's `docs/decisions/`.
//! This module is internal; the public surface is [`crate::Durability`] and
//! [`crate::Candidate::durability`]. (The DR reference is an in-repo document,
//! not a docs.rs link.)

use std::path::{Component, Path};

use crate::path_analysis;

/// Whether a path string keeps pointing at the same (logical) target across
/// rebuild / upgrade / reboot.
///
/// This is a *durable-to-pin* judgement: a [`Durable`](Durability::Durable)
/// path may be baked into a launchd plist / systemd unit and is expected to
/// survive upgrades and reboots. Judgement is **per candidate** — for the same
/// binary, the reference path (`/opt/homebrew/bin/git`) can be
/// [`Durable`](Durability::Durable) while its canonical realpath
/// (`/opt/homebrew/Cellar/git/2.44.0/bin/git`) is
/// [`NotDurable`](Durability::NotDurable).
///
/// # Judging model (allow-list)
///
/// 1. `NotDurable` is decided first: versioned-install / ephemeral /
///    build-output / project-local patterns.
/// 2. Otherwise, if the path is under a known durable location (the
///    "environment-wide reference surfaces": standard system bins, profile
///    bins matched by their exact structure, and shims at standard
///    HOME-anchored locations), it is `Durable`.
/// 3. Otherwise `Unknown` (treated as not-durable by
///    [`Candidate::is_stable`](crate::Candidate::is_stable)).
///
/// User dropbox directories (`~/bin`, `~/.local/bin`) are deliberately *not*
/// in the durable allow-list: they mix shebang-embedded scripts and versioned
/// symlinks, so they fall through to `Unknown`.
///
/// # Known limitations
///
/// Judgement is path-string based and cannot detect:
///
/// - **shebang-embedded versioned dependencies**: e.g. a script in a standard
///   reference surface whose shebang points at a versioned interpreter
///   (`cpython@<ver>`). Such a path may be judged `Durable` yet break on
///   upgrade. This is why shebang-prone user dropboxes are excluded from the
///   allow-list (see above), but the case cannot be ruled out entirely.
/// - **environment-dependent paths**: e.g. `~/.nix-profile` is alive in some
///   environments and dead in others; durability cannot be confirmed without
///   inspecting the realpath.
/// - **non-UTF-8 paths**: pattern matching goes through
///   [`Path::to_string_lossy`], so a path containing non-UTF-8 bytes in a
///   matched segment may compare unexpectedly; such paths typically fall
///   through to `Unknown` (safe side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Durability {
    /// The path string keeps pointing at the same entity (or same logical
    /// target) across rebuild / upgrade / reboot.
    Durable,
    /// Versioned-install / ephemeral / build-output / project-local etc.;
    /// baking this path into a service definition can break it.
    NotDurable,
    /// Matched neither a known durable nor a known not-durable pattern; cannot
    /// decide (safe side = treated as not-durable).
    Unknown,
}

/// Internal scope of a path. Not exposed publicly (DR-016 Decision 2):
/// project-local is folded into [`Durability::NotDurable`].
///
/// Kept as an enum (rather than a bool) as a deliberate placeholder for the
/// future: if DR-016 Decision 2's "expose scope" option is ever taken, the
/// public `Scope` enum can grow from this internal one without a churny
/// bool→enum migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// Scoped to a single project (`.direnv/`, `venv/`, `node_modules/.bin/`).
    /// A global service must not reference it.
    ProjectLocal,
    /// Not project-scoped (the common case).
    Global,
}

/// Project-local path fragments. A global service should never reference these.
const PROJECT_LOCAL_PATTERNS: &[&str] = &[
    "/.direnv/",
    "/node_modules/.bin/",
    "/.venv/",
    "/venv/bin/",
    "/.tox/",
];

/// Versioned-install path fragments: the path embeds a version identifier, so
/// it changes on upgrade. These are NotDurable.
const VERSIONED_INSTALL_PATTERNS: &[&str] = &[
    // Homebrew Cellar / Caskroom
    "/Cellar/",
    "/Caskroom/",
    // mise / asdf installs
    "/mise/installs/",
    "/.mise/installs/",
    "/asdf/installs/",
    "/.asdf/installs/",
    // nix store (hash + version in first segment)
    "/nix/store/",
    // version-manager versions/ trees
    "/.nvm/versions/",
    "/.fnm/node-versions/",
    "/.rustup/toolchains/",
    "/.volta/tools/",
    "/.sdkman/candidates/",
    "/.pyenv/versions/",
    "/.rbenv/versions/",
    "/.goenv/versions/",
    "/.proto/tools/",
    "/scoop/apps/",
    "/chocolatey/lib/",
    "/WinGet/Packages/",
    // aqua internal packages
    "/aquaproj-aqua/internal/pkgs/",
    // app-specific versioned trees, anchored to the specific tool name to avoid
    // false positives from an unrelated generic `/versions/` segment (findings:
    // claude / cursor-agent each keep a `versions/<ver>/` tree).
    "/claude/versions/",
    "/cursor-agent/versions/",
    // rye cpython interpreters
    "/cpython@",
];

/// Durable directory surfaces where the binary lives **directly** in the dir
/// (`<dir>/<file>`). These are matched by exact parent-directory equality, not
/// substring, so `/usr/local/binutils/foo` or `/opt/homebrew/binfoo/git` do
/// not match.
///
/// Design rationale: `/usr/local/bin` is included because it is a standard,
/// environment-wide reference surface populated by `make install`, distro
/// packages, and `brew link` symlinks — the reference path is stable across
/// upgrades. The residual risk is a tool that itself places a *versioned
/// symlink* directly in `/usr/local/bin` (rare); that case is a known
/// limitation of path-string judgement (a `Durable` verdict that could still
/// break) and is documented on [`Durability`].
const DURABLE_DIRECT_DIRS: &[&str] = &[
    // standard system bins
    "/usr/bin",
    "/bin",
    "/usr/local/bin",
    "/usr/sbin",
    "/sbin",
    // homebrew profile bins (the symlink surface, not Cellar)
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    // nix-darwin / Determinate Nix profile surfaces
    "/run/current-system/sw/bin",
    "/nix/var/nix/profiles/default/bin",
];

/// Standard shim directory suffixes (HOME-anchored). A shim path is treated as
/// durable only when it both matches one of these and is anchored under the
/// resolved HOME, so a project-local `/repo/.mise/shims/tool` does not qualify.
const STANDARD_SHIM_SUFFIXES: &[&str] = &[
    "/.local/share/mise/shims",
    "/.mise/shims",
    "/.asdf/shims",
    "/.pyenv/shims",
    "/.rbenv/shims",
    "/.goenv/shims",
    "/.proto/shims",
];

/// Normalize a path string to forward slashes for pattern matching.
fn norm(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Detect whether a path is project-local (scoped to a single project).
fn scope_of(path: &Path) -> Scope {
    let s = norm(path);
    if PROJECT_LOCAL_PATTERNS.iter().any(|p| s.contains(p)) {
        Scope::ProjectLocal
    } else {
        Scope::Global
    }
}

/// Detect whether a path embeds a version identifier (versioned install).
fn is_versioned_install(path: &Path) -> bool {
    let s = norm(path);
    VERSIONED_INSTALL_PATTERNS.iter().any(|p| s.contains(p))
}

/// Collect the normal (named) path components of `path` as strings.
///
/// Only [`Component::Normal`] segments are kept; root / prefix / `.` / `..`
/// are dropped. A path with any `..` is considered un-normalized by callers and
/// will not match the strict structural checks below.
fn normal_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(os) => Some(os.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// `/etc/profiles/per-user/<user>/bin/<file>` (home-manager profile surface).
///
/// Strictly require the exact structure: components `etc`, `profiles`,
/// `per-user`, `<user>`, `bin`, `<file>` with nothing in between and the file
/// as the last component. This rejects
/// `/etc/profiles/per-user/alice/lib/python/bin/foo` and a non-absolute
/// `home/u/etc/profiles/per-user/alice/bin/foo`.
fn is_etc_profiles_per_user_bin(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let c = normal_components(path);
    c.len() == 6
        && c[0] == "etc"
        && c[1] == "profiles"
        && c[2] == "per-user"
        // c[3] is the user name (any single segment)
        && c[4] == "bin"
    // c[5] is the file name (last component); len()==6 guarantees terminal.
}

/// Whether the binary lives directly in one of [`DURABLE_DIRECT_DIRS`]
/// (`<dir>/<file>`), matched by exact parent equality on an absolute path.
fn is_durable_direct_dir(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    // file_name() must exist (so `path` is `<dir>/<file>`, not `<dir>/`).
    if path.file_name().is_none() {
        return false;
    }
    let parent_s = norm(parent);
    DURABLE_DIRECT_DIRS.iter().any(|&d| parent_s == d)
}

/// Whether `path` is a shim at a standard HOME-anchored location.
///
/// `home` is the resolved HOME directory; when `None`, a HOME-anchored shim
/// cannot be confirmed and the function returns `false` (→ `Unknown`).
fn is_standard_shim(path: &Path, home: Option<&Path>) -> bool {
    // The shim must be recognized as a shim by the shared detector first.
    if !path_analysis::is_shim_path(path) {
        return false;
    }
    let Some(home) = home else {
        return false;
    };
    if !path.is_absolute() {
        return false;
    }
    let home_s = {
        let mut h = norm(home);
        while h.ends_with('/') {
            h.pop();
        }
        h
    };
    if home_s.is_empty() {
        return false;
    }
    let s = norm(path);
    // The shim dir must be `<home><suffix>/<file>`: HOME-anchored, with exactly
    // the standard shims directory, then a single file segment.
    STANDARD_SHIM_SUFFIXES.iter().any(|suffix| {
        let dir = format!("{home_s}{suffix}");
        let with_sep = format!("{dir}/");
        // `s` starts with `<home><suffix>/` and the remainder is a single file
        // segment (no further `/`).
        s.strip_prefix(&with_sep)
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
    })
}

/// Detect whether a path lives at a known durable reference surface.
///
/// Each surface is matched by its exact structure (direct-dir parent equality,
/// the strict per-user profile shape, or a HOME-anchored standard shim), never
/// by loose substring. Unrecognized locations fall through to `Unknown`.
fn is_durable_location(path: &Path, home: Option<&Path>) -> bool {
    is_durable_direct_dir(path)
        || is_etc_profiles_per_user_bin(path)
        || is_standard_shim(path, home)
}

/// Judge the durability of a candidate path, resolving HOME from the process
/// environment. See [`Durability`].
pub fn judge(path: &Path) -> Durability {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    judge_with_home(path, home.as_deref())
}

/// Judge durability with an explicit HOME (for testability). See [`Durability`].
fn judge_with_home(path: &Path, home: Option<&Path>) -> Durability {
    // 1. NotDurable is decided first.
    if is_versioned_install(path)
        || path_analysis::is_ephemeral(path)
        || path_analysis::is_build_output(path)
        || scope_of(path) == Scope::ProjectLocal
    {
        return Durability::NotDurable;
    }

    // 2. Known durable reference surfaces.
    if is_durable_location(path, home) {
        return Durability::Durable;
    }

    // 3. Fall through: cannot decide.
    Durability::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const HOME: &str = "/home/u";

    fn judge_str(s: &str) -> Durability {
        // Most tests anchor shims under "/home/u".
        judge_with_home(Path::new(s), Some(Path::new(HOME)))
    }

    // --- NotDurable: versioned-install (findings real examples) ---

    #[test]
    fn homebrew_cellar_is_not_durable() {
        assert_eq!(
            judge_str("/opt/homebrew/Cellar/node/26.0.0/bin/node"),
            Durability::NotDurable
        );
    }

    #[test]
    fn nix_store_is_not_durable() {
        assert_eq!(
            judge_str("/nix/store/abc123hash-git-2.54.0/bin/git"),
            Durability::NotDurable
        );
    }

    #[test]
    fn mise_installs_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/.local/share/mise/installs/node/22.22.0/bin/node"),
            Durability::NotDurable
        );
    }

    #[test]
    fn claude_versions_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/.local/share/claude/versions/2.1.177/bin/claude"),
            Durability::NotDurable
        );
    }

    #[test]
    fn cursor_agent_versions_is_not_durable() {
        assert_eq!(
            judge_str(
                "/home/u/.local/share/cursor-agent/versions/2025.09.17-25b418f/bin/cursor-agent"
            ),
            Durability::NotDurable
        );
    }

    #[test]
    fn generic_versions_segment_is_not_versioned_install() {
        // H: the generic `/versions/` pattern was removed. An unrelated dir
        // named "versions" should NOT be classified as a versioned install
        // (falls through to Unknown here, not NotDurable-by-versioned).
        assert_eq!(
            judge_str("/home/u/myapp/versions/data/tool"),
            Durability::Unknown
        );
    }

    #[test]
    fn rye_cpython_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/.dotfiles/cache/rye/py/cpython@3.12.8/bin/python"),
            Durability::NotDurable
        );
    }

    #[test]
    fn aqua_internal_pkgs_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/.local/share/aquaproj-aqua/internal/pkgs/foo/v2.56.2/bin/aqua"),
            Durability::NotDurable
        );
    }

    // --- NotDurable: ephemeral / build-output / project-local ---

    #[test]
    fn cache_dir_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/.cache/jj-worktree/bin/git"),
            Durability::NotDurable
        );
    }

    #[test]
    fn build_output_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/project/target/release/myapp"),
            Durability::NotDurable
        );
    }

    #[test]
    fn direnv_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/project/.direnv/python-3.12/bin/python"),
            Durability::NotDurable
        );
    }

    #[test]
    fn venv_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/project/.venv/bin/python"),
            Durability::NotDurable
        );
    }

    #[test]
    fn node_modules_bin_is_not_durable() {
        assert_eq!(
            judge_str("/home/u/project/node_modules/.bin/eslint"),
            Durability::NotDurable
        );
    }

    // --- Durable: environment-wide reference surfaces ---

    #[test]
    fn homebrew_bin_is_durable() {
        assert_eq!(judge_str("/opt/homebrew/bin/node"), Durability::Durable);
    }

    #[test]
    fn homebrew_sbin_is_durable() {
        assert_eq!(judge_str("/opt/homebrew/sbin/foo"), Durability::Durable);
    }

    #[test]
    fn usr_bin_is_durable() {
        assert_eq!(judge_str("/usr/bin/git"), Durability::Durable);
    }

    #[test]
    fn usr_sbin_is_durable() {
        assert_eq!(judge_str("/usr/sbin/foo"), Durability::Durable);
    }

    #[test]
    fn sbin_is_durable() {
        assert_eq!(judge_str("/sbin/init"), Durability::Durable);
    }

    #[test]
    fn bin_is_durable() {
        assert_eq!(judge_str("/bin/sh"), Durability::Durable);
    }

    #[test]
    fn usr_local_bin_is_durable() {
        assert_eq!(judge_str("/usr/local/bin/node"), Durability::Durable);
    }

    #[test]
    fn mise_shim_is_durable() {
        assert_eq!(
            judge_str("/home/u/.local/share/mise/shims/node"),
            Durability::Durable
        );
    }

    #[test]
    fn run_current_system_is_durable() {
        assert_eq!(
            judge_str("/run/current-system/sw/bin/git"),
            Durability::Durable
        );
    }

    #[test]
    fn etc_profiles_per_user_is_durable() {
        assert_eq!(
            judge_str("/etc/profiles/per-user/alice/bin/curl"),
            Durability::Durable
        );
    }

    #[test]
    fn nix_default_profile_is_durable() {
        assert_eq!(
            judge_str("/nix/var/nix/profiles/default/bin/nix"),
            Durability::Durable
        );
    }

    // --- A: strict per-user profile structure (false-positive regression) ---

    #[test]
    fn etc_profiles_per_user_deep_is_not_durable() {
        // Deeper than `<user>/bin/<file>`: must NOT be Durable.
        assert_eq!(
            judge_with_home(
                Path::new("/etc/profiles/per-user/alice/lib/python/bin/foo"),
                Some(Path::new(HOME))
            ),
            Durability::Unknown
        );
    }

    #[test]
    fn etc_profiles_per_user_relative_is_not_durable() {
        // Non-absolute lookalike must NOT match.
        assert_eq!(
            judge_with_home(
                Path::new("home/u/etc/profiles/per-user/alice/bin/foo"),
                Some(Path::new(HOME))
            ),
            Durability::Unknown
        );
    }

    // --- A/E: direct-dir exact match (substring false-positive regression) ---

    #[test]
    fn usr_local_binutils_is_unknown() {
        // `/usr/local/binutils/foo` must NOT match `/usr/local/bin`.
        assert_eq!(judge_str("/usr/local/binutils/foo"), Durability::Unknown);
    }

    #[test]
    fn opt_homebrew_binfoo_is_unknown() {
        // `/opt/homebrew/binfoo/git` must NOT match `/opt/homebrew/bin`.
        assert_eq!(judge_str("/opt/homebrew/binfoo/git"), Durability::Unknown);
    }

    #[test]
    fn usr_bin_nested_is_unknown() {
        // A file nested below /usr/bin (not direct child) must NOT match.
        assert_eq!(judge_str("/usr/bin/subdir/foo"), Durability::Unknown);
    }

    // --- B: project/user-local shim must not be Durable ---

    #[test]
    fn project_local_mise_shim_is_unknown() {
        // `/repo/.mise/shims/tool` is shim-shaped but NOT HOME-anchored.
        assert_eq!(
            judge_with_home(Path::new("/repo/.mise/shims/tool"), Some(Path::new(HOME))),
            Durability::Unknown
        );
    }

    #[test]
    fn shim_without_home_is_unknown() {
        // HOME unknown → cannot confirm standard shim → Unknown.
        assert_eq!(
            judge_with_home(Path::new("/home/u/.local/share/mise/shims/node"), None),
            Durability::Unknown
        );
    }

    #[test]
    fn shim_anchored_under_home_is_durable() {
        assert_eq!(
            judge_with_home(Path::new("/home/u/.asdf/shims/ruby"), Some(Path::new(HOME))),
            Durability::Durable
        );
    }

    // --- Unknown: user dropbox and unrecognized locations ---

    #[test]
    fn local_bin_jupyter_is_unknown() {
        // ~/.local/bin mixes shebang/versioned, deliberately excluded.
        assert_eq!(judge_str("/home/u/.local/bin/jupyter"), Durability::Unknown);
    }

    #[test]
    fn home_bin_is_unknown() {
        assert_eq!(judge_str("/home/u/bin/mytool"), Durability::Unknown);
    }

    #[test]
    fn cargo_bin_is_unknown() {
        // ~/.cargo/bin is durable in practice but not in the allow-list yet;
        // safe side = Unknown (0.5.x may promote).
        assert_eq!(judge_str("/home/u/.cargo/bin/ripgrep"), Durability::Unknown);
    }

    #[test]
    fn unrecognized_dir_is_unknown() {
        assert_eq!(judge_str("/some/random/dir/tool"), Durability::Unknown);
    }

    // --- ordering: NotDurable wins over a durable-looking surface ---

    #[test]
    fn ephemeral_shim_is_not_durable_despite_shim() {
        // jj-worktree cache: ephemeral + shim. NotDurable must win (decided first).
        assert_eq!(
            judge_str("/home/u/.cache/jj-worktree/.mise/shims/git"),
            Durability::NotDurable
        );
    }

    // --- O: Windows backslash normalization ---

    #[test]
    fn windows_chocolatey_lib_is_not_durable() {
        assert_eq!(
            judge_str(r"C:\ProgramData\chocolatey\lib\foo\bin\foo.exe"),
            Durability::NotDurable
        );
    }

    #[test]
    fn windows_winget_packages_is_not_durable() {
        assert_eq!(
            judge_str(r"C:\Users\u\AppData\Local\Microsoft\WinGet\Packages\Foo\bin\foo.exe"),
            Durability::NotDurable
        );
    }

    #[test]
    fn windows_scoop_apps_is_not_durable() {
        assert_eq!(
            judge_str(r"C:\Users\u\scoop\apps\foo\1.2.3\foo.exe"),
            Durability::NotDurable
        );
    }

    // --- helper-level direct tests ---

    #[test]
    fn is_durable_direct_dir_exact_only() {
        assert!(is_durable_direct_dir(&PathBuf::from("/usr/bin/git")));
        assert!(!is_durable_direct_dir(&PathBuf::from("/usr/bin/sub/git")));
        assert!(!is_durable_direct_dir(&PathBuf::from("usr/bin/git")));
    }
}
