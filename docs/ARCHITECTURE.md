# Architecture

stable-which はバイナリパスの安定性を評価し、PATH 上の全候補をタグ付きで列挙するツール/ライブラリ。

## Workspace 構成

| Crate | 役割 | 依存 | Publish |
|---|---|---|---|
| `stable-which` | ライブラリ | なし（std のみ） | crates.io |
| `stable-which-cli` | CLI バイナリ | stable-which, serde, serde_json | No（Homebrew 配布） |

## データモデル

### PathTag

候補パスの属性を表す enum。3色分類:

**緑 (positive):** Input, InPathEnv(order), SymlinkTo(target), SameCanonical, SameContent
**オレンジ (warning):** ManagedBy(name), Shim, BuildOutput, Ephemeral, Relative, NonNormalized
**赤 (negative):** DifferentBinary

`InPathEnv(usize)` は PATH 上の発見順序を保持（0 = 最初のマッチ）。同スコア時の tie-break に使用。

### Candidate

フィールドは private（DR-015 Decision 1）。アクセサ経由で参照する:

```
path() -> &Path           -- エントリパス（タグ・durability 評価対象、canonical とは限らない）
canonical() -> &Path       -- canonicalize 成功時は realpath、失敗時は入力パスにフォールバック
tags() -> &[PathTag]       -- 付与されたタグ（順序非保証）
durability() -> Durability -- 焼き込み耐久性（下記）
is_stable() -> bool        -- matches!(durability(), Durable) の convenience
```

内部 `path_order()`（`pub(crate)`）で `InPathEnv` の発見順を取得。Input 候補は `usize::MAX`（tie-break で最低優先）。テスト用に `Candidate::for_test(..)`（`#[cfg(test)]`）。

### Durability（DR-016）

`PathTag` とは直交する別軸 enum。「そのパスを launchd plist / systemd unit に焼き込んで rebuild / upgrade / reboot を跨いで生き続けるか（durable-to-pin）」を表す。`#[non_exhaustive]`。

| variant | 意味 |
|---|---|
| Durable | 環境全体の参照面（system bin / profile bin / 標準 shim 等）。焼き込んでよい |
| NotDurable | versioned-install / ephemeral / build-output / project-local。焼き込むと壊れうる |
| Unknown | 既知パターン非該当（user dropbox `~/bin`・`~/.local/bin` 等）。安全側 = not-durable 扱い |

判定は allow-list 方式・候補ごと（DR-016 Decision 3/4）:

1. NotDurable 先行: versioned-install | ephemeral | build-output | project-local
2. durable location（厳格マッチ）: 直下ディレクトリ完全一致（`/usr/bin`, `/usr/local/bin`, `/opt/homebrew/bin` 等）/ `/etc/profiles/per-user/<user>/bin/<file>` の構造一致 / HOME アンカーされた標準 shim（`~/.local/share/mise/shims/` 等）
3. それ以外 → Unknown

部分一致（`contains`）は使わず構造で照合する（`/usr/local/binutils` や project-local な `/repo/.mise/shims/` の誤判定を防ぐ）。Scope（project-local）は内部のみ、公開しない。

### ScoringPolicy

| ポリシー | 重み | ユースケース |
|---|---|---|
| SameBinary（デフォルト） | binary × 1000 + preference_tier × 10 + bonus + penalty | サービス登録（同一バイナリ重視） |
| Stable | preference_tier × 1000 + binary × 10 + bonus + penalty | 設定ファイル（パス安定度重視） |

スコア要素:
- binary_score: SameCanonical=3, SameContent=2, DifferentBinary=0
- preference_tier: クリーン=3, ManagedBy/Shim=1, BuildOutput/Ephemeral=0（tier 2 は将来用に予約）
- in_path_bonus: InPathEnv=+5
- penalty: Relative=-3, NonNormalized=-2（累積）

