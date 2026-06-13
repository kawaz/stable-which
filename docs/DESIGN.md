# Design

> English | [日本語](./DESIGN-ja.md)

stable-which is a tool/library that evaluates binary path stability and enumerates all candidates on PATH with tags.

## Workspace

| Crate | Role | Dependencies | Publish |
|---|---|---|---|
| `stable-which` | Library | none (std only) | crates.io |
| `stable-which-cli` | CLI binary | stable-which, serde, serde_json | No (Homebrew distribution) |

## Data Model

### PathTag

An enum representing attributes of a candidate path. Three-color classification:

**Positive (green):** Input, InPathEnv(order), SymlinkTo(target), SameCanonical, SameContent
**Warning (orange):** ManagedBy(name), Shim, BuildOutput, Ephemeral, Relative, NonNormalized
**Negative (red):** DifferentBinary

`InPathEnv(usize)` holds the discovery order in PATH (0 = first match). Used for tie-breaking at equal scores.

### Candidate

Fields are private (DR-015 Decision 1). Access via accessors:

```
path() -> &Path           -- entry path (subject of tag/durability evaluation, not necessarily canonical)
canonical() -> &Path       -- realpath on successful canonicalize, fallback to input path on failure
tags() -> &[PathTag]       -- assigned tags (order not guaranteed)
durability() -> Durability -- pin durability (see below)
is_stable() -> bool        -- convenience for matches!(durability(), Durable)
```

Internal `path_order()` (`pub(crate)`) retrieves the `InPathEnv` discovery order. Input candidates use `usize::MAX` (lowest priority in tie-breaking). `Candidate::for_test(..)` (`#[cfg(test)]`) is available for tests.

### Durability (DR-016)

An orthogonal separate-axis enum from `PathTag`. Represents "can this path be baked into a launchd plist / systemd unit and survive rebuild / upgrade / reboot (durable-to-pin)?". `#[non_exhaustive]`.

| variant | meaning |
|---|---|
| Durable | Environment-wide reference surface (system bin / profile bin / standard shim etc.). Safe to pin |
| NotDurable | versioned-install / ephemeral / build-output / project-local. Pinning may break |
| Unknown | No known pattern match (user dropboxes `~/bin`, `~/.local/bin` etc.). Treated as not-durable (safe side) |

Judgment is allow-list based, per candidate (DR-016 Decision 3/4):

1. NotDurable first: versioned-install | ephemeral | build-output | project-local
2. durable location (strict match): exact directory match (`/usr/bin`, `/usr/local/bin`, `/opt/homebrew/bin` etc.) / structural match of `/etc/profiles/per-user/<user>/bin/<file>` / HOME-anchored standard shim (`~/.local/share/mise/shims/` etc.)
3. Otherwise → Unknown

Uses structural matching rather than partial match (`contains`) to avoid false positives like `/usr/local/binutils` or project-local `/repo/.mise/shims/`. Scope (project-local) is internal only, not exposed.

### ScoringPolicy

| Policy | Weight | Use case |
|---|---|---|
| SameBinary (default) | binary × 1000 + preference_tier × 10 + bonus + penalty | Service registration (binary identity priority) |
| Stable | preference_tier × 1000 + binary × 10 + bonus + penalty | Config files (path stability priority) |

Score components:
- binary_score: SameCanonical=3, SameContent=2, DifferentBinary=0
- preference_tier: clean=3, ManagedBy/Shim=1, BuildOutput/Ephemeral=0 (tier 2 reserved for future use)
- in_path_bonus: InPathEnv=+5
- penalty: Relative=-3, NonNormalized=-2 (cumulative)

`preference_tier` is a separate concept from durability (DR-016) (preference tier within a score band). `score()` is `pub(crate)` (not exposed). Ranking is represented as the sort result of `rank_candidates`.

Sort: descending score → ties broken by `path_order()` ascending (PATH front takes priority).

### Error

`#[non_exhaustive]` enum. Variants: NotFound, NotAFile, NoFileName, NotInPath, Canonicalize, Metadata. `impl Display` + `impl std::error::Error`.

## API (3-layer separation of find / rank / resolve, DR-015 Decision 2)

```rust
// Discovery only (no policy, deterministic PATH order)
find_candidates(binary) -> Result<Vec<Candidate>, Error>
find_candidates_with_path_env(binary, path_env) -> Result<Vec<Candidate>, Error>
// Ranking (in-place sort)
rank_candidates(candidates: &mut [Candidate], policy)
// Resolution (composition of find + rank, returns best candidate)
resolve_stable_path(binary, policy) -> Result<Candidate, Error>
```

`find_candidates*` always returns non-empty on success (always includes at least the input candidate). Zero candidates means `Err` only.

Public types: `Candidate`, `PathTag`, `ScoringPolicy`, `Durability`, `Error` re-exported from crate root (`stable_which::Candidate` etc.). The `candidate` / `durability` / `path_analysis` modules, and internal helpers such as `detect_version_manager` / `VersionManagerInfo` / `is_executable` / `files_have_same_content` are private (DR-015 Decision 5/6).

## CLI

```
stable-which [OPTIONS] <binary>

--all            Show all candidates
--format <F>     path (default) | json
--policy <P>     same-binary (default) | stable
--inspect        Shorthand for --all --format json
-q, --quiet      Suppress warnings
--help           Show help (stdout)
--version        Show version
```

Running without arguments exits with 1. Explicit `--help` exits with 0.

## Detection Patterns

### Version managers (ManagedBy)

mise, asdf, nix, homebrew, nvm, fnm, rustup, volta, sdkman, pyenv, rbenv, goenv, aqua, proto

### Shims

- Directory patterns: `/mise/shims/`, `/asdf/shims/`, `/pyenv/shims/` etc.
- Heuristic: when the symlink target name is not a prefix of the candidate name (`git` → `jj-worktree`)

### Build outputs (BuildOutput)

`target/debug/`, `target/release/`, `.build/debug/`, `dist-newstyle/`, `DerivedData/`, `zig-out/` etc.

### Ephemeral paths

Matches `\b(cache|tmp|temp|temporary)\b` case-insensitively against `path.parent()`. Excludes paths inside `.app` bundles.

### Executable bit check

PATH candidates are checked for Unix executable bit (`mode & 0o111`). The input binary itself uses `is_file()` only (since it is explicitly specified, it is analyzed even without executable bit).

## File identity check

Byte-level streaming comparison (zero dependencies). Rejects on size mismatch (O(1)); byte comparison (64KB buffer) only when sizes match. No cryptographic hashing.

## Design Principles

- Durability (pin durability) is judged by allow-list (NotDurable first → durable allow-list → Unknown). Unknown is the safe side (DR-016)
- preference_tier (preference within a score band) is calculated from the absence of unstable patterns. A separate axis from durability
- Tags and durability are evaluated against `candidate.path()` (not canonical). Durability is per candidate
- Tags are objective attributes; scores are subjective weights (separated). Score absolute values are private; ranking is expressed as the sort result of `rank_candidates`
- Library has zero dependencies (Serialize etc. are on the CLI side)
- Candidates with equal scores are deterministically tie-broken by PATH discovery order

## Related Documents

- [Design Records](decisions/) — Individual design decisions and their rationale
- [Path Patterns Research](research/2026-04-09-path-patterns.md) — Comprehensive survey of unstable binary path patterns
- [Service Registration Constraints](findings/2026-04-09-service-registration.md) — Binary path constraints in launchd / systemd
