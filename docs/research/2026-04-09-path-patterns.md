# バイナリの「不安定パス」パターン調査

## 1. 言語別ビルド出力パス（開発時の一時パス）

| 言語/ビルドシステム | 不安定パスパターン | 検出文字列 |
|---|---|---|
| **Rust (cargo)** | `target/debug/<name>`, `target/release/<name>` | `target/debug/`, `target/release/` |
| **Go** | プロジェクト内にはビルド出力なし | — |
| **C/C++ (cmake)** | `build/`, `cmake-build-debug/`, `cmake-build-release/` | `cmake-build-` |
| **C/C++ (meson)** | `builddir/`, `build/` | `builddir/` |
| **Java (maven)** | `target/<name>.jar` | `target/` |
| **Java (gradle)** | `build/libs/`, `build/classes/` | `build/libs/`, `build/bin/` |
| **Kotlin/Native** | `build/bin/native/` | `build/bin/` |
| **Swift (SwiftPM)** | `.build/debug/`, `.build/release/` | `.build/debug/`, `.build/release/` |
| **Swift (Xcode)** | `DerivedData/<project>-<hash>/Build/Products/` | `DerivedData/`, `Build/Products/` |
| **Zig** | `zig-out/bin/`, `zig-cache/` | `zig-out/`, `zig-cache/` |
| **Nim** | `nimcache/` | `nimcache/` |
| **D言語 (dub)** | `.dub/build/` | `.dub/build/` |
| **Haskell (cabal)** | `dist-newstyle/build/...` | `dist-newstyle/` |
| **Haskell (stack)** | `.stack-work/dist/...` | `.stack-work/` |
| **OCaml (dune)** | `_build/default/` | `_build/default/` |
| **Erlang (rebar3)** | `_build/default/` | `_build/default/` |
| **Elixir (mix)** | `_build/dev/`, `_build/prod/` | `_build/dev/`, `_build/prod/` |
| **Dart** | `.dart_tool/` | `.dart_tool/` |
| **MoonBit** | `target/wasm-gc/debug/build/`, `target/wasm-gc/release/build/` | `target/wasm-gc/`, `target/wasm/`, `target/js/` |
| **.NET (dotnet)** | `bin/Debug/net<ver>/`, `bin/Release/net<ver>/` | `bin/Debug/`, `bin/Release/` |

## 2. パッケージマネージャの bin/shim パターン

### 2a. バージョンマネージャ（installs + shims）

| ツール | 安定パス（shim） | 不安定パス（実バイナリ） | 検出文字列 | shim種別 |
|---|---|---|---|---|
| **mise** | `~/.local/share/mise/shims/` | `~/.local/share/mise/installs/<plugin>/<ver>/bin/` | `/mise/installs/`, `/.mise/installs/` | 実行バイナリ（mise本体コピー） |
| **asdf** | `~/.asdf/shims/` | `~/.asdf/installs/<plugin>/<ver>/bin/` | `.asdf/installs/` | bashスクリプト |
| **nvm** | なし（PATH書換） | `~/.nvm/versions/node/v<ver>/bin/` | `.nvm/versions/node/v` | PATH直接操作 |
| **fnm** | なし（PATH書換） | `~/.fnm/node-versions/v<ver>/installation/bin/` | `.fnm/node-versions/v` | PATH直接操作 |
| **volta** | `~/.volta/bin/` | `~/.volta/tools/image/node/<ver>/bin/` | `.volta/tools/image/` | shimバイナリ（volta-shim） |
| **pyenv** | `~/.pyenv/shims/` | `~/.pyenv/versions/<ver>/bin/` | `.pyenv/versions/` | bashスクリプト |
| **uv** | なし（venv経由） | `~/.local/share/uv/python/cpython-<ver>-<arch>/bin/` | `.local/share/uv/python/cpython-` | venv内symlink |
| **rbenv** | `~/.rbenv/shims/` | `~/.rbenv/versions/<ver>/bin/` | `.rbenv/versions/` | bashスクリプト |
| **rvm** | なし（PATH書換） | `~/.rvm/rubies/ruby-<ver>/bin/` | `.rvm/rubies/ruby-` | PATH直接操作 |
| **rustup** | `~/.cargo/bin/` | `~/.rustup/toolchains/<toolchain>/bin/` | `.rustup/toolchains/` | proxy実行バイナリ |
| **sdkman** | `~/.sdkman/candidates/<tool>/current/bin/` | `~/.sdkman/candidates/<tool>/<ver>/bin/` | `.sdkman/candidates/` | `current` symlink |
| **goenv** | `~/.goenv/shims/` | `~/.goenv/versions/<ver>/bin/` | `.goenv/versions/` | bashスクリプト |
| **juliaup** | `~/.juliaup/bin/julia` | `~/.julia/juliaup/julia-<ver>+0/bin/julia` | `.julia/juliaup/julia-` | launcher |
| **aqua** | `~/.local/share/aquaproj-aqua/bin/` | `aquaproj-aqua/internal/pkgs/github_release/<owner>/<repo>/v<ver>/` | `aquaproj-aqua/internal/pkgs/` | aqua-proxy symlink |
| **proto (moonrepo)** | `~/.proto/shims/` | `~/.proto/tools/<tool>/<ver>/bin/` | `.proto/tools/` | shimスクリプト |

