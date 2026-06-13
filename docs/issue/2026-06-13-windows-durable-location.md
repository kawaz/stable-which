# Windows durable location 対応 (0.5.x)

Status: open
Date: 2026-06-13

## 概要

`durability.rs` の durable location allow-list (`DURABLE_DIRECT_DIRS` および
`is_etc_profiles_per_user_bin`) はすべて Unix 絶対パス形式 (`/usr/bin`, `/opt/homebrew/bin` 等)。
Windows では `Path::is_absolute()` がドライブレターを要求するため、これらの Unix パスに対して
`false` を返す。結果として `is_durable_direct_dir()` / `is_etc_profiles_per_user_bin()` は
Windows で常に `false` を返し、durable location にマッチするパスが存在しない。

Windows で NotDurable パターンに合致しないパスはすべて `Unknown` (安全側) に落ちる。
現時点ではこれが意図された挙動 (0.4.x の Windows サポート範囲)。

## 0.5.x での対応予定

Windows-native な durable surfaces を allow-list に追加する:

- `C:\Windows\System32` / `C:\Windows\SysWOW64`
- scoop global/local bin (`C:\Users\<user>\scoop\shims`, `C:\ProgramData\scoop\shims`)
- chocolatey bin (`C:\ProgramData\chocolatey\bin`)
- winget managed 系は versioned install が多く慎重判断が必要
- PowerShell profile 相当の shims
- パス区切り文字の正規化 (`norm()` の `\\` → `/` 変換は実装済み)

### 調査ポイント

1. Windows で `Path::is_absolute()` が `true` になるパス形式 (UNC パス含む)
2. 各 durable surface の実際の構造 (Windows CI 環境での実機確認が必要)
3. `is_durable_direct_dir` の `path.is_absolute()` チェックを
   `#[cfg(windows)]` ブロックで補完するか、`norm()` ベースの文字列比較に切り替えるか

## 現状の対処 (0.4.x)

CI Windows テストで失敗していた「Unix 絶対パス + Durable 期待」のテストを
`#[cfg(unix)]` でガードした。Windows では durable location の判定が効かず
`Unknown` になることを `Durability` の doc コメント ("Platform support" セクション)
および `judge()` の doc コメントに明記済み。
