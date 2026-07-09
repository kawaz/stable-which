# DR-016: Durability モデルの導入（durable-to-pin 判定の第一級化）

- Status: Accepted
- Date: 2026-06-13

## Context

現状の `is_stable()`（DR-015 Decision 1）は `PathTag::BuildOutput` / `PathTag::Ephemeral` の不在で「安定」と判定する。しかしこの定義では、downstream の cache-warden が必要とする「**そのパスを launchd plist / systemd unit に焼き込んで、reboot / upgrade を跨いで生き続けるか**」（= durable-to-pin）を正しく表現できない。根本問題は 2 点ある。

1. **`PathTag::ManagedBy(name)` が version manager 名しか持たず、durability を区別しない**。同じ「version manager 管理下」でも以下は durability が全く異なる:
   - **versioned-install**（`/opt/homebrew/Cellar/git/2.44.0/...`、`/nix/store/<hash>/...`、`~/.local/share/mise/installs/node/20/...`）: upgrade でバージョンディレクトリごと消える → **焼き込むと壊れる**
   - **durable-reference**（`~/.nix-profile/bin/...`、shim ディレクトリ、`/opt/homebrew/bin/` の symlink）: パス文字列は upgrade を跨いで不変 → **焼き込んでよい**
   - **project-local**（`.direnv/`、`venv/`）: プロジェクトに scoped、グローバルサービスから参照すべきでない

2. **実機調査（[2026-06-13 binary-path-durability-matrix](../findings/2026-06-13-binary-path-durability-matrix.md)）で「参照パス durable / realpath versioned」の 2 段構造が確認された**。多くの環境では「durable な参照パス（symlink / profile）」がその先で「versioned な実体（realpath）」を指す。さらに環境依存性（`~/.nix-profile` が壊れている環境がある等）と、現 `path_analysis`（`crates/stable-which/src/path_analysis.rs:15-50`）が取りこぼす多様な manager パターンが判明し、**パス文字列ベースでの完全網羅は構造的に不可能**と実証された。

`PathTag`（`crates/stable-which/src/candidate.rs:53-80`）は既に provenance（`Input` / `InPathEnv`）・identity（`SameCanonical` / `SameContent` / `DifferentBinary`）・validity（`NotExecutable`）・durability（`BuildOutput` / `Ephemeral` / `ManagedBy`）の軸が混在しており、ここに durability タグを足すと「何が排他で何が併存可能か」が読めない API になる。

## Decision

### 1. Durability を別軸 enum で第一級化する

`PathTag` に durability タグを追加するのではなく、durability を直交軸として独立した enum に分離する。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Durability {
    /// パス文字列が rebuild / upgrade / reboot を跨いで同じ実体（または同じ論理ターゲット）を指し続ける
    Durable,
    /// versioned-install / ephemeral / build-output / project-local 等、焼き込むと壊れうる
    NotDurable,
    /// 既知の durable / not-durable いずれのパターンにもマッチせず判定不能（安全側 = not-durable 扱い）
    Unknown,
}
```

- `Candidate::durability() -> Durability` を公開アクセサとして提供する。
- `Candidate::is_stable() -> bool` は `matches!(self.durability(), Durability::Durable)` の convenience とする（DR-015 Decision 1）。
- `PathTag`（`ManagedBy(String)` を含む）は **観測事実・診断用** として従来どおり残す。`ManagedBy` は「どの manager 配下か」という観測タグであり続け、durability の判定材料の 1 つとして使われる。

- **理由**: `PathTag` は既に複数軸が混在しており（`crates/stable-which/src/candidate.rs:53-80`）、durability タグを足すと排他/併存の関係が読めなくなる。durability は本質的に「パスを焼き込めるか」という単一の直交軸なので、専用 enum に分離するのが正しい責務分割である。`#[non_exhaustive]` により将来 variant を非破壊で追加できる。
- **影響**: cache-warden は `is_stable()`（= `Durable` 判定）を register gate に使える。`ManagedBy` を読んで自前判定していた箇所を廃止できる。

