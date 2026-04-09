# stable-which

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
4. Score candidates based on the selected policy
5. Return the best candidate (or all candidates with `--all`)

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

```rust
use stable_which::{find_candidates, ScoringPolicy};
use std::path::Path;

let candidates = find_candidates(Path::new("jj"), ScoringPolicy::SameBinary)?;
for c in &candidates {
    println!("{}: {:?}", c.path.display(), c.tags);
}
```

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
