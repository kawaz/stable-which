# Decision Records

このディレクトリは stable-which プロジェクトの設計判断を記録する。
命名規約: `DR-NNNN-<slug>.md`（4桁連番）。

| DR | タイトル | Status | 概要 |
|----|----------|--------|------|
| [DR-001](DR-001-workspace-separation.md) | Workspace 分離（ライブラリ + CLI） | Accepted | ライブラリ crate と CLI crate を workspace で分離。ライブラリは std のみ依存で crates.io に publish |
| [DR-002](DR-002-tag-based-evaluation.md) | タグベース評価モデル | Accepted | 候補に PathTag を付与しタグとスコアを分離。ライブラリ利用者が独自の選択ロジックを組める |
| [DR-003](DR-003-stability-by-absence.md) | 安定性は不安定パターンの不在で判定 | Accepted | ホワイトリストではなく不安定パターン（ManagedBy / BuildOutput / Ephemeral / Shim）の不在で安定とみなす |
| [DR-004](DR-004-ephemeral-detection.md) | Ephemeral 判定は case insensitive ワードマッチ | Accepted | `path.parent()` に対して `/\b(cache|tmp|temp|temporary)\b/i` でマッチ。`.app` バンドル内は除外 |
| [DR-005](DR-005-shim-detection.md) | Shim 検出はディレクトリパターン + symlink 名前不一致ヒューリスティック | Accepted | 既知シムディレクトリパターンと symlink 名前不一致の 2 手法を組み合わせる |
| [DR-006](DR-006-explicit-different-binary.md) | DifferentBinary タグを明示的に保持 | Accepted | SameCanonical/SameContent の不在で暗黙判定するのではなく DifferentBinary を明示タグとして付与 |
| [DR-007](DR-007-cli-output-design.md) | CLI 出力設計 | Accepted（一部 DR-012 で改訂） | `--format` / `--all` / `--policy` の直交設計。`-v/--verbose` は DR-012 で `--inspect` にリネーム・廃止 |
| [DR-008](DR-008-path-order-in-tag.md) | PATH 発見順序を InPathEnv タグに持たせる | Accepted | `InPathEnv(usize)` として PATH 上の発見順序をタグに保持。同スコア候補の tie-break に使用 |
| [DR-009](DR-009-byte-comparison.md) | ファイル同一性判定をバイト単位比較に変更 | Accepted | DefaultHasher を廃止し、サイズ比較 → バイト単位ストリーミング比較に変更。依存ゼロを維持 |
| [DR-010](DR-010-executable-check.md) | PATH 候補に実行ビットチェックを追加 | Accepted | PATH 上の候補フィルタリングに Unix 実行ビット (`mode & 0o111`) チェックを追加 |
| [DR-011](DR-011-error-type.md) | 公開 API に専用 Error 型を導入 | Accepted | `Result<_, String>` を `Result<_, Error>` に変更し `#[non_exhaustive]` を付与 |
| [DR-012](DR-012-cli-improvements.md) | CLI 改善（v0.2 レビュー対応） | Accepted | `--verbose` → `--inspect` へリネーム・`-v` 廃止、`--help` は stdout、引数なしは exit 1 |
| [DR-013](DR-013-module-rename.md) | version_manager モジュールを path_analysis にリネーム | Accepted | モジュール名と実態の乖離を解消。crates.io publish 前に実施 |
| [DR-014](DR-014-windows-support.md) | Windows Support | Accepted | Windows をターゲットプラットフォームとしてサポート。`cfg(unix)` / `cfg(windows)` で条件コンパイル |
| [DR-015](DR-015-public-api-stabilization.md) | 0.4.0 に向けた公開 API の安定化方針 | Accepted | Candidate カプセル化（`is_stable()` = durable-to-pin + `durability()` アクセサ）・探索/ランキング/解決の 3 層分離・`score()` 非公開化・空フォールバック削除・path_analysis 非公開化など API 整理 |
| [DR-016](DR-016-durability-model.md) | Durability モデルの導入（durable-to-pin 判定の第一級化） | Accepted | durability を別軸 `Durability { Durable, NotDurable, Unknown }` enum で第一級化。allow-list 方式（肯定条件のみ Durable、Unknown=安全側 not-durable）で焼き込み可否を判定。0.4.0 で軸 freeze、0.5.x で認識精度を非破壊向上 |