### 2b. システムパッケージマネージャ

| ツール | 安定パス | 不安定パス | 検出文字列 | リンク種別 |
|---|---|---|---|---|
| **Homebrew (macOS arm64)** | `/opt/homebrew/bin/` | `/opt/homebrew/Cellar/<pkg>/<ver>/bin/` | `/Cellar/` | symlink |
| **Homebrew (macOS Intel)** | `/usr/local/bin/` | `/usr/local/Cellar/<pkg>/<ver>/bin/` | `/Cellar/` | symlink |
| **Homebrew (Linux)** | `/home/linuxbrew/.linuxbrew/bin/` | `/home/linuxbrew/.linuxbrew/Cellar/<pkg>/<ver>/bin/` | `/Cellar/` | symlink |
| **MacPorts** | `/opt/local/bin/` | `/opt/local/bin/<cmd><ver>` | — | select切替 |
| **Nix** | `~/.nix-profile/bin/`, `/run/current-system/sw/bin/` | `/nix/store/<hash>-<pkg>-<ver>/bin/` | `/nix/store/` | 多段symlink |
| **Guix** | `~/.guix-profile/bin/` | `/gnu/store/<hash>-<pkg>-<ver>/bin/` | `/gnu/store/` | symlink |
| **apt/dpkg** | `/usr/bin/` | `/usr/bin/` | — | 直接配置（安定） |
| **rpm/dnf** | `/usr/bin/` | `/usr/bin/` | — | 直接配置（安定） |
| **pacman** | `/usr/bin/` | `/usr/bin/` | — | 直接配置（安定） |
| **Snap** | `/snap/bin/` | `/snap/<pkg>/<revision>/bin/` | `/snap/` + リビジョン | wrapper |
| **Flatpak** | `/var/lib/flatpak/exports/bin/` | `/var/lib/flatpak/app/<app-id>/<arch>/<branch>/<hash>/` | `/flatpak/app/` | symlink |
| **AppImage** | 手動配置 | 自己完結型 | `.AppImage` | 直接実行 |

### 2c. 言語固有パッケージマネージャ

| ツール | 安定パス | 不安定パス | 検出文字列 |
|---|---|---|---|
| **npm (global)** | node経由 | `node_modules/<pkg>/bin/` | `node_modules/` |
| **npm (local .bin)** | `node_modules/.bin/` | — | `node_modules/.bin/` |
| **pipx** | `~/.local/bin/` | `~/.local/pipx/venvs/<pkg>/bin/` | `.local/pipx/venvs/` |
| **cargo install** | `~/.cargo/bin/` | — | — (安定) |
| **go install** | `~/go/bin/` | — | — (安定) |
| **gem** | `/usr/local/bin/` | `<gemdir>/gems/<pkg>-<ver>/bin/` | `/gems/` |
| **opam** | `~/.opam/<switch>/bin/` | 同左 | `.opam/` |
| **nimble** | `~/.nimble/bin/` | `~/.nimble/pkgs2/<pkg>-<ver>-<hash>/bin/` | `.nimble/pkgs2/` |
| **dotnet tool** | `~/.dotnet/tools/` | `~/.dotnet/tools/.store/<pkg>/<ver>/` | `.dotnet/tools/.store/` |

