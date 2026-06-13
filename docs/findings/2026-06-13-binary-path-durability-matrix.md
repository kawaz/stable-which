# バイナリパス耐久性 (durability) 実機調査

対象環境: macOS (darwin/Apple Silicon)  
調査日: 2026-06-13

---

## 判明した事実

確定事実のみ。これだけ読めば結論が分かる。

1. **「参照パス durable / realpath versioned」の2段構造が多数存在する。**  
   Homebrew `/opt/homebrew/bin/<name>`、nix home-manager `/etc/profiles/per-user/<user>/bin/<name>`、nix-darwin `/run/current-system/sw/bin/<name>` はいずれも参照パス自体はバージョン非依存だが、`realpath` はバージョン識別子を含む。durability は「どちらの視点で評価するか」で変わる。

2. **シム (shim) を持つマネージャは参照パスが durable になる傾向がある。**  
   mise shim (`~/.local/share/mise/shims/<name>`)、aqua proxy (`~/.local/share/aquaproj-aqua/bin/<name>`)、rye shim (`~/.dotfiles/cache/rye/shims/<name>`) はすべて「参照パス = durable reference、realpath = シム/ディスパッチャ本体」であり、バージョンアップでパスが変わらない。

3. **バージョン識別子がパス中の段数・位置はマネージャごとに大きく異なる。**  
   - Homebrew: `Cellar/<pkg>/<ver>/bin/` (3段目)  
   - mise installs: `installs/<tool>/<ver>/bin/` (3段目)  
   - nix store: `/nix/store/<hash>-<pkg>-<ver>/bin/` (1段目・ハッシュ+バージョン)  
   - claude/cursor-agent: `versions/<ver>/` (2段目)  
   - claude-plugin cache: `cache/<plugin>/<name>/<ver>/bin/` (4段目)  
   - aqua internal: `internal/pkgs/.../<ver>/` (深いネスト)  
   単一の正規表現では捕捉しきれず、パターン群が必要。

4. **`~/.nix-profile/bin/` はこの環境では空 (dead symlink)。home-manager が代替を提供する。**  
   home-manager 環境では `/etc/profiles/per-user/<user>/bin/` が実質的な nix profile の担い手になる。`~/.nix-profile/bin/` を versioned と誤分類しても実害は少ないが、durable な代替パスが未検出になる。

5. **shebang 埋め込み型の versioned 依存 (`rye` の script エントリポイント等) はパス文字列のみでは検出不能。**  
   `~/.local/bin/jupyter` は実体スクリプトでパスにバージョン文字列を持たないが、shebang に `cpython@3.12.8` が埋め込まれており、実質 not durable である。この種の依存は設計上の検出限界。

6. **`aqua` は `bin/aqua` 本体だけが versioned 直リンクであり、他の管理バイナリは aqua-proxy 経由で durable になる非対称構造を持つ。**

7. **`cargo install` (`~/.cargo/bin/<name>`)、`go install` (`~/go/bin/<name>`)、`moon` (`~/.moon/bin/moon`) はパスにバージョン識別子を持たず、upgrade で上書きされる形式のため参照パスとして durable。**

---

## 実用的な示唆 / durability 判定への影響

- **「参照パスが durable か」と「realpath が durable か」を分けて評価すること。**  
  焼き込む候補として最適なのは「参照パス durable かつ upgrade で追従するシム/プロキシ経由」のケース。`realpath` だけを見ると Homebrew・nix 系が durable に見えないが、参照パスは安定している。

- **nix home-manager と nix-darwin の `/run/current-system/` は durable として扱うべき。**  
  現 `path_analysis.rs` の INSTALL_PATTERNS には欠落しており、versioned (not durable) と誤分類される可能性がある。

- **`~/.nix-profile/bin/` を versioned と分類するのは誤り。**  
  参照パス自体にバージョン識別子はなく、構造上は durable reference に近い。ただしこの環境では空であるため実際の焼き込み候補には上がらない。

- **`rye cpython@ver`、`claude versions/<ver>`、`cursor-agent versions/<ver>`、プラグインキャッシュ `/<ver>/bin/` のパターンが INSTALL_PATTERNS に欠落。**  
  これらは versioned だが現状 durable と誤分類される可能性がある。

- **`~/.cache/` 配下 (jj-worktree 等) は ephemeral として扱う方針と合致しており、既存の EPHEMERAL_PATTERNS が機能すれば問題ない。**  
  ただし jj-worktree の場合はさらに shim (git → jj-worktree) かつ versioned という3要素が重なるため、どのパターンが先に適用されるかで分類が変わる。

