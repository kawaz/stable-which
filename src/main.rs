use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedPath {
    /// Stable path: PATH entry if found, otherwise canonical path
    pub path: PathBuf,
    /// Whether a matching PATH entry was found
    pub in_path: bool,
    /// Canonical (real) path of the binary
    pub canonical: PathBuf,
}

/// Resolve the stable PATH entry for a binary.
///
/// 1. Canonicalize the input path
/// 2. Extract the file name
/// 3. Search PATH directories for entries with the same file name
/// 4. For each candidate, check if its canonical path matches the input's canonical path
/// 5. Return the first match (stable PATH entry) or fall back to canonical path
pub fn resolve_stable_path(binary: &Path) -> Result<ResolvedPath, String> {
    resolve_stable_path_with_env(binary, env::var_os("PATH"))
}

/// Testable version that accepts PATH as a parameter.
pub fn resolve_stable_path_with_env(
    binary: &Path,
    path_env: Option<std::ffi::OsString>,
) -> Result<ResolvedPath, String> {
    // If the input has no path separator, treat it as a command name and look it up in PATH first.
    let resolved_binary;
    let binary = if !binary.to_string_lossy().contains('/') {
        let name = binary
            .file_name()
            .ok_or_else(|| format!("'{}' has no file name", binary.display()))?;
        resolved_binary = path_env
            .as_ref()
            .and_then(|pv| {
                env::split_paths(pv)
                    .map(|dir| dir.join(name))
                    .find(|c| c.is_file())
            })
            .ok_or_else(|| format!("'{}' not found in PATH", binary.display()))?;
        resolved_binary.as_path()
    } else {
        binary
    };

    if !binary.exists() {
        return Err(format!("'{}' does not exist", binary.display()));
    }

    let metadata =
        fs::metadata(binary).map_err(|e| format!("cannot stat '{}': {}", binary.display(), e))?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a file", binary.display()));
    }

    let canonical = fs::canonicalize(binary)
        .map_err(|e| format!("cannot canonicalize '{}': {}", binary.display(), e))?;

    let file_name = binary
        .file_name()
        .ok_or_else(|| format!("'{}' has no file name", binary.display()))?;

    if let Some(path_var) = path_env {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join(file_name);
            if !candidate.exists() {
                continue;
            }
            if let Ok(candidate_canonical) = fs::canonicalize(&candidate)
                && candidate_canonical == canonical
            {
                return Ok(ResolvedPath {
                    path: candidate,
                    in_path: true,
                    canonical,
                });
            }
        }
    }

    Ok(ResolvedPath {
        path: canonical.clone(),
        in_path: false,
        canonical,
    })
}

enum OutputMode {
    Json,
    PathOnly,
}

