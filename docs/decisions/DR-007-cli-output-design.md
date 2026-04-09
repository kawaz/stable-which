# DR-007: CLI 出力設計

## 決定

- `--format <path|json>`: 出力形式（デフォルト: path）
- `--all`: 全候補表示（デフォルト: 最良候補のみ）
- `--policy <same-binary|stable>`: スコアリングポリシー
- `-v, --verbose`: `--all --format json` のショートハンド

## 理由

`--format` と `--all` を直交させることで、4つの組み合わせが自然に生まれる。デフォルトが path 1行でスクリプト親和性が高い（`$(stable-which foo)` で使える）。

## 不採用案

- `--json` と `--path-only` の排他フラグ: 組み合わせルールが複雑になる
- `--candidates`: 内部実装寄りの命名。`--all` の方が直感的