---

## 検証の詳細

実機 PATH エントリ (darwin/Apple Silicon) からサンプリングした全マネージャの観測結果。

| マネージャ | 実パス例 | symlink チェーン | 分類 | バージョン識別子の位置 | 焼き込み耐久性 |
|-----------|----------|-----------------|------|----------------------|--------------|
| **Homebrew** (bin) | `/opt/homebrew/bin/node` | `→ ../Cellar/node/26.0.0/bin/node` | durable reference | Cellar 直下の3段目 | ◎ 参照パス durable |
| **Homebrew** (Cellar) | `/opt/homebrew/Cellar/node/26.0.0/bin/node` | 実体 | versioned | 3段目 (`/26.0.0/`) | ✗ not durable |
| **mise** (shim) | `~/.local/share/mise/shims/node` | `→ mise 本体` | durable reference | なし | ◎ 参照パス durable |
| **mise** (installs 実体) | `~/.local/share/mise/installs/node/22.22.0/bin/node` | 実体 | versioned | 3段目 (`/22.22.0/`) | ✗ not durable |
| **mise** (alias) | `~/.local/share/mise/installs/node/22/` | `→ ./22.22.0` | versioned alias | エイリアス先が versioned | ✗ not durable |
| **nix store** | `/nix/store/<hash>-git-2.54.0/bin/git` | 実体 | versioned | 1段目 (ハッシュ+ver) | ✗ not durable |
| **nix ~/.nix-profile** | `~/.nix-profile/bin/` | 空ディレクトリ (dead) | — | — | — (この環境では使用不可) |
| **nix home-manager** | `/etc/profiles/per-user/<user>/bin/curl` | `→ /nix/store/<hash>-curl-.../bin/curl` | durable reference | realpath のみに ver | ◎ 参照パス durable |
| **nix-darwin** | `/run/current-system/sw/bin/git` | `→ /nix/store/<hash>-.../bin/git` | durable reference | realpath のみに ver | ◎ 参照パス durable |
| **Determinate Nix** | `/nix/var/nix/profiles/default/bin/nix` | 実体 | durable reference | なし | ◎ durable |
| **aqua** (proxy) | `~/.local/share/aquaproj-aqua/bin/<name>` | `→ aqua-proxy` | durable reference | なし (proxy が解決) | ◎ 参照パス durable |
| **aqua** (本体 bin) | `~/.local/share/aquaproj-aqua/bin/aqua` | `→ internal/pkgs/.../v2.56.2/...` | versioned | 深いネスト中に ver | ✗ not durable (例外的) |
| **aqua** (internal/pkgs) | `~/.local/share/aquaproj-aqua/internal/pkgs/...` | 実体 | versioned | パス中に ver | ✗ not durable |
| **rustup/cargo** | `~/.cargo/bin/<name>` | 実体 | durable | バージョン識別子なし | ◎ durable (upgrade で上書き) |
| **rye** (shim) | `~/.dotfiles/cache/rye/shims/python` | `→ rye dispatcher` | durable reference | なし | ◎ 参照パス durable |
| **rye** (cpython 実体) | `~/.dotfiles/cache/rye/py/cpython@3.12.8/bin/python` | 実体 | versioned | `cpython@<ver>` | ✗ not durable |
| **rye** (script) | `~/.local/bin/jupyter` | 実体スクリプト | 見かけ上 durable | パスに ver なし (shebang に埋め込み) | △ 実質 not durable (検出不能) |
| **go install** | `~/go/bin/gopls` | 実体 | durable | なし | ◎ durable |
| **claude CLI** | `~/.local/bin/claude` | `→ ~/.local/share/claude/versions/2.1.177/...` | versioned symlink | `versions/<ver>/` (2段目) | ✗ not durable |
| **cursor-agent** | `~/.local/bin/cursor-agent` | `→ versions/2025.09.17-25b418f/...` | versioned symlink | `versions/<date>-<hash>/` (2段目) | ✗ not durable |
| **claude-plugin cache** | `~/.claude-personal/plugins/cache/cmux-msg/cmux-msg/0.29.0/bin/cmux-msg` | 実体 | versioned | 4段目 (`/0.29.0/`) | ✗ not durable |
| **jj-worktree cache** | `~/.cache/jj-worktree/bin/git` | `→ /opt/homebrew/bin/jj-worktree → Cellar/jj-worktree/0.2.3/...` | ephemeral + shim (名前不一致) + versioned | 複合 | ✗ not durable (ephemeral 優先) |
| **moon** | `~/.moon/bin/moon` | 実体 | durable | なし | ◎ durable |
| **system** | `/usr/bin/git` | 実体 | durable | なし | ◎ durable |

