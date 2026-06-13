# stable-which justfile
#
# Canonical task runner. VCS-shaped operations (commit/push/clean check/diff)
# and the translation-pair freshness check delegate to `bump-semver vcs`
# subcommands so the project dogfoods the bump-semver release flow.
#
# Declaration order is intentional: most-used recipes first so `just --list`
# (and `default`) surface them prominently.

set shell := ["bash", "-euo", "pipefail", "-c"]

set script-interpreter := ["bash", "-euo", "pipefail"]

set positional-arguments

# default behaviour: alias for `list`
default: list

# show the recipe list
list:
    @just --list --unsorted

# ---------- atomic (lint / test / build) ----------

# cargo fmt --check + clippy (non-mutating)
[private]
lint-rust:
    cargo fmt --check --all
    cargo clippy --workspace --all-targets -- -D warnings

# just --fmt (justfile self-format check)
[private]
lint-just:
    just --unstable --fmt --check

# lint-rust + lint-just
lint: lint-rust lint-just

# cargo test
test: lint
    cargo test --workspace

# build release binary
build: lint
    cargo build --release -p stable-which-cli

# build then run the local binary, forwarding all args (e.g. `just run /usr/bin/grep`)
run *ARGS: build
    ./target/release/stable-which "$@"

# lint + test + build (CI entry point)
ci: lint test build

# ---------- gates (push の内部、利用者が直接叩くことほぼなし) ----------

# working copy is clean (dogfood: bump-semver vcs is clean)
[private]
ensure-clean:
    bump-semver vcs is clean

# fail if bump-trigger-paths changed since main@origin but Cargo.toml version was not bumped
# Note: Rust tests are embedded via #[cfg(test)] in source files (no separate *_test.rs),
# so per-file test exclusion is not possible — crates/ is used as the trigger in full.
# When main@origin lacks [workspace.package] (e.g. on first push after migration),
# bump-semver compare exits 2; || true passes through to avoid over-blocking.
[private]
[script]
check-version-bumped:
    if ! bump-semver vcs diff -q main@origin -- Cargo.toml crates/; then
        bump-semver compare gt Cargo.toml 'vcs:main@origin:Cargo.toml' || true
    fi

# translation pair freshness check via `bump-semver vcs outdated`
[private]
check-outdated-translations: ensure-clean
    bump-semver vcs outdated 'glob:**/*-ja.md' '$1/$2.md'

# ---------- release flow ----------

# bump Cargo.toml version (default: patch) and create a release commit
bump-version level="patch": ensure-clean
    #!/usr/bin/env bash
    set -euo pipefail
    bump-semver "$1" Cargo.toml --write --quiet
    # Regenerate Cargo.lock (the CLI path-dep carries no version to sync)
    cargo check --quiet
    bump-semver vcs commit \
      -m "Release v$(bump-semver get Cargo.toml)" \
      Cargo.toml Cargo.lock

# push to origin/main with gates
push: ci check-outdated-translations check-version-bumped
    bump-semver vcs push --branch main --jj-bookmark-auto-advance
    @echo "[hint] gh-monitor:watch-workflow --sha $(bump-semver vcs get commit-id --rev main) --on-success release.yml 'just on-success-release' kawaz/stable-which"

# tap pull + brew upgrade after release.yml succeeds (triggered via watch-workflow --on-success)
on-success-release:
    # tap repo を直接 git pull (= `brew update` 全 tap 巡回より速い)
    git -C "$(brew --repository)/Library/Taps/kawaz/homebrew-tap" pull --ff-only
    brew upgrade kawaz/tap/stable-which
    stable-which --version

# ---------- utility ----------

# format source (cargo fmt --all)
fmt:
    cargo fmt --all

# display Cargo.toml version + binary --version output
version:
    echo "Cargo.toml: $(bump-semver get Cargo.toml)"
    if [ -x ./target/release/stable-which ]; then echo "binary: $(./target/release/stable-which --version)"; fi
    if command -v stable-which >/dev/null 2>&1; then echo "installed: $(stable-which --version)"; fi