`preference_tier` は durability（DR-016）とは別概念（同スコア帯内の選好ティア）。`score()` は `pub(crate)`（公開しない）。ランキングは `rank_candidates` のソート結果として一意に表現する。

ソート: スコア降順 → 同スコアは `path_order()` 昇順（PATH 先頭が優先）。

### Error

`#[non_exhaustive]` enum。バリアント: NotFound, NotAFile, NoFileName, NotInPath, Canonicalize, Metadata。`impl Display` + `impl std::error::Error`。

## API（find / rank / resolve の 3 層分離、DR-015 Decision 2）

```rust
// 探索のみ（policy なし、PATH 発見順で決定的）
find_candidates(binary) -> Result<Vec<Candidate>, Error>
find_candidates_with_path_env(binary, path_env) -> Result<Vec<Candidate>, Error>
// 順位付け（in-place ソート）
rank_candidates(candidates: &mut [Candidate], policy)
// 解決（find + rank の合成、最良候補を返す）
resolve_stable_path(binary, policy) -> Result<Candidate, Error>
```

`find_candidates*` は成功時に必ず非空（入力候補を常に 1 件は返す）。候補ゼロは `Err` のみ。

公開型: `Candidate`, `PathTag`, `ScoringPolicy`, `Durability`, `Error` をクレートルートから re-export（`stable_which::Candidate` 等）。`candidate` / `durability` / `path_analysis` モジュール、および `detect_version_manager` / `VersionManagerInfo` / `is_executable` / `files_have_same_content` 等の内部ヘルパーは非公開（DR-015 Decision 5/6）。

## CLI

```
stable-which [OPTIONS] <binary>

--all            全候補表示
--format <F>     path（デフォルト）| json
--policy <P>     same-binary（デフォルト）| stable
--inspect        --all --format json のショートハンド
--help           ヘルプ（stdout）
--version        バージョン
```

引数なし実行は exit 1。`--help` 明示は exit 0。

## 検出パターン

### バージョンマネージャ (ManagedBy)

mise, asdf, nix, homebrew, nvm, fnm, rustup, volta, sdkman, pyenv, rbenv, goenv, aqua, proto

### シム (Shim)

- ディレクトリパターン: `/mise/shims/`, `/asdf/shims/`, `/pyenv/shims/` 等
- ヒューリスティック: symlink 先の名前が候補名の prefix でない場合（`git` → `jj-worktree`）

### ビルド成果物 (BuildOutput)

`target/debug/`, `target/release/`, `.build/debug/`, `dist-newstyle/`, `DerivedData/`, `zig-out/` 等

### 一時パス (Ephemeral)

`path.parent()` に対して `\b(cache|tmp|temp|temporary)\b` を case insensitive でマッチ。`.app` バンドル内は除外。

### 実行ビットチェック

PATH 候補は Unix 実行ビット (`mode & 0o111`) をチェック。入力バイナリ自体は `is_file()` のみ（明示指定なので実行ビットがなくても分析対象）。

## ファイル同一性判定

バイト単位ストリーミング比較（依存ゼロ）。ファイルサイズ不一致で即棄却（O(1)）、一致時のみバイト比較（8KB バッファ）。暗号学的ハッシュは使用しない。

## 設計原則

- durability（焼き込み耐久性）は allow-list 方式で判定（NotDurable 先行 → durable allow-list → Unknown）。判定不能は安全側 Unknown に倒す（DR-016）
- preference_tier（スコア内の選好）は不安定パターンの不在で算出。durability とは別軸
- タグ・durability は candidate.path() に対して評価（canonical ではない）。durability は候補ごと
- タグは客観的属性、スコアは主観的重み付け（分離）。スコア絶対値は非公開、順位は rank_candidates で表現
- ライブラリは依存ゼロ（Serialize 等は CLI 側）
- 同スコアの候補は PATH 発見順で決定的に tie-break

## 関連ドキュメント

- [Design Records](decisions/) — 個別の設計判断とその理由
- [調査レポート](reports/) — パスパターン、サービス登録の調査結果