**PATH エントリ一覧 (観測順)**

```
~/.claude-personal/plugins/cache/.../bin
/Applications/cmux.app/.../bin
~/go/bin
~/.cache/jj-worktree/bin
~/bin
~/.local/bin
~/.dotfiles/bin
~/.moon/bin
~/.dotfiles/cache/.bun/bin
/opt/homebrew/bin
/opt/homebrew/sbin
~/.nix-profile/bin           # 空・dead
/etc/profiles/per-user/<user>/bin   # nix home-manager
/run/current-system/sw/bin   # nix-darwin
/nix/var/nix/profiles/default/bin  # Determinate Nix
/usr/local/bin
/usr/bin
/bin
/usr/sbin
/sbin
```

**未調査マネージャ (インストール未確認、推測で埋めない)**

asdf, nvm, fnm, volta, pyenv, rbenv, goenv, sdkman, proto, scoop, chocolatey, winget, `node_modules/.bin`, `.venv`, `.direnv`

---

## 現 path_analysis.rs の取りこぼし

`crates/stable-which/src/path_analysis.rs` の `INSTALL_PATTERNS` / `SHIM_PATTERNS` において、以下のパターンが欠落または誤分類している。

### 欠落 (durable パスが検出されていない)

| パス例 | 問題 | 影響 |
|-------|------|------|
| `/run/current-system/sw/bin/` | INSTALL_PATTERNS に未定義 | nix-darwin の durable パスを未検出 or 誤分類 |
| `/etc/profiles/per-user/<user>/bin/` | INSTALL_PATTERNS に未定義 | nix home-manager の durable パスを未検出 |
| `/nix/var/nix/profiles/default/bin/` | 確認要 | Determinate Nix の durable パスを未検出の可能性 |

### 誤分類 (not durable なパスを durable と見なす可能性)

| パス例 | 問題 | 影響 |
|-------|------|------|
| `~/.local/share/claude/versions/<ver>/` | バージョン識別子パターン未定義 | versioned を durable と誤分類 |
| `versions/<date>-<hash>/` (cursor-agent 等) | 同上 | versioned を durable と誤分類 |
| `~/.dotfiles/cache/rye/py/cpython@<ver>/bin/` | `cpython@<ver>` パターン未定義 | versioned を durable と誤分類 |
| `~/.claude-personal/plugins/cache/<p>/<n>/<ver>/bin/` | プラグインキャッシュの4段目 ver 未定義 | versioned を durable と誤分類 |
| `~/.local/share/aquaproj-aqua/bin/aqua` (本体のみ) | versioned 直リンクだが SHIM_PATTERNS が `/aquaproj-aqua/bin/` 全体に適用されている可能性 | aqua 本体だけ誤分類 |

### 構造的誤分類

| パス例 | 問題 | 影響 |
|-------|------|------|
| `~/.nix-profile/bin/<name>` | INSTALL_PATTERNS で versioned 扱いとなっているが、参照パス自体は durable reference | この環境では dead なので実害なし、他環境では誤分類 |

---

## 既知の限界

1. **「参照パス durable / realpath versioned」の2段構造は候補ごとに評価が必要。**  
   パス文字列だけではどちらの性質を持つかを決定できないケースが存在する。`realpath` の追跡と組み合わせた評価が必要。

2. **durability は実行環境依存。**  
   `~/.nix-profile/bin/` がこの環境で dead であるように、同じパスでも環境によって実態が異なる。存在確認 (`stat` / `realpath`) なしに durability を断言できない。

3. **shebang 埋め込み型の versioned 依存は検出不能。**  
   スクリプトの実体がバージョン識別子を持たないパスに置かれていても、内部の shebang がバージョン固定の interpreter を参照している場合、パス文字列のみでは not durable と判定できない。stable-which の設計スコープ外の限界として受容する。

4. **未調査マネージャ (asdf, volta, fnm 等) のパターンは不明。**  
   今回の実機環境にインストールされていないため観測不可。将来の調査対象。
