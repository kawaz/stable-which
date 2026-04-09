# DR-012: CLI 改善（v0.2 レビュー対応）

## 決定

1. `--verbose` → `--inspect` に名前変更。`-v` ショートオプション廃止
2. `--help` 明示時は stdout、エラー時は stderr
3. 引数なし実行は exit 1（`--help` 明示は exit 0）
4. lib.rs にルート re-export 追加

## 理由

1. `--verbose` が出力フォーマットを変えるのは CLI 慣習に反する（ripgrep, fd, curl 等と乖離）
2. `--help | less` や `--help > file` が空になる問題の解消
3. スクリプトから引数忘れを検出可能に
4. `stable_which::candidate::find_candidates` は深すぎる。`stable_which::find_candidates` で使えるべき
