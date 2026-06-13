# stable-which justfile

set shell := ["bash", "-euo", "pipefail", "-c"]

# デフォルト: レシピ一覧
default:
    @just --list

# ビルド (release)
build:
    cargo build --release -p stable-which-cli

# テスト
test:
    cargo test --workspace

# lint + format チェック
check:
    cargo fmt --check --all
    cargo clippy --workspace -- -D warnings

# format 適用
fmt:
    cargo fmt --all

# ビルドして実行
run *ARGS: build
    ./target/release/stable-which {{ARGS}}

# [workspace.package] version を bump する (bump: major / minor / patch)
# version 値を Cargo.toml に書き込み、CLI の path 依存 version も同期更新する。
# commit/push はしない。tag は release.yml が打つ。
bump bump="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patchv <<< "$current"
    case "{{bump}}" in
        major) major=$((major + 1)); minor=0; patchv=0 ;;
        minor) minor=$((minor + 1)); patchv=0 ;;
        patch) patchv=$((patchv + 1)) ;;
        *) echo "Error: Invalid bump type '{{bump}}' (use major/minor/patch)" >&2; exit 1 ;;
    esac
    new_version="${major}.${minor}.${patchv}"
    # BSD/GNU 両対応: perl -i で in-place 置換
    perl -i -pe "s/^version = \"${current}\"/version = \"${new_version}\"/" Cargo.toml
    # CLI の path 依存 version を同期更新 (stable-which = { path = "...", version = "..." } 行のみ)
    cli_toml="crates/stable-which-cli/Cargo.toml"
    perl -i -pe "s/(stable-which\s*=\s*\{[^}]*version\s*=\s*\")${current}(\")/${1}${new_version}${2}/" "${cli_toml}"
    cargo check --quiet
    echo "Version: ${current} -> ${new_version}"

# main へ push する (push-guard hook の正規経路)
push:
    jj git push --bookmark main

# リリース: bump → check → commit → push
# tag は release.yml が自動で打つ。手動で tag を打たない。
release bump="patch": (bump bump)
    #!/usr/bin/env bash
    set -euo pipefail
    new_version=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

    # CI チェック
    cargo fmt --check --all || { echo "Error: Run 'just fmt' first." >&2; exit 1; }
    cargo clippy --workspace -- -D warnings
    cargo test --workspace

    # commit + push (jj 経由)
    jj describe -m "Release v${new_version}"
    jj new
    jj bookmark set main -r @-
    just push

    echo "Pushed v${new_version} to main. release.yml will create the tag and GH Release."
