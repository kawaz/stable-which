use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionManagerInfo {
    pub name: String,
}

const INSTALL_PATTERNS: &[(&str, &str)] = &[
    ("/mise/installs/", "mise"),
    ("/.mise/installs/", "mise"),
    ("/asdf/installs/", "asdf"),
    ("/.asdf/installs/", "asdf"),
    ("/nix/store/", "nix"),
    ("/.nix-profile/", "nix"),
    ("/Cellar/", "homebrew"),
    ("/Caskroom/", "homebrew"),
    ("/.nvm/versions/", "nvm"),
    ("/.fnm/node-versions/", "fnm"),
    ("/.rustup/toolchains/", "rustup"),
    ("/.volta/tools/", "volta"),
    ("/.sdkman/candidates/", "sdkman"),
    ("/.pyenv/versions/", "pyenv"),
    ("/.rbenv/versions/", "rbenv"),
    ("/.goenv/versions/", "goenv"),
    ("/aquaproj-aqua/internal/pkgs/", "aqua"),
    ("/.proto/tools/", "proto"),
];

const SHIM_PATTERNS: &[&str] = &[
    "/mise/shims/",
    "/.mise/shims/",
    "/asdf/shims/",
    "/.asdf/shims/",
    "/.pyenv/shims/",
    "/.rbenv/shims/",
    "/.goenv/shims/",
    "/.proto/shims/",
];

const BUILD_OUTPUT_PATTERNS: &[&str] = &[
    "/target/debug/",
    "/target/release/",
    "/.build/debug/",
    "/.build/release/",
    "/dist-newstyle/",
    "/.stack-work/",
    "/_build/",
    "/zig-out/",
    "/zig-cache/",
    "/cmake-build-",
    "/bin/Debug/",
    "/bin/Release/",
    "/.dub/build/",
    "/nimcache/",
    "/DerivedData/",
    "/Build/Products/",
    "/target/wasm-gc/",
    "/target/wasm/",
    "/target/js/",
];

/// Detect whether a path is under a version manager's install directory.
pub fn detect_version_manager(path: &Path) -> Option<VersionManagerInfo> {
    let s = path.to_string_lossy();
    for &(pattern, name) in INSTALL_PATTERNS {
        if s.contains(pattern) {
            return Some(VersionManagerInfo {
                name: name.to_string(),
            });
        }
    }
    None
}

/// Detect whether a path is inside a shim directory.
pub fn is_shim_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    SHIM_PATTERNS.iter().any(|pattern| s.contains(pattern))
}

/// Heuristic: if the symlink target name does not start with the candidate name,
/// it is likely a shim dispatcher (e.g. `git` -> `jj-worktree`).
/// Version suffixes like `python3` -> `python3.12` are not shims.
pub fn is_shim_by_name(candidate_name: &str, symlink_target_name: &str) -> bool {
    !symlink_target_name.starts_with(candidate_name)
}

/// Detect whether a path is inside a build output directory.
pub fn is_build_output(path: &Path) -> bool {
    let s = path.to_string_lossy();
    BUILD_OUTPUT_PATTERNS
        .iter()
        .any(|pattern| s.contains(pattern))
}

/// Detect whether the parent directory of a path is a temporary/cache location.
///
/// Checks path components (split by `-`, `_`, `.`) case-insensitively for
/// `cache`, `tmp`, `temp`, or `temporary`. Excludes the portion of the path
/// before `.app/` to avoid false positives from macOS app bundle names.
pub fn is_ephemeral(path: &Path) -> bool {
    let parent = match path.parent() {
        Some(p) => p.to_string_lossy(),
        None => return false,
    };

    // .app bundle exclusion: only evaluate the path after .app/
    let check_target = if let Some(pos) = parent.find(".app/") {
        &parent[pos + 5..]
    } else {
        &parent
    };

    for component in check_target.split('/') {
        let lower = component.to_ascii_lowercase();
        for word in lower.split(['-', '_', '.']) {
            if matches!(word, "cache" | "tmp" | "temp" | "temporary") {
                return true;
            }
        }
    }
    false
}

/// Check if a path is an executable file (Unix: has execute permission)
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(path) {
        Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}