fn print_help() {
    eprintln!(
        "\
{NAME} {VERSION}
Resolve the stable PATH entry for a binary, verified by canonical path identity.

Usage:
    {NAME} [OPTIONS] <binary>

Arguments:
    <binary>         Path to the binary, or a command name to look up in PATH

Options:
    --json           Output as JSON (default)
    --path-only      Output only the resolved path
    --help           Show this help message
    --version        Show version"
    );
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_help();
        return Ok(());
    }

    let mut output_mode = OutputMode::Json;
    let mut binary_path: Option<String> = None;

    for arg in &args {
        match arg.as_str() {
            "--help" => {
                print_help();
                return Ok(());
            }
            "--version" => {
                println!("{NAME} {VERSION}");
                return Ok(());
            }
            "--json" => output_mode = OutputMode::Json,
            "--path-only" => output_mode = OutputMode::PathOnly,
            _ if arg.starts_with('-') => {
                return Err(format!("unknown option: {arg}"));
            }
            _ => {
                if binary_path.is_some() {
                    return Err("too many arguments".to_string());
                }
                binary_path = Some(arg.clone());
            }
        }
    }

    let binary_path = binary_path.ok_or_else(|| {
        print_help();
        String::new()
    })?;

    let resolved = resolve_stable_path(Path::new(&binary_path))?;

    match output_mode {
        OutputMode::Json => {
            let json = serde_json::to_string(&resolved)
                .map_err(|e| format!("JSON serialization error: {e}"))?;
            println!("{json}");
        }
        OutputMode::PathOnly => {
            println!("{}", resolved.path.display());
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        if !e.is_empty() {
            eprintln!("{NAME}: {e}");
        }
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// Create a temporary directory structure for testing:
    ///   tmp/
    ///     real/
    ///       mybinary          (real executable file)
    ///     stable_dir/
    ///       mybinary -> ../real/mybinary  (symlink)
    ///     other_dir/
    ///       mybinary          (different file with same name)
    struct TestFixture {
        _tmpdir: tempfile::TempDir,
        real_binary: PathBuf,
        stable_link: PathBuf,
        stable_dir: PathBuf,
        other_dir: PathBuf,
    }

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

            let stable_link = stable_dir.join("mybinary");
            symlink(&real_binary, &stable_link).unwrap();

            let other_binary = other_dir.join("mybinary");
            fs::write(&other_binary, "other-content").unwrap();

            TestFixture {
                _tmpdir: tmpdir,
                real_binary,
                stable_link,
                stable_dir,
                other_dir,
            }
        }

        fn make_path(&self, dirs: &[&Path]) -> std::ffi::OsString {
            env::join_paths(dirs).unwrap()
        }
    }

    #[test]
    fn test_finds_stable_path_via_symlink() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let result = resolve_stable_path_with_env(&f.real_binary, Some(path_env)).unwrap();

        assert!(result.in_path);
        assert_eq!(result.path, f.stable_link);
        assert_eq!(result.canonical, fs::canonicalize(&f.real_binary).unwrap());
    }

    #[test]
    fn test_falls_back_to_canonical_when_not_in_path() {
        let f = TestFixture::new();
        // PATH contains a dir with no matching binary
        let empty_dir = f._tmpdir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();
        let path_env = f.make_path(&[&empty_dir]);

        let result = resolve_stable_path_with_env(&f.real_binary, Some(path_env)).unwrap();

        assert!(!result.in_path);
        assert_eq!(result.path, fs::canonicalize(&f.real_binary).unwrap());
    }

    #[test]
    fn test_rejects_same_name_different_binary() {
        let f = TestFixture::new();
        // PATH contains other_dir which has mybinary pointing to different content
        let path_env = f.make_path(&[&f.other_dir]);

        let result = resolve_stable_path_with_env(&f.real_binary, Some(path_env)).unwrap();

        // Should NOT match the other binary
        assert!(!result.in_path);
        assert_eq!(result.path, fs::canonicalize(&f.real_binary).unwrap());
    }

    #[test]
    fn test_prefers_first_path_entry() {
        let f = TestFixture::new();
        // Both other_dir and stable_dir in PATH; stable_dir is second
        let path_env = f.make_path(&[&f.other_dir, &f.stable_dir]);

        let result = resolve_stable_path_with_env(&f.real_binary, Some(path_env)).unwrap();

        // Should find stable_dir's entry (second in PATH) since other_dir doesn't match
        assert!(result.in_path);
        assert_eq!(result.path, f.stable_link);
    }

    #[test]
    fn test_stable_link_as_input() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        let result = resolve_stable_path_with_env(&f.stable_link, Some(path_env)).unwrap();

        assert!(result.in_path);
        assert_eq!(result.path, f.stable_link);
        assert_eq!(result.canonical, fs::canonicalize(&f.real_binary).unwrap());
    }

    #[test]
    fn test_nonexistent_path_returns_error() {
        let result = resolve_stable_path_with_env(Path::new("/nonexistent/binary"), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_directory_returns_error() {
        let tmpdir = tempfile::tempdir().unwrap();
        let result = resolve_stable_path_with_env(tmpdir.path(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a file"));
    }

    #[test]
    fn test_no_path_env() {
        let f = TestFixture::new();

        let result = resolve_stable_path_with_env(&f.real_binary, None).unwrap();

        assert!(!result.in_path);
        assert_eq!(result.path, fs::canonicalize(&f.real_binary).unwrap());
    }

    #[test]
    fn test_command_name_lookup() {
        let f = TestFixture::new();
        let path_env = f.make_path(&[&f.stable_dir]);

        // Pass just the command name (no path separator)
        let result = resolve_stable_path_with_env(Path::new("mybinary"), Some(path_env)).unwrap();

        assert!(result.in_path);
        assert_eq!(result.path, f.stable_link);
    }

    #[test]
    fn test_command_name_not_found() {
        let f = TestFixture::new();
        let empty_dir = f._tmpdir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();
        let path_env = f.make_path(&[&empty_dir]);

        let result = resolve_stable_path_with_env(Path::new("mybinary"), Some(path_env));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found in PATH"));
    }

    #[test]
    fn test_json_serialization() {
        let resolved = ResolvedPath {
            path: PathBuf::from("/opt/homebrew/bin/jj"),
            in_path: true,
            canonical: PathBuf::from("/opt/homebrew/Cellar/jj/0.24.0/bin/jj"),
        };
        let json = serde_json::to_string(&resolved).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["path"], "/opt/homebrew/bin/jj");
        assert_eq!(parsed["in_path"], true);
        assert_eq!(parsed["canonical"], "/opt/homebrew/Cellar/jj/0.24.0/bin/jj");
    }
}
