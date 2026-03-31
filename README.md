# stable-which

Resolve the stable PATH entry for a binary, verified by canonical path identity.

## Problem

Package managers like Homebrew place binaries in versioned directories:

```
/opt/homebrew/Cellar/jj/0.24.0/bin/jj
```

After `brew upgrade`, the version directory changes and the old path becomes invalid. Meanwhile, stable symlinks exist on PATH:

```
/opt/homebrew/bin/jj -> ../Cellar/jj/0.24.0/bin/jj
```

`realpath` gives you the versioned path. What you actually want is the stable symlink.

## Why not just `which`?

`which` finds a command by name on PATH, but it does **not** verify that the found entry points to the same binary. If multiple versions coexist (e.g. system vs Homebrew), `which` may return a different binary entirely.

`stable-which` verifies identity by comparing canonical paths, ensuring the returned PATH entry points to the exact same file.

## How it works

1. Canonicalize the input binary path
2. Extract the file name and search PATH directories
3. For each candidate with the same name, compare canonical paths
4. Match found: return the PATH entry (`in_path: true`)
5. No match: return the canonical path as fallback (`in_path: false`)

## Usage

```bash
# JSON output (default)
stable-which /opt/homebrew/Cellar/jj/0.24.0/bin/jj
# {"path":"/opt/homebrew/bin/jj","in_path":true,"canonical":"/opt/homebrew/Cellar/jj/0.24.0/bin/jj"}

# Path only
stable-which --path-only /opt/homebrew/Cellar/jj/0.24.0/bin/jj
# /opt/homebrew/bin/jj

# Works with any binary path
stable-which --path-only "$(realpath "$(which jj)")"
# /opt/homebrew/bin/jj
```

## Install

```bash
brew install kawaz/tap/stable-which
```

Or build from source:

```bash
cargo build --release
```

## License

MIT