### 2d. コンテナ

| ツール | 安定パス | 不安定パス | 検出文字列 |
|---|---|---|---|
| **Docker (OrbStack)** | `~/.orbstack/bin/docker` | `/Applications/OrbStack.app/Contents/MacOS/xbin/` | `OrbStack.app/Contents/` |
| **Docker Desktop** | `/usr/local/bin/docker` | `/Applications/Docker.app/Contents/Resources/bin/` | `Docker.app/Contents/` |

## 3. 不安定度ランキング（検出優先度順）

| ランク | パターン | 検出文字列 | 不安定の原因 |
|---|---|---|---|
| 1 | Nix store | `/nix/store/` | ハッシュが毎ビルドで変化 |
| 2 | Guix store | `/gnu/store/` | 同上 |
| 3 | Xcode DerivedData | `DerivedData/` | ランダムハッシュ |
| 4 | Homebrew Cellar | `/Cellar/` | バージョン更新で変化 |
| 5 | nvm/fnm | `.nvm/versions/`, `.fnm/node-versions/` | バージョン切替でPATH変化 |
| 6 | mise/asdf installs | `/installs/` | バージョン付きパス |
| 7 | pyenv/rbenv/goenv | `/versions/` | バージョン付きパス |
| 8 | rustup toolchains | `.rustup/toolchains/` | ツールチェイン名付き |
| 9 | volta tools | `.volta/tools/image/` | バージョン付きパス |
| 10 | Snap | `/snap/` + リビジョン | 自動更新 |
| 11 | Flatpak | `/flatpak/app/` | ハッシュ付き |
| 12 | Cargo target | `target/debug/`, `target/release/` | ビルド成果物 |

## 4. 検出パターンまとめ

### 不安定パス検出パターン（grep/contains用）

```
# ハッシュベース（最も不安定）
/nix/store/
/gnu/store/
DerivedData/

# バージョン付きパッケージマネージャ
/Cellar/
/Caskroom/

# バージョンマネージャ installs/versions
/installs/
/versions/
.nvm/versions/
.fnm/node-versions/
.rustup/toolchains/
.volta/tools/
.sdkman/candidates/
.juliaup/
.julia/juliaup/

# 言語パッケージマネージャ
node_modules/.bin/
node_modules/
.yarn/unplugged/
.local/share/pnpm/global/
.local/pipx/venvs/
/gems/
.nimble/pkgs
.pub-cache/hosted/
.dotnet/tools/.store/
.opam/

# その他ツール
aquaproj-aqua/internal/pkgs/
.proto/tools/
/snap/
/flatpak/app/

# ビルド成果物
target/debug/
target/release/
.build/debug/
.build/release/
dist-newstyle/
.stack-work/
_build/
zig-out/
zig-cache/
cmake-build-
bin/Debug/
bin/Release/
.dub/build/
nimcache/
```

### シムパターン

```
/mise/shims/
/.mise/shims/
/asdf/shims/
/.asdf/shims/
/.pyenv/shims/
/.rbenv/shims/
/.goenv/shims/
/.proto/shims/
```

### 安定パス

| パス | 理由 |
|---|---|
| `/usr/bin/`, `/usr/sbin/`, `/bin/`, `/sbin/` | システム管理 |
| `/usr/local/bin/` | 手動インストール先 |
| `~/bin/` | ユーザー慣習的な手動配置先 |
| `/opt/homebrew/bin/`, `/opt/homebrew/sbin/` | Homebrew symlink |
| `~/.local/bin/` | XDG準拠 |
| `~/.cargo/bin/` | cargo install / rustup proxy |
| `~/go/bin/` | go install |
| `~/.volta/bin/` | volta shim |
| `~/.local/share/mise/shims/` | mise shim |
| `~/.asdf/shims/` | asdf shim |
| `~/.nix-profile/bin/` | Nix profile |
| `/run/current-system/sw/bin/` | NixOS system |
| `~/.local/share/aquaproj-aqua/bin/` | aqua proxy |
| `/snap/bin/` | Snap frontend |
