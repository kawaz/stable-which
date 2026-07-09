# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] — 2026-07-09

### Changed

- **`~/.cargo/bin`, `~/go/bin`, `~/.moon/bin` promoted from `Unknown` to
  `Durable`**: all three are direct-install surfaces whose owning installer
  (`cargo install` / `go install` / `moon upgrade`) overwrites the binary in
  place at the same path on every install/upgrade (no versioned tree), and
  rustup's toolchain-proxy binaries (`rustc`, `cargo`, ...) also live in
  `~/.cargo/bin` without moving on toolchain switches — they now qualify as
  durable reference surfaces (DR-016 Decision 5 / its 2026-07-09 addendum;
  `Unknown → Durable` is a non-breaking refinement). The match is strict and
  HOME-anchored (`<home><dir>/<file>`, a single direct file segment); a
  project-local `/repo/.cargo/bin/tool` or an unresolved `HOME` still falls
  through to `Unknown`.

## [0.4.2] — 2026-06-13

### Fixed

- **Windows test gating**: guard the 14 tests that expect `Durability::Durable`
  for Unix absolute paths (`/usr/bin`, `/opt/homebrew/bin`, etc.) behind
  `#[cfg(unix)]`. `Path::is_absolute()` returns `false` for these paths on
  Windows (no drive letter), so `is_durable_direct_dir` /
  `is_etc_profiles_per_user_bin` always return `false` there and the
  candidates fall through to `Unknown` instead of `Durable`; the tests would
  otherwise fail on Windows CI.

### Documentation

- Document the Windows behavior of the durable-location allow-list on
  `Durability` and `judge`: on Windows all such paths fall through to
  `Unknown` (the safe side) until native Windows durable-location support
  (`C:\Windows\System32`, scoop/chocolatey/winget bins, etc.) lands in 0.5.x.
  Tracked in `docs/issue/2026-06-13-windows-durable-location.md`.

## [0.4.1] — 2026-06-13

### Fixed

- **MSRV**: replace a let-chain (`&& let`) with a nested `if let` so the crate
  truly builds on the declared `rust-version = "1.85"`. 0.4.0 used a let-chain
  that requires Rust 1.88, so `cargo build` failed on Rust 1.85–1.87; 0.4.0 is
  yanked for this reason.

### Changed

- Drop the unused `version` field from the CLI crate's path dependency on
  `stable-which` (`publish = false`); the release task no longer perl-syncs it.

## [0.4.0] — 2026-06-13

This is a **breaking** release that stabilizes the public API toward 1.0. See
`docs/decisions/DR-015-public-api-stabilization.md` and
`docs/decisions/DR-016-durability-model.md` for the rationale.

### Added

- **`Durability` enum** (`Durable` / `NotDurable` / `Unknown`, `#[non_exhaustive]`)
  and `Candidate::durability() -> Durability`. Judges whether a path is
  *durable-to-pin* (can be baked into a launchd plist / systemd unit and survive
  rebuild / upgrade / reboot). Judged per candidate, so the reference path
  (`/opt/homebrew/bin/git`) can be `Durable` while its versioned realpath
  (`/opt/homebrew/Cellar/git/2.44.0/bin/git`) is `NotDurable`.
- **`Candidate::is_stable() -> bool`** convenience: `true` iff
  `durability() == Durability::Durable` (`Unknown` is treated as not stable).
- **`Candidate` accessors**: `path()`, `canonical()`, `tags()` (fields are now
  private; see Changed).
- **`rank_candidates(&mut [Candidate], policy)`**: sorts a candidate slice in
  place (score descending, PATH discovery order as tie-break).
- CLI JSON output (`--inspect` / `--all --format json`) now includes a
  `durability` field per candidate.

### Changed

- **`Candidate` fields are private.** `path` / `canonical` / `tags` are no longer
  `pub`; use the accessors `path()` / `canonical()` / `tags()` plus the new
  `durability()` / `is_stable()`. This hides the internal `Vec<PathTag>`
  representation.
- **`find_candidates` / `find_candidates_with_*` no longer take a `policy`
  argument.** Discovery and ranking are separated:
  - `find_candidates(binary)` — discovery only, deterministic PATH order.
  - `rank_candidates(&mut candidates, policy)` — ranking step.
  - `resolve_stable_path(binary, policy)` keeps its signature (composes
    find + rank internally).
- **`find_candidates_with_env` renamed to `find_candidates_with_path_env`.** The
  argument is the PATH-equivalent list, not a full environment (Windows PATHEXT
  is still read from the process environment). `path_env = None` does not skip
  PATH search; a bare command name with `None` fails with `Error::NotInPath`.
- **`is_stable()` semantics changed to durable-to-pin.** It was previously
  "no `BuildOutput` / `Ephemeral` tag"; it now means `durability() == Durable`.
  Versioned installs (e.g. `Cellar/<pkg>/<ver>`), which were treated as stable
  before, are now `NotDurable`.
- **CLI JSON `score` field replaced by `rank`.** The field is now the 0-based
  rank index after sorting (best = 0), not the raw policy-dependent score. The
  absolute score value is no longer part of the public API. *(Breaking change to
  the `--format json` output contract.)*

### Removed

- **`Candidate::score()` is no longer public** (now `pub(crate)`). Ranking is
  expressed solely via `rank_candidates`. The internal `stability_score` term
  was renamed `preference_tier` to avoid confusion with the durability axis.
- **`path_analysis` module and `VersionManagerInfo` are no longer public.** The
  module is private and the `pub use path_analysis::VersionManagerInfo`
  re-export was removed. Version-manager names remain available via
  `PathTag::ManagedBy(String)`.
- **Module paths are no longer public.** Types are reachable only from the crate
  root (`stable_which::Candidate`, etc.), not `stable_which::candidate::*`.

### Migration (downstream)

- **cache-warden**: a self-rolled `is_unstable_resolution(tags)` check can be
  replaced with `!candidate.is_stable()` (which now correctly treats versioned
  installs as not durable). Read paths via `candidate.path()` /
  `candidate.canonical()` instead of the former public fields.
- Callers of `find_candidates(binary, policy)` should drop the `policy` argument
  and call `rank_candidates(&mut candidates, policy)` afterward (or use
  `resolve_stable_path` for the single best candidate).
- Callers of `find_candidates_with_env(...)` should rename to
  `find_candidates_with_path_env(...)` and drop the `policy` argument.
- Replace `stable_which::candidate::X` imports with `stable_which::X`.
- Consumers of the CLI JSON `score` field should switch to `rank` (0-based
  position) and/or the new `durability` field.

## [0.3.3] and earlier

See the git history.
