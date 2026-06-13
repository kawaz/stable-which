# stable-which

[![crates.io](https://img.shields.io/crates/v/stable-which.svg)](https://crates.io/crates/stable-which)
[![docs.rs](https://docs.rs/stable-which/badge.svg)](https://docs.rs/stable-which)
[![CI](https://github.com/kawaz/stable-which/actions/workflows/ci.yml/badge.svg)](https://github.com/kawaz/stable-which/actions/workflows/ci.yml)

Evaluate binary path stability and find stable PATH candidates.

## Problem

Package managers and version managers place binaries in versioned or volatile directories:

```
/opt/homebrew/Cellar/jj/0.24.0/bin/jj          # Homebrew Cellar (version-specific)
~/.local/share/mise/installs/node/22.0.0/bin/node  # mise installs
./target/release/myapp                           # Cargo build output
```

After `brew upgrade`, the Cellar path breaks. When mise switches versions, the installs path changes. Build outputs move with every rebuild. Meanwhile, stable symlinks or shims exist on PATH:

```
/opt/homebrew/bin/jj -> ../Cellar/jj/0.24.0/bin/jj
~/.local/share/mise/shims/node
```

`which` finds a command by name but does **not** verify that the result points to the same binary. If multiple versions coexist, `which` may return a completely different binary.

`stable-which` enumerates all same-name candidates on PATH, tags each with stability properties, and scores them to find the most stable path that points to the same file.

## How it works

1. Canonicalize the input binary path
2. Search PATH for all same-name binaries
3. Tag each candidate (SameCanonical, InPathEnv, ManagedBy, BuildOutput, Ephemeral, etc.)
4. Judge each candidate's **durability** (whether the path can be pinned into a service definition; see below)
5. Rank candidates by the selected policy
6. Return the best candidate (or all candidates with `--all`)

## Usage

### CLI

```bash
# Best stable path (default: path format)
stable-which /opt/homebrew/Cellar/jj/0.24.0/bin/jj
# /opt/homebrew/bin/jj

# Command name lookup
stable-which jj
# /opt/homebrew/bin/jj

# All candidates as JSON
stable-which --all --format json jj

# Inspect all candidates as JSON (shorthand for --all --format json)
stable-which --inspect jj

# Prefer path stability over binary identity
stable-which --policy stable ./target/release/myapp
```

### Library

Add the dependency:

```bash
cargo add stable-which
```

```rust
// Keep this example in sync with the crate-root doc example in
// crates/stable-which/src/lib.rs (the canonical, doctest-verified version).
use stable_which::{find_candidates, rank_candidates, Durability, ScoringPolicy};
use std::path::Path;

// 1. Discover candidates (discovery only, deterministic PATH order)
let mut candidates = find_candidates(Path::new("jj"))?;
// 2. Rank them in place by a scoring policy
rank_candidates(&mut candidates, ScoringPolicy::SameBinary);
// 3. Inspect via accessors (fields are private)
for c in &candidates {
    println!("{}: {:?} durable={}", c.path().display(), c.tags(), c.is_stable());
}
let best = &candidates[0];
if best.durability() == Durability::Durable {
    println!("safe to pin: {}", best.path().display());
}
```

`resolve_stable_path(binary, policy)` is a convenience that composes
`find_candidates` + `rank_candidates` and returns the single best candidate.

## CLI Options

```
stable-which [OPTIONS] <binary>

Arguments:
    <binary>         Path to the binary, or a command name to look up in PATH

Options:
    --all            Show all candidates (default: best candidate only)
    --format <F>     Output format: path (default), json
    --policy <P>     Scoring policy: same-binary (default), stable
    --inspect        Show all candidates as JSON (same as --all --format json)
    --help           Show this help message
    --version        Show version
```

## Scoring Policies

| Policy | Priority | Use case |
|---|---|---|
| same-binary (default) | Binary identity > Path stability | Service registration |
| stable | Path stability > Binary identity | Config files that survive upgrades |

## Path Tags

Tags describe properties of each candidate path:

**Positive (green):** Input, InPathEnv, SymlinkTo, SameCanonical, SameContent

**Warning (orange):** ManagedBy, Shim, BuildOutput, Ephemeral, Relative, NonNormalized

**Negative (red):** DifferentBinary

## Durability

Each candidate is also judged on an orthogonal **durability** axis (`durable` /
`not-durable` / `unknown`), exposed as `Candidate::durability()` and the
`is_stable()` convenience (`true` only for `durable`). This answers "can this
path be baked into a launchd plist / systemd unit and survive upgrade and
reboot?":

- **durable**: environment-wide reference surfaces (`/usr/bin`, `/opt/homebrew/bin`, profile bins, standard shim dirs)
- **not-durable**: versioned installs (`Cellar/`, `nix/store/`, `installs/`), ephemeral / build-output / project-local paths
- **unknown**: unrecognized locations and user dropboxes (`~/bin`, `~/.local/bin`) — treated as not safe to pin (safe side)

Durability is judged per candidate, so the reference path `/opt/homebrew/bin/git`
is `durable` while its canonical realpath `/opt/homebrew/Cellar/git/2.44.0/bin/git`
is `not-durable`. The JSON output (`--inspect`) includes a `durability` field per
candidate.

## Install

```bash
brew install kawaz/tap/stable-which
```

Or build from source:

```bash
cargo build --release -p stable-which-cli
```

## License

MIT
