# DR-005: Shim 検出はディレクトリパターン + symlink 名前不一致ヒューリスティック

## 決定

2つの検出方法を組み合わせる:
1. 既知シムディレクトリパターン（`/mise/shims/`, `/asdf/shims/` 等）
2. symlink 先のファイル名が候補名で始まらない場合（例: `git` → `jj-worktree`）

prefix match するもの（`python3` → `python3.12`）はバージョン suffix とみなし shim 扱いしない。

## 理由

- mise/asdf の shim は symlink ではなくスクリプト/バイナリなので `read_link` で解決不可
- jj-worktree のような動的 PATH 注入型 shim はディレクトリパターンだけでは捕捉できない
- symlink 名前不一致ヒューリスティックがこのギャップを埋める