/// Compare two files for byte-identical content.
/// Returns true only if both files exist, have the same size, and identical content.
pub fn files_have_same_content(path_a: &Path, path_b: &Path) -> bool {
    // 1. Compare file sizes first (fast rejection)
    let meta_a = match fs::metadata(path_a) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let meta_b = match fs::metadata(path_b) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if meta_a.len() != meta_b.len() {
        return false;
    }

    // 2. Byte-by-byte streaming comparison
    let mut file_a = match fs::File::open(path_a) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut file_b = match fs::File::open(path_b) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut buf_a = [0u8; 8192];
    let mut buf_b = [0u8; 8192];

    loop {
        let n_a = match file_a.read(&mut buf_a) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let n_b = match file_b.read(&mut buf_b) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if n_a != n_b || buf_a[..n_a] != buf_b[..n_b] {
            return false;
        }
        if n_a == 0 {
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // --- detect_version_manager ---

    #[test]
    fn test_detect_mise() {
        let path = Path::new("/home/user/.local/share/mise/installs/node/20/bin/node");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "mise");
    }

    #[test]
    fn test_detect_dot_mise() {
        let path = Path::new("/home/user/.mise/installs/python/3.12/bin/python3");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "mise");
    }

    #[test]
    fn test_detect_asdf() {
        let path = Path::new("/home/user/.asdf/installs/ruby/3.2.0/bin/ruby");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "asdf");
    }

    #[test]
    fn test_detect_nix_store() {
        let path = Path::new("/nix/store/abc123-hello/bin/hello");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "nix");
    }

    #[test]
    fn test_detect_nix_profile() {
        let path = Path::new("/home/user/.nix-profile/bin/git");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "nix");
    }

    #[test]
    fn test_detect_homebrew_cellar() {
        let path = Path::new("/opt/homebrew/Cellar/git/2.44.0/bin/git");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "homebrew");
    }

    #[test]
    fn test_detect_homebrew_caskroom() {
        let path = Path::new("/opt/homebrew/Caskroom/firefox/125.0/Firefox.app");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "homebrew");
    }

    #[test]
    fn test_detect_nvm() {
        let path = Path::new("/home/user/.nvm/versions/node/v20.0.0/bin/node");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "nvm");
    }

    #[test]
    fn test_detect_fnm() {
        let path = Path::new("/home/user/.fnm/node-versions/v20.0.0/installation/bin/node");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "fnm");
    }

    #[test]
    fn test_detect_rustup() {
        let path = Path::new("/home/user/.rustup/toolchains/stable-x86_64/bin/rustc");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "rustup");
    }

    #[test]
    fn test_detect_volta() {
        let path = Path::new("/home/user/.volta/tools/image/node/20.0.0/bin/node");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "volta");
    }

    #[test]
    fn test_detect_sdkman() {
        let path = Path::new("/home/user/.sdkman/candidates/java/17.0.1/bin/java");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "sdkman");
    }

    #[test]
    fn test_detect_pyenv() {
        let path = Path::new("/home/user/.pyenv/versions/3.12.0/bin/python3");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "pyenv");
    }

    #[test]
    fn test_detect_rbenv() {
        let path = Path::new("/home/user/.rbenv/versions/3.2.0/bin/ruby");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "rbenv");
    }

    #[test]
    fn test_detect_goenv() {
        let path = Path::new("/home/user/.goenv/versions/1.21.0/bin/go");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "goenv");
    }

    #[test]
    fn test_detect_aqua() {
        let path = Path::new("/home/user/.local/share/aquaproj-aqua/internal/pkgs/foo/bin/foo");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "aqua");
    }

    #[test]
    fn test_detect_proto() {
        let path = Path::new("/home/user/.proto/tools/node/20.0.0/bin/node");
        let result = detect_version_manager(path).unwrap();
        assert_eq!(result.name, "proto");
    }

    #[test]
    fn test_detect_none_for_system_path() {
        let path = Path::new("/usr/bin/git");
        assert!(detect_version_manager(path).is_none());
    }

    #[test]
    fn test_detect_none_for_local_bin() {
        let path = Path::new("/usr/local/bin/node");
        assert!(detect_version_manager(path).is_none());
    }

    // --- is_shim_path ---

    #[test]
    fn test_shim_mise() {
        assert!(is_shim_path(Path::new(
            "/home/user/.local/share/mise/shims/node"
        )));
    }

    #[test]
    fn test_shim_dot_mise() {
        assert!(is_shim_path(Path::new("/home/user/.mise/shims/python3")));
    }

    #[test]
    fn test_shim_asdf() {
        assert!(is_shim_path(Path::new("/home/user/.asdf/shims/ruby")));
    }

    #[test]
    fn test_shim_pyenv() {
        assert!(is_shim_path(Path::new("/home/user/.pyenv/shims/python3")));
    }

    #[test]
    fn test_shim_rbenv() {
        assert!(is_shim_path(Path::new("/home/user/.rbenv/shims/ruby")));
    }

    #[test]
    fn test_shim_goenv() {
        assert!(is_shim_path(Path::new("/home/user/.goenv/shims/go")));
    }

    #[test]
    fn test_shim_proto() {
        assert!(is_shim_path(Path::new("/home/user/.proto/shims/node")));
    }

    #[test]
    fn test_not_shim_system_bin() {
        assert!(!is_shim_path(Path::new("/usr/bin/git")));
    }

    #[test]
    fn test_not_shim_installs() {
        assert!(!is_shim_path(Path::new(
            "/home/user/.mise/installs/node/20/bin/node"
        )));
    }

    // --- is_shim_by_name ---

    #[test]
    fn test_shim_by_name_different_binary() {
        // git -> jj-worktree is a shim (names completely different)
        assert!(is_shim_by_name("git", "jj-worktree"));
    }

    #[test]
    fn test_shim_by_name_version_suffix_not_shim() {
        // python3 -> python3.12 is NOT a shim (version suffix)
        assert!(!is_shim_by_name("python3", "python3.12"));
    }

    #[test]
    fn test_shim_by_name_version_dash_suffix_not_shim() {
        // foo -> foo-0.2.1 is NOT a shim (version suffix with dash)
        assert!(!is_shim_by_name("foo", "foo-0.2.1"));
    }

    #[test]
    fn test_shim_by_name_exact_match_not_shim() {
        // same name is not a shim
        assert!(!is_shim_by_name("node", "node"));
    }

    // --- is_build_output ---

    #[test]
    fn test_build_output_target_debug() {
        assert!(is_build_output(Path::new(
            "/home/user/project/target/debug/myapp"
        )));
    }

    #[test]
    fn test_build_output_target_release() {
        assert!(is_build_output(Path::new(
            "/home/user/project/target/release/myapp"
        )));
    }

    #[test]
    fn test_build_output_swift_build() {
        assert!(is_build_output(Path::new(
            "/home/user/project/.build/debug/myapp"
        )));
    }

    #[test]
    fn test_build_output_haskell_dist() {
        assert!(is_build_output(Path::new(
            "/home/user/project/dist-newstyle/build/myapp"
        )));
    }

    #[test]
    fn test_build_output_stack() {
        assert!(is_build_output(Path::new(
            "/home/user/project/.stack-work/install/bin/myapp"
        )));
    }

    #[test]
    fn test_build_output_elixir_build() {
        assert!(is_build_output(Path::new(
            "/home/user/project/_build/dev/lib/myapp"
        )));
    }

    #[test]
    fn test_build_output_zig_out() {
        assert!(is_build_output(Path::new(
            "/home/user/project/zig-out/bin/myapp"
        )));
    }

    #[test]
    fn test_build_output_zig_cache() {
        assert!(is_build_output(Path::new(
            "/home/user/project/zig-cache/o/myobj"
        )));
    }

    #[test]
    fn test_build_output_cmake() {
        assert!(is_build_output(Path::new(
            "/home/user/project/cmake-build-debug/bin/myapp"
        )));
    }

    #[test]
    fn test_build_output_dotnet_debug() {
        assert!(is_build_output(Path::new(
            "/home/user/project/bin/Debug/net8.0/myapp"
        )));
    }

    #[test]
    fn test_build_output_dotnet_release() {
        assert!(is_build_output(Path::new(
            "/home/user/project/bin/Release/net8.0/myapp"
        )));
    }

    #[test]
    fn test_build_output_dub() {
        assert!(is_build_output(Path::new(
            "/home/user/project/.dub/build/myapp"
        )));
    }

    #[test]
    fn test_build_output_nim() {
        assert!(is_build_output(Path::new(
            "/home/user/project/nimcache/myapp"
        )));
    }

    #[test]
    fn test_build_output_xcode_derived_data() {
        assert!(is_build_output(Path::new(
            "/Users/user/Library/Developer/Xcode/DerivedData/MyApp-abc/Build/Products/Debug/myapp"
        )));
    }

    #[test]
    fn test_build_output_xcode_build_products() {
        assert!(is_build_output(Path::new(
            "/Users/user/project/Build/Products/Release/myapp"
        )));
    }

    #[test]
    fn test_build_output_moonbit_wasm_gc() {
        assert!(is_build_output(Path::new(
            "/home/user/project/target/wasm-gc/release/build/main/main.wasm"
        )));
    }

    #[test]
    fn test_build_output_moonbit_wasm() {
        assert!(is_build_output(Path::new(
            "/home/user/project/target/wasm/release/build/main/main.wasm"
        )));
    }

    #[test]
    fn test_build_output_moonbit_js() {
        assert!(is_build_output(Path::new(
            "/home/user/project/target/js/release/build/main/main.js"
        )));
    }

    #[test]
    fn test_not_build_output_system_bin() {
        assert!(!is_build_output(Path::new("/usr/bin/git")));
    }

    #[test]
    fn test_not_build_output_local_bin() {
        assert!(!is_build_output(Path::new("/usr/local/bin/node")));
    }

    // --- is_ephemeral ---

    #[test]
    fn test_ephemeral_tmp() {
        assert!(is_ephemeral(Path::new("/tmp/foo/bar")));
    }

    #[test]
    fn test_ephemeral_cache_dir() {
        assert!(is_ephemeral(Path::new("/home/user/.cache/x/bin/y")));
    }

    #[test]
    fn test_ephemeral_temp_upper() {
        // case insensitive
        assert!(is_ephemeral(Path::new("/Users/x/TEMP/y")));
    }

    #[test]
    fn test_not_ephemeral_cache_warden() {
        // "cache-warden" as a binary name should not trigger (we check parent only)
        // But even if parent contained "cache", word boundary prevents "cache-warden" dir from matching
        // Actually /usr/bin is the parent here, so no match
        assert!(!is_ephemeral(Path::new("/usr/bin/cache-warden")));
    }

    #[test]
    fn test_not_ephemeral_app_bundle_cache_name() {
        // "Cache Warden.app" in path before .app/ should be excluded
        assert!(!is_ephemeral(Path::new(
            "/Applications/Cache Warden.app/Contents/MacOS/bin/cache-warden"
        )));
    }

    #[test]
    fn test_ephemeral_temporary_dir() {
        assert!(is_ephemeral(Path::new("/var/temporary/data/bin/foo")));
    }

    #[test]
    fn test_not_ephemeral_normal_path() {
        assert!(!is_ephemeral(Path::new("/usr/local/bin/node")));
    }

    #[test]
    fn test_ephemeral_compound_component() {
        // "my-cache-dir" contains "cache" as a word separated by hyphens
        assert!(is_ephemeral(Path::new("/opt/my-cache-dir/bin/foo")));
    }

    #[test]
    fn test_not_ephemeral_no_parent() {
        assert!(!is_ephemeral(Path::new("binary")));
    }

    // --- files_have_same_content ---

    #[test]
    fn test_same_content() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a");
        let f2 = dir.path().join("b");
        std::fs::write(&f1, "hello world").unwrap();
        std::fs::write(&f2, "hello world").unwrap();
        assert!(files_have_same_content(&f1, &f2));
    }

    #[test]
    fn test_different_content() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a");
        let f2 = dir.path().join("b");
        std::fs::write(&f1, "hello").unwrap();
        std::fs::write(&f2, "world").unwrap();
        assert!(!files_have_same_content(&f1, &f2));
    }

    #[test]
    fn test_different_size() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a");
        let f2 = dir.path().join("b");
        std::fs::write(&f1, "short").unwrap();
        std::fs::write(&f2, "much longer content").unwrap();
        assert!(!files_have_same_content(&f1, &f2));
    }

    #[test]
    fn test_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a");
        let f2 = dir.path().join("b");
        std::fs::write(&f1, "").unwrap();
        std::fs::write(&f2, "").unwrap();
        assert!(files_have_same_content(&f1, &f2));
    }

    #[test]
    fn test_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a");
        std::fs::write(&f1, "hello").unwrap();
        assert!(!files_have_same_content(
            &f1,
            Path::new("/nonexistent/file")
        ));
        assert!(!files_have_same_content(
            Path::new("/nonexistent/file"),
            &f1
        ));
    }

    // --- is_executable ---

    #[test]
    fn test_is_executable_true() {
        use std::os::unix::fs::PermissionsExt;
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("exec_file");
        fs::write(&path, "content").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&path));
    }

    #[test]
    fn test_is_executable_false_no_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("no_exec");
        fs::write(&path, "content").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_executable(&path));
    }

    #[test]
    fn test_is_executable_false_directory() {
        let tmpdir = tempfile::tempdir().unwrap();
        assert!(!is_executable(tmpdir.path()));
    }

    #[test]
    fn test_is_executable_false_nonexistent() {
        assert!(!is_executable(Path::new("/nonexistent/path")));
    }
}
