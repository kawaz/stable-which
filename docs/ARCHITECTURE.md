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

```
path: PathBuf       -- エントリパス（タグ評価対象）
canonical: PathBuf  -- realpath（symlink 全解決済み）
tags: Vec<PathTag>  -- 付与されたタグ
```

`path_order()` メソッドで `InPathEnv` の発見順を取得。Input 候補は `usize::MAX`（tie-break で最低優先）。

### ScoringPolicy

| ポリシー | 重み | ユースケース |
|---|---|---|
| SameBinary（デフォルト） | binary × 1000 + stability × 10 + bonus + penalty | サービス登録（同一バイナリ重視） |
| Stable | stability × 1000 + binary × 10 + bonus + penalty | 設定ファイル（パス安定度重視） |

スコア要素:
- binary_score: SameCanonical=3, SameContent=2, DifferentBinary=0
- stability_score: クリーン=3, ManagedBy/Shim=1, BuildOutput/Ephemeral=0
- in_path_bonus: InPathEnv=+5
- penalty: Relative=-3, NonNormalized=-2（累積）

ソート: スコア降順 → 同スコアは `path_order()` 昇順（PATH 先頭が優先）。

### Error

`#[non_exhaustive]` enum。バリアント: NotFound, NotAFile, NoFileName, NotInPath, Canonicalize, Metadata。`impl Display` + `impl std::error::Error`。

## API

```rust
find_candidates(binary, policy) -> Result<Vec<Candidate>, Error>
find_candidates_with_env(binary, path_env, policy) -> Result<Vec<Candidate>, Error>
resolve_stable_path(binary, policy) -> Result<Candidate, Error>
detect_version_manager(path) -> Option<VersionManagerInfo>
files_have_same_content(path_a, path_b) -> bool
is_executable(path) -> bool
```

ルート re-export あり: `stable_which::find_candidates` で直接利用可能。

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

- 安定性は不安定パターンの不在で判定（ホワイトリストではない）
- タグは candidate.path に対して評価（canonical ではない）
- タグは客観的属性、スコアは主観的重み付け（分離）
- ライブラリは依存ゼロ（Serialize 等は CLI 側）
- 同スコアの候補は PATH 発見順で決定的に tie-break

## 関連ドキュメント

- [Design Records](decisions/) — 個別の設計判断とその理由
- [調査レポート](reports/) — パスパターン、サービス登録の調査結果