### 2. Scope（project-local 判定）は内部に留め公開しない

project-local（`.direnv/`、`venv/` 等）の検出には内部 enum を用いるが、公開 API には出さない。

- **理由**: cache-warden は per-user / per-system のグローバルサービス登録にしか使わず、scope の区別を必要としない（YAGNI）。project-local は「`NotDurable` の一種」として `durability()` に畳み込めば十分。将来 scope を公開する必要が出たら、`Durability` enum と同様に `#[non_exhaustive]` で非破壊に公開追加できる。
- **影響**: 公開 API 面は `Durability` の 3 variant のみ。内部判定の粒度は外部契約から隠れる。

### 3. allow-list 方式（肯定条件のみ Durable と判定する）

durability の判定は **肯定列挙（allow-list）+ 安全側フォールバック** で行う。判定順序:

1. **NotDurable を先に判定**: versioned-install | ephemeral | build-output | project-local のいずれかのパターンにマッチ → `NotDurable`。
2. **既知の durable location にマッチ → `Durable`**（**「環境全体の参照面」に限定する**）:
   - 標準 system bin: `/usr/bin`、`/bin`、`/usr/local/bin`
   - shim ディレクトリ（`crates/stable-which/src/path_analysis.rs:39-50` の `SHIM_PATTERNS`）
   - profile 参照パス: `/opt/homebrew/bin`、`/run/current-system/sw/bin`、`/etc/profiles/per-user/<user>/bin`
3. **どちらにもマッチしなければ `Unknown`** → `is_stable()` は `false`。

**`~/bin` / `~/.local/bin` のような user dropbox 的な場所は durable location allow-list に含めない**。これらは個別ツールが versioned symlink（実機例: `claude` → `versions/<ver>`、`cursor-agent` → `versions/<ver>`）や shebang 埋め込みスクリプト（実機例: `jupyter` の内部 `cpython@<ver>` 参照）を混在させるため、ディレクトリ単位で無条件 `Durable` と判定すると false positive（壊れるパスを焼き込む）を生む。これらは `Unknown`（= 安全側 not-durable）に倒す。allow-list に入れてよいのは「環境全体の参照面」（system bin・profile・shim・homebrew bin 等、個別ツールが versioned 実体を直接置かない面）に限る。

- **理由**: 消極条件の否定（「versioned でなければ durable」）にすると、未知 manager の versioned パターンを見落としたときに **false positive（壊れるパスを durable と誤判定して焼き込む）** になる。これは最も避けたい failure mode である。肯定列挙 + `Unknown = 安全側 not-durable` にすることで、false positive を構造的に防ぐ。標準 system bin・profile・homebrew bin 等の「環境全体の参照面」を allow に含めるのは、過剰警告（durable なのに `Unknown` 扱いになる）を抑えるため。
- **原則: 検出不能な limitation が存在する場所は allow-list に入れない**。`~/.local/bin` 等の user dropbox は shebang 埋め込み・versioned symlink が混在し（Decision 6）、パス文字列だけでは durable / not-durable を判別できない。判別不能な場所を allow に入れると false positive 回避の思想（本 Decision）が崩れるため、`Unknown` に倒す。これにより user dropbox 上の真に durable な自前ツールも警告対象になる（過剰警告）が、false positive 回避を優先する。
- **影響**: 未知環境では保守的に `Unknown`（= not-durable）に倒れる。cache-warden は「焼き込めない」と判断して安全側に振る。網羅性は 0.5.x で `Unknown → Durable` 方向に精緻化していく（Decision 5）。

### 4. 判定は「原則」+「manager パターン」の二層構造

durability 判定を 2 つのレイヤに分ける。

- **原則レイヤ**（manager パターン不要）: build output / temp / relative / project-local といった「危険側」は、パスの構造的特徴だけで `NotDurable` と判定できる。
- **manager パターンレイヤ**: durable reference と versioned install の区別、および shim の同定は、manager 固有のパターン（`INSTALL_PATTERNS` / `SHIM_PATTERNS`）を必要とする。

