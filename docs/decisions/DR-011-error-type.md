# DR-011: 公開 API に専用 Error 型を導入

## 決定

`Result<_, String>` を `Result<_, Error>` に変更。`#[non_exhaustive]` を付与。

## 理由

- `String` は `match` できず、利用者がエラー種別（not found vs not a file vs not in PATH）でハンドリングできない
- crates.io publish 後のエラーバリアント追加が `#[non_exhaustive]` で非破壊的に可能
- `impl std::error::Error` により `?` 演算子でのエラーチェーンが使える

## 不採用案

- `thiserror` crate: 依存ゼロ方針に反する
