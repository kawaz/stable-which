# DR-001: Workspace 分離（ライブラリ + CLI）

## 決定

`stable-which`（ライブラリ crate）と `stable-which-cli`（CLI crate）を workspace で分離する。

- ライブラリ: 依存なし（std のみ）。crates.io に publish
- CLI: serde, serde_json に依存。`publish = false`（Homebrew / GitHub Releases で配布）

## 理由

ライブラリ利用者に孫依存を強制しない。diesel + diesel_cli, sqlx + sqlx-cli と同じ Rust エコシステムの王道パターン。

## 不採用案

単一クレート + features: `--no-default-features` をライブラリ利用者に要求するのは負担。