durability は **候補ごとに評価する**。同一の binary でも、参照パス `/opt/homebrew/bin/git`（= `Durable`）と canonical な `/opt/homebrew/Cellar/git/2.44.0/bin/git`（= `NotDurable`）が別々の `Candidate` として並ぶ。daemon は `Durable` な候補を選んで焼き込める。

- **理由**: 「参照パス durable / realpath versioned」の 2 段構造（findings 参照）を、候補ごとの `durability()` で自然に表現できる。原則レイヤと manager パターンレイヤを分けることで、後者の網羅性向上（Decision 5）が前者の安定性を壊さない。
- **影響**: `rank_candidates`（DR-015 Decision 2）と組み合わせると、cache-warden は「`Durable` かつ高スコアの候補」を選択できる。

### 5. 網羅性の線引き（0.4.0 で軸 freeze、0.5.x で精緻化）

- **0.4.0**: 「durability 軸の導入 + 保守的フォールバック（`Unknown = not-durable`）」を API として freeze する。`Durability` enum・`durability()`・`is_stable()` の形状と意味（DR-015 Decision 1）はここで確定する。
- **0.5.x 以降**: 個別 manager パターンの網羅・精度向上を継続する。`Unknown → Durable` への精緻化は、判定が「より多くの durable パスを認識する」だけなので **API 契約を変えない非破壊変更** である。

実機調査で判明した取りこぼし（`/run/current-system`、`/etc/profiles/per-user`、rye の `cpython@<ver>`、claude / cursor の `versions/`、plugin cache の `/<ver>/bin` 等）のうち、0.4.0 で安全に入れられる分は入れ、残りは 0.5.x で追加する。

- **理由**: 完全網羅は構造的に不可能（findings で実証）なので、「軸と契約を先に freeze し、認識精度は非破壊で育てる」方針が最も安全。`Unknown` を安全側に置いてあるので、未対応 manager があっても誤判定（false positive）にはならず、単に「焼き込まない」に倒れるだけ。
- **影響**: 0.4.0 のユーザは未対応 manager 配下で `Unknown` を受け取る。0.5.x で認識が増えても破壊的変更にならない。

### 6. 既知の限界を doc に明記する

パス文字列ベース判定の限界を rustdoc に明記する。

- **shebang 埋め込み型**: rye の `jupyter` が内部で `cpython@<ver>` を shebang で参照する等、実行系が別の versioned パスを内部参照するケースは、パス文字列だけでは検出不能。このため、shebang 型が混在しうる user dropbox（`~/.local/bin` 等）は Decision 3 で durable allow-list から外し `Unknown` に倒す。標準参照面（system bin・profile・homebrew bin 等）に shebang 型が置かれる稀なケースは検出不能な残存 limitation として doc に明記する（**その場合 durable と判定されても実行が壊れうる**）。
- **環境依存パス**: `~/.nix-profile` 等は環境によって生死が異なり、realpath を見ないと durability が確定しない。

これらは「パス文字列ベース判定の限界」として doc に明記し、判定不能なケースは `Unknown` / `NotDurable` 側に倒すことで安全を確保する。

- **理由**: 限界を doc 化しておくことで、利用者が「durable と出たのに壊れた」ケースの原因を理解でき、過信を防げる（`document-design-rationale` の趣旨）。
- **影響**: cache-warden は doc の限界を踏まえ、必要なら realpath 検証等の追加チェックを上位で実装できる。

### 追記 (2026-07-09, v0.5.0): HOME-anchored direct-install 面の昇格

Decision 5 の精緻化路線に沿った最初の `Unknown → Durable` 昇格として、
**`~/.cargo/bin` / `~/go/bin` / `~/.moon/bin`** を durable allow-list に追加した
(`HOME_ANCHORED_DIRECT_DIRS`)。

- **根拠**: findings fact 7 — 3 面とも「installer (cargo install / go install /
  moon upgrade) が同一パスへ in-place 上書きし、versioned tree を作らない」という
  同一の性質を実機確認済み。同一根拠の面を一部だけ昇格すると「意図的後回しか
  見落としか」が判別不能になるため 3 面同時に昇格する
