# check-version-bumped の `|| true` を 0.4.0 push 後に精緻化

- Status: open
- Date: 2026-06-13

## 背景

justfile を bump-semver canonical に移行(docs-structure 準拠)した際、`check-version-bumped` gate に移行期間限定の暫定対処を入れた。

## 課題

`check-version-bumped`(justfile)は、`Cargo.toml` / `crates/` に diff があれば `bump-semver compare gt Cargo.toml 'vcs:main@origin:Cargo.toml'` で version が `main@origin` より上かを検証する。しかし現在の `main@origin`(Windows support コミット)は `[workspace.package]` セクションを持たない(`version.workspace` 化は 0.4.0 の変更)ため、compare が exit 2(比較不能)で失敗する。これを `|| true` でスキップしている。

`|| true` は **compare の全失敗をスキップする**ため、本来 gate が止めるべき「version 未 bump(exit 1)」も見逃す副作用がある。現状は `release.yml` 側の version-change gate(`check-version` job)が二重に検証するので実害は限定的。

## 対応(0.4.0 を push した後)

0.4.0 を push すると `main@origin` に `[workspace.package]` が入る。その後:

- `|| true` を外し、exit code で分岐する: **exit 2(比較不能 = 移行初回のみ)はスキップ、exit 1(version 未 bump)は push を block** にする。
- `bump-semver compare --help` で exit code 仕様を確認してから実装する。

## メモ

- 移行初回特有の暫定。0.4.0 リリース後に解消できる。
- それまでも `release.yml` の version gate が二重防御になっているので push の安全性は保たれる。
