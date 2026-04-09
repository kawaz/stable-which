# DR-014: Windows Support

## Decision

Support Windows as a target platform. Platform-specific logic uses `cfg(unix)` / `cfg(windows)` conditional compilation.

## Changes

- **Executable detection**: Unix uses permission bits (`mode & 0o111`), Windows uses PATHEXT extension matching
- **PATH search**: On Windows, PATHEXT extensions are appended when searching for command names
- **Path separators**: Pattern matching normalizes `\` to `/` before comparison via `normalize_separators()`
- **Path deduplication**: On Windows, path comparison is case insensitive via `normalize_for_dedup()`
- **Package managers**: Added Scoop, Chocolatey, and winget patterns for install and shim detection
- **Relative path prefix**: Both `./` and `.\` (and `../` / `..\`) are recognized as explicit relative prefixes
- **Tests**: Unix-specific tests (symlink creation, permission bits) are gated with `#[cfg(unix)]`; score-only tests remain cross-platform

## Rejected Alternatives

- **`compile_error!` to block Windows**: Inappropriate for a crates.io library; users on Windows would be unable to compile dependents
- **Ignore Windows entirely**: Library consumers on Windows would get no functionality
- **Runtime feature flag**: Adds unnecessary complexity; `cfg` is the idiomatic Rust approach for platform-specific code
