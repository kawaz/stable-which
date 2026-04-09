# stable-which

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

# リリース (bump: major, minor, patch)
release bump="patch":
    #!/usr/bin/env bash
    set -euo pipefail

    # Pre-checks
    cargo fmt --check --all || { echo "Error: Run 'cargo fmt' first." >&2; exit 1; }
    cargo clippy --workspace -- -D warnings
    cargo test --workspace

    # Version bump (update both crate Cargo.toml files)
    current=$(grep '^version' crates/stable-which/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patchv <<< "$current"
    case "{{bump}}" in
        major) major=$((major + 1)); minor=0; patchv=0 ;;
        minor) minor=$((minor + 1)); patchv=0 ;;
        patch) patchv=$((patchv + 1)) ;;
        *) echo "Error: Invalid bump type '{{bump}}'" >&2; exit 1 ;;
    esac
    new_version="${major}.${minor}.${patchv}"
    sed -i '' "s/^version = \"${current}\"/version = \"${new_version}\"/" crates/stable-which/Cargo.toml crates/stable-which-cli/Cargo.toml
    cargo check --quiet
    echo "Version: ${current} -> ${new_version}"

    # Commit, tag, push
    jj describe -m "Release v${new_version}"
    jj new
    jj bookmark set main -r @-
    jj tag set "v${new_version}" -r @-
    jj git push --bookmark main
    jj git export
    GIT_WORK_TREE="$(pwd)" git --git-dir="$(jj root)/../.git" push origin "v${new_version}"

    # Watch workflow
    sleep 3
    run_id=$(gh run list --repo kawaz/stable-which --limit 1 --json databaseId -q '.[0].databaseId')
    gh run watch "$run_id" --repo kawaz/stable-which