- **Decision 3 との整合**: user dropbox (`~/bin` / `~/.local/bin`) の除外理由は
  「不特定ツールの versioned symlink / shebang スクリプトの混在」。これら 3 面は
  単一の well-known installer が支配する専用面であり除外理由に該当しない。
  マッチは shim と同じ HOME アンカー + 直下 1 セグメントの厳密構造
- **残存リスク**: user-writable な PATH 上ディレクトリなので、第三者ツールや手動
  `ln -s` が versioned symlink を置く可能性は排除できない (`/usr/local/bin` と
  同クラスの既知 limitation)。コード側 doc comment に明記
- **スコープ外**: Windows-native 面 (scoop shims 等) は引き続き 0.5.x 以降の
  別 issue。環境依存面 (`~/.nix-profile` 等) は判別不能のため `Unknown` のまま

## Consequences

- `is_stable()` が「durable-to-pin」の正しい意味になり、cache-warden の versioned-managed 見落としバグ（versioned-install を durable と誤認していた）が同時に埋まる。
- 公開 API は `durability()` + `is_stable()` + `tags()` + `path()` + `canonical()` に整理される。Scope は内部に留める。
- 0.4.0 で durability 軸を freeze、0.5.x で認識精度を非破壊に向上させる運用線が確定する。

## 不採用案

- **`PathTag` に `VersionedInstall` / `DurableManaged` タグを追加（codex 案 A）**: 既に複数軸が混在する `PathTag`（`crates/stable-which/src/candidate.rs:53-80`）に durability 軸を足すと、排他/併存関係が読めず API 可読性が下がる。別軸 enum（Decision 1）を選択した。
- **Scope を公開 enum にする（codex 完全案）**: cache-warden は scope を必要とせず YAGNI。内部に留めた（Decision 2）。将来必要になれば `#[non_exhaustive]` で非破壊に公開できる。
- **デフォルト durable（消極否定: 「versioned でなければ durable」）**: 未知 manager の versioned パターンを false positive で durable 判定し、壊れるパスを焼き込むリスクがある。allow-list 方式（Decision 3）を選択した。
- **`is_stable` を `Stability` / `PathStability` の多値 enum にする（以前の codex 案）**: durability は本質的に Durable / NotDurable / Unknown の 3 値で、cache-warden の register gate は `bool` で足りる。多値の「選好ティア」は score 側の責務（別概念。DR-015 Decision 3 の `preference_tier`）であり、durability に混ぜない。

## 関連 DR

- [DR-002](DR-002-tag-based-evaluation.md): タグベース評価モデル。本 DR は `PathTag`（`ManagedBy` を含む）を**観測タグ**として残す方針を継承し、durability を別軸 enum に分離する。
- [DR-013](DR-013-module-rename.md): `path_analysis` モジュール。本 DR の Decision 3/4 の判定ロジックは `path_analysis` の `INSTALL_PATTERNS` / `SHIM_PATTERNS`（`crates/stable-which/src/path_analysis.rs:15-50`）を内部で参照する。
- [DR-015](DR-015-public-api-stabilization.md): 公開 API 安定化。`is_stable()` / `durability()` の API 形状（カプセル化・アクセサ）はこちらで定義し、本 DR はその「中身（durable-to-pin の意味）」を定義する。
- [DR-004](DR-004-ephemeral-detection.md): ephemeral 判定。本 DR の `NotDurable` 判定の構成要素の 1 つ（Decision 3-1）。
- [DR-005](DR-005-shim-detection.md): shim 判定。shim ディレクトリは本 DR の durable location allow-list（Decision 3-2）に含まれる。

findings: [docs/findings/2026-06-13-binary-path-durability-matrix.md](../findings/2026-06-13-binary-path-durability-matrix.md) — 実機調査マトリクス（「参照パス durable / realpath versioned」の 2 段構造、環境依存性、完全網羅が構造的に不可能であることの実証）。
