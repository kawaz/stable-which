# stable-which

> [English](./README.md) | 日本語

[![crates.io](https://img.shields.io/crates/v/stable-which.svg)](https://crates.io/crates/stable-which)
[![docs.rs](https://docs.rs/stable-which/badge.svg)](https://docs.rs/stable-which)
[![CI](https://github.com/kawaz/stable-which/actions/workflows/ci.yml/badge.svg)](https://github.com/kawaz/stable-which/actions/workflows/ci.yml)

バイナリパスの安定性を評価し、PATH 上の安定した候補を探すツール/ライブラリ。

## 問題

パッケージマネージャやバージョンマネージャは、バイナリをバージョン付きまたは揮発的なディレクトリに配置します:

```
/opt/homebrew/Cellar/jj/0.24.0/bin/jj          # Homebrew Cellar（バージョン固有）
~/.local/share/mise/installs/node/22.0.0/bin/node  # mise installs
./target/release/myapp                           # Cargo ビルド出力
```

`brew upgrade` 後、Cellar パスは壊れます。mise がバージョンを切り替えると installs パスが変わります。ビルド出力はリビルドのたびに変わります。一方、PATH 上には安定したシムリンクやシムが存在します:

```
/opt/homebrew/bin/jj -> ../Cellar/jj/0.24.0/bin/jj
~/.local/share/mise/shims/node
```

`which` はコマンド名でバイナリを探しますが、その結果が同じバイナリを指すかを**検証しません**。複数バージョンが共存している場合、`which` は全く別のバイナリを返す可能性があります。

`stable-which` は PATH 上の同名候補をすべて列挙し、各候補に安定性属性タグを付与し、スコアリングにより「同じファイルを指す最も安定したパス」を探します。

## 動作原理

1. 入力バイナリパスを正規化（canonicalize）
2. PATH 上の同名バイナリをすべて探索
3. 各候補にタグ付与（SameCanonical, InPathEnv, ManagedBy, BuildOutput, Ephemeral 等）
4. 各候補の **durability**（サービス定義に焼き込み可能か）を判定
5. 選択したポリシーで候補をランク付け
6. 最良候補を返す（`--all` で全候補表示も可能）

## 使い方

### CLI

```bash
# 最良の安定パス（デフォルト: path 形式）
stable-which /opt/homebrew/Cellar/jj/0.24.0/bin/jj
# /opt/homebrew/bin/jj

# コマンド名で探索
stable-which jj
# /opt/homebrew/bin/jj

# 全候補を JSON で表示
stable-which --all --format json jj

# 全候補を JSON で検査（--all --format json のショートハンド）
stable-which --inspect jj

# バイナリ同一性よりパス安定度を優先
stable-which --policy stable ./target/release/myapp
```

### ライブラリ

依存関係を追加:

```bash
cargo add stable-which
```

```rust
// この例は crates/stable-which/src/lib.rs のクレートルート doc example と同期を保つこと
// （doctest で検証済みの canonical バージョン）。
use stable_which::{find_candidates, rank_candidates, Durability, ScoringPolicy};
use std::path::Path;

// 1. 候補を探索（探索のみ、決定的な PATH 順）
let mut candidates = find_candidates(Path::new("jj"))?;
// 2. スコアリングポリシーで in-place ランク付け
rank_candidates(&mut candidates, ScoringPolicy::SameBinary);
// 3. アクセサ経由で検査（フィールドは private）
for c in &candidates {
    println!("{}: {:?} durable={}", c.path().display(), c.tags(), c.is_stable());
}
let best = &candidates[0];
if best.durability() == Durability::Durable {
    println!("safe to pin: {}", best.path().display());
}
```

`resolve_stable_path(binary, policy)` は `find_candidates` + `rank_candidates` を合成し、最良の単一候補を返すコンビニエンス関数です。

## CLI オプション

```
stable-which [OPTIONS] <binary>

引数:
    <binary>         バイナリへのパス、または PATH から探すコマンド名

オプション:
    --all            全候補を表示（デフォルト: 最良候補のみ）
    --format <F>     出力形式: path（デフォルト）、json
    --policy <P>     スコアリングポリシー: same-binary（デフォルト）、stable
    --inspect        全候補を JSON で表示（--all --format json と同じ）
    -q, --quiet      警告を抑制
    --help           ヘルプを表示
    --version        バージョンを表示
```

## スコアリングポリシー

| ポリシー | 優先度 | ユースケース |
|---|---|---|
| same-binary（デフォルト） | バイナリ同一性 > パス安定度 | サービス登録 |
| stable | パス安定度 > バイナリ同一性 | アップグレードを跨いで使う設定ファイル |

## パスタグ

各候補パスの属性を表すタグ:

**Positive（緑）:** Input, InPathEnv, SymlinkTo, SameCanonical, SameContent

**Warning（オレンジ）:** ManagedBy, Shim, BuildOutput, Ephemeral, Relative, NonNormalized

**Negative（赤）:** DifferentBinary

## Durability（焼き込み耐久性）

各候補はタグとは直交する **durability** 軸（`durable` / `not-durable` / `unknown`）でも評価されます。`Candidate::durability()` と便利メソッド `is_stable()`（`durable` の場合のみ `true`）で参照できます。これは「このパスを launchd plist / systemd unit に焼き込んでアップグレードや再起動を跨いで生き続けるか？」に答えます:

- **durable**: 環境全体の参照面（`/usr/bin`、`/opt/homebrew/bin`、プロファイル bin、標準シムディレクトリ）
- **not-durable**: バージョン付きインストール（`Cellar/`、`nix/store/`、`installs/`）、一時パス / ビルド出力 / プロジェクトローカル
- **unknown**: 認識できない場所やユーザードロップボックス（`~/bin`、`~/.local/bin`）— ピン留め不可として扱う（安全側）

Durability は候補ごとに判定するため、参照パス `/opt/homebrew/bin/git` は `durable`、その canonical realpath `/opt/homebrew/Cellar/git/2.44.0/bin/git` は `not-durable` となります。JSON 出力（`--inspect`）には各候補に `durability` フィールドが含まれます。

## インストール

```bash
brew install kawaz/tap/stable-which
```

またはソースからビルド:

```bash
cargo build --release -p stable-which-cli
```

## ライセンス

MIT
