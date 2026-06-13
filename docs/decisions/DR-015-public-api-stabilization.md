# DR-015: 0.4.0 に向けた公開 API の安定化方針

- Status: Accepted
- Date: 2026-06-13

## Context

`stable-which` は既に crates.io に公開済み（v0.3.3）。0.x のうちに公開 API を 1.0 に向けて締める。現状の公開 API には以下の歪みがある。

1. **実装詳細リーク**: `Candidate` の全フィールドが `pub`（`crates/stable-which/src/candidate.rs:82-87`）。`Vec<PathTag>` という内部表現がそのまま外部契約になっている。
2. **探索とランキングの混線**: `find_candidates` の `policy` 引数が「候補の探索」と「順位付け」を混ぜている。実際には `policy` はソート順の決定にしか使われていない（`crates/stable-which/src/candidate.rs:483`）。
3. **内部ヘルパーの API 契約化**: `path_analysis` モジュール全体が `pub`（`crates/stable-which/src/lib.rs:19`）で、`detect_version_manager` / `is_shim_path` / `is_ephemeral` / `is_executable` / `files_have_same_content` 等の内部ヘルパーが公開 API になっている。

downstream consumer は 2 つ:

- **cache-warden**: `resolve_stable_path` のみ使用。`Candidate` は読むだけで構築しない。
- **公式 CLI（stable-which-cli）**: `find_candidates` / `Candidate::score` / `Candidate` のフィールドを使用（`crates/stable-which-cli/src/main.rs:2-4,25-28,176-184`）。

両者への影響を踏まえて公開面を整理する。

## Decision

### 1. Candidate のカプセル化

`path` / `canonical` / `tags` フィールドを private 化し、アクセサを提供する。

- `path() -> &Path`
- `canonical() -> &Path`
- `tags() -> &[PathTag]`
- 耐久性アクセサ `durability() -> Durability` を追加
- 安定判定述語 `is_stable() -> bool` を追加

`is_stable()` の定義: `matches!(self.durability(), Durability::Durable)`（= durable-to-pin。rebuild / upgrade / reboot を跨いで同じパスを指し続け、サービス定義に焼き込んでよい）。durability モデルの詳細（`Durability` enum、judging 順序、allow-list 方式、既知の限界）は [DR-016](DR-016-durability-model.md) を参照。`NotExecutable`（実行可否）や `DifferentBinary`（同一性）は別軸なので `is_stable` には含めない。

- **理由**: `Vec<PathTag>`（`crates/stable-which/src/candidate.rs:86`）という内部表現の露出を隠し、将来 `tags` の保持形式を変える自由を確保する。cache-warden の要望（tag ハードコード脱却）と独立 API 監査の指摘（実装詳細リーク）が一致した。`is_stable()` を「`BuildOutput`/`Ephemeral` 不在」から `durability() == Durable` に置き換えるのは、versioned-install（upgrade で壊れる）を安定とみなしていた誤りを正すため（DR-016 Context）。
- **影響**: cache-warden は `Candidate` を読むだけ（構築しない）なので、`path()` / `tags()` / `durability()` / `is_stable()` への書き換えのみ（daemon_cmd.rs 1 ファイル、0.4.0 移行案内で対応）。CLI は同 workspace で一緒に修正する。

### 2. 探索 / ランキング / 解決の 3 層分離

`policy` は本来「候補集合」ではなく「順位付け」の関心事である（現 `find_candidates` は `policy` をソート順にしか使っていない: `crates/stable-which/src/candidate.rs:483`）。探索と順位付けを分離する。

- `find_candidates(binary) -> Result<Vec<Candidate>, Error>`: 探索のみ。`policy` 引数を**削除**。決定的順序（PATH discovery 順）で返す。
- `find_candidates_with_path_env(binary, path_env: Option<OsString>) -> Result<Vec<Candidate>, Error>`: 同上、`policy` 引数を削除（改名は Decision 7）。`path_env` が `None` の意味を doc に明記する（Decision 7）。
- `rank_candidates(candidates: &mut [Candidate], policy: ScoringPolicy)`: **新規**。`policy` でスコア降順 + PATH 順タイブレークで **in-place ソート**する（現 `find_candidates_with_env` 内のソートロジック `crates/stable-which/src/candidate.rs:482-485` を切り出す）。
- `resolve_stable_path(binary, policy) -> Result<Candidate, Error>`: **シグネチャ維持**（cache-warden が依存）。内部実装は `find_candidates` + `rank_candidates` の合成に変わる（現 `crates/stable-which/src/candidate.rs:494-506`）。

- **理由**: `policy` は順位付けの関心事であり、探索と順位付けを分離するのが正しい責務分割である。`rank_candidates` を `Vec<Candidate>` 消費 → `Vec<Candidate>` 返却にすると、所有権の受け渡し契約を不要に固定する。`&mut [Candidate]` で受けて in-place ソートするのが Rust の慣習（`slice::sort_by` 等）であり、呼び出し側が `Vec` を保持したままソートできる。
- **影響**: cache-warden は `find_candidates` 未使用なので無影響。CLI の `--all`（`crates/stable-which-cli/src/main.rs:176-177`）は `find_candidates(p)` で取得 → `rank_candidates(&mut candidates, policy)` で in-place ソートに書き換える。

### 3. `score()` を `pub(crate)` 化し、公開順位付けは `rank_candidates` に集約

`Candidate::score`（`crates/stable-which/src/candidate.rs:98`）を **`pub(crate)` に変更し、公開 API から除外する**。公開面で「順位」を表現する手段は `rank_candidates`（Decision 2）に一本化する。

- CLI の JSON 出力（`crates/stable-which-cli/src/main.rs:28`）の `score` フィールドは、生スコア値ではなく **rank 順の index（0 始まりの順位）** に変更する。
- `compare_candidates` のような 2 候補比較 API は作らない（`rank_candidates` で全体を並べれば足りる。YAGNI）。
- あわせて、`score()` 内部の `stability_score`（`crates/stable-which/src/candidate.rs:107-118`）を **`preference_tier`（等）に改名**し、durability（DR-016）との語義衝突を解消する。この内部スコア項は「焼き込み可否（durability）」ではなく「同スコア帯内での選好ティア」であり、別概念である。

- **理由**: スコアの絶対値は `ScoringPolicy` 依存で意味が変わるため、`score()` を公開して `Ord` 的に使わせるのは不適切（policy が変われば順序も変わる）。順位は「`rank_candidates` でソート済みの並び順」として表現するのが正しい。`stability_score` という名前は durability 軸（DR-016）と紛らわしいので、選好ティアを表す名前に直す。
- **影響**: CLI は JSON の `score` を rank index に切り替え（`crates/stable-which-cli/src/main.rs:28`）。**これは JSON `score` フィールドの意味変更を伴う 0.4.0 の破壊的 CLI 出力変更である**（生スコア値 → 0 始まりの順位。DR-007 の `--format json` 契約変更にあたるため、CHANGELOG / リリースノートで明示する）。`score()` の外部公開がなくなるので、将来の重み変更が破壊的変更にならない。

### 4. 空フォールバック（候補ゼロ時の tagless Candidate 捏造）を削除する

`resolve_stable_path` の候補ゼロ時フォールバック（`crates/stable-which/src/candidate.rs:494-506` の `else` 枝で `tags: vec![]` の `Candidate` を構築する箇所）を **削除する**。

これは実質 dead code である。`find_candidates`（`find_candidates_with_path_env`）は **成功時に必ず input candidate を 1 件は返す**（raw input を無条件で push する: `crates/stable-which/src/candidate.rs:349-360`）。候補が空になるのは `Err` を返すパス（`NotFound` / `NotAFile` / `NotInPath` 等）だけで、その場合 `resolve_stable_path` は `?` で早期 return する。したがって `else` 枝には到達しない。

- 削除と同時に、**「`find_candidates` は成功時に候補が非空」を不変条件として doc に明記**する。
- **理由**:
  - tagless な `Candidate` を捏造するのは DR-002 のタグベースモデル（候補は必ず観測タグを持つ）に反する。
  - tagless candidate は durability 判定で `Durable` 側に倒れうる（NotDurable パターンにも durable allow-list にもマッチせず…という以前に、判定材料のタグが無い）ため、`is_stable() = true` の誤判定（DR-016 の false positive failure mode）を生む。dead code とはいえ放置すると将来の事故源になる。
- **影響**: 公開 API のシグネチャは不変（到達しない枝の削除）。不変条件が doc 化され、cache-warden は「`resolve_stable_path` が `Ok` を返したら必ずタグ付き候補」と仮定できる。

### 5. path_analysis モジュールと VersionManagerInfo を非公開化

- `pub mod path_analysis`（`crates/stable-which/src/lib.rs:19`）→ `pub(crate) mod path_analysis`（または `mod`）。
- `pub use path_analysis::VersionManagerInfo`（`crates/stable-which/src/lib.rs:26`）を削除。

`detect_version_manager` / `is_shim_path` / `is_ephemeral` / `is_executable` / `files_have_same_content` 等（`crates/stable-which/src/path_analysis.rs:75-174`）は内部ヘルパーであり、公開 API 契約にすべきでない。version manager 名は `PathTag::ManagedBy(String)`（`crates/stable-which/src/candidate.rs:73`）経由で取得できるので `VersionManagerInfo`（`crates/stable-which/src/path_analysis.rs:10-13`）の外部公開は不要。

- **影響**: cache-warden・CLI とも `path_analysis` 未使用なので無影響。

### 6. モジュール構造を実装詳細化

`pub mod candidate` / `pub mod path_analysis`（`crates/stable-which/src/lib.rs:18-19`）をやめ、`mod candidate` + 既存の `pub use candidate::{...}`（`crates/stable-which/src/lib.rs:22-25`）re-export のみを公開面にする。型・関数はクレートルート（`stable_which::Candidate` 等）からのみ参照可能にする。

- **影響**: CLI が `use stable_which::candidate::{...}`（`crates/stable-which-cli/src/main.rs:2-4`）とモジュール直パスで参照しているので、`use stable_which::{...}` に書き換える。

### 7. `find_candidates_with_env` を改名し、env 経路の意味と契約を doc 化

`find_candidates_with_env` を **`find_candidates_with_path_env` に改名**する。

- **改名理由**: この関数は `path_env`（= PATH 環境変数相当）だけを引数で受けるが、Windows の PATHEXT は引数ではなく `std::env::var` でプロセス環境から直読みする（`crates/stable-which/src/candidate.rs:306,448`）。`with_env` という名前は「env 全体を注入する」かのように読めて不正確で、実態（PATH 相当のみ受ける）と乖離している。`with_path_env` にすることで「受けるのは PATH 系」という契約が名前から明確になる。
- **PATHEXT 直読みは 0.4.0 では維持**: 完全な env 注入（PATHEXT も引数化）は将来課題とし、0.4.0 では「現状は PATHEXT をプロセス環境から読む」と doc に明記するに留める（YAGNI、過剰設計回避）。Design rationale コメントをコードに残す。
- **`path_env = None` の意味を doc に明記**: `None` は「PATH 探索をスキップ」ではない。コマンド名（パス区切りを含まない入力）を渡したまま `path_env = None` だと PATH 探索に失敗して **`Error::NotInPath` を返す**（`crates/stable-which/src/candidate.rs:286-323` の解決ロジック）。明示パス入力なら `None` でも input 由来候補は返る。この差を doc に明記する。
- **`tags()` の順序は非保証**: `Input` / `InPathEnv` は先頭に `insert(0, ...)` される（`crates/stable-which/src/candidate.rs:353,470`）が、それ以外のタグは実装順で並ぶ。利用者は順序に依存せず `contains` / `iter().any(...)` で判定すべきことを doc に明記する。
- **`canonical()` の契約を明記**: `canonicalize` に失敗した場合は入力パスにフォールバックする（`crates/stable-which/src/candidate.rs:238`）。`canonical()` が返すパスは「canonicalize 成功時は実体パス、失敗時は入力パスそのまま」であり、常に realpath とは限らないことを doc に明記する。

- **影響**: CLI / cache-warden とも `find_candidates`（PATH を自動取得する薄いラッパ）を使うため、改名の直接影響は受けない。`find_candidates_with_path_env` を直接呼ぶ利用者（テスト含む）は呼び出し名を更新する。

## Consequences

- 0.4.0 は破壊的リリースになる。downstream（cache-warden）への影響は 1 ファイル軽微で、移行案内を 0.4.0 リリース時に送る。
- 公開 API が「find（探索） / rank（順位） / resolve（解決）」+ カプセル化された `Candidate` + `durability()` / `is_stable()` に整理され、1.0 に向けて安定化する。
- `score()` が公開面から外れ、順位は `rank_candidates` のソート結果として一意に表現される。スコアの重み調整が破壊的変更にならない。

## 不採用案

- **`Candidate` に `#[non_exhaustive]` だけ付けてフィールド `pub` 維持**: `tags` の内部表現リークが残るため不採用。設計優位で private + アクセサを選択した。
- **`is_stable` を `Stability` enum にする**: durability の意味づけは [DR-016](DR-016-durability-model.md) の `Durability` enum（Durable / NotDurable / Unknown）に集約し、`is_stable()` はその `Durable` 判定の convenience（`bool`）とした。`bool` + `durability()` + `tags()` の併用を選択。
- **`score()` を `pub` のまま rustdoc 契約で限定**: 「順序比較にのみ意味があり絶対値は契約しない」と doc 注記する案もあったが、`score()` の絶対値・順序はいずれも `ScoringPolicy` 依存で `Ord` として安定しない。doc 注記より公開面から外す（`pub(crate)`）方が誤用を構造的に防げるため不採用（Decision 3）。
- **`rank_candidates` を `Vec<Candidate> -> Vec<Candidate>` にする**: 所有権の受け渡し契約を不要に固定する。`&mut [Candidate]` の in-place ソート（Rust 慣習）を選択（Decision 2）。

## 関連 DR

- [DR-001](DR-001-workspace-separation.md): ライブラリ / CLI の workspace 分離。downstream 影響評価の前提（ライブラリは依存なしで publish）。
- [DR-002](DR-002-tag-based-evaluation.md): タグベース評価モデル。本 DR の `is_stable()` は durability 軸（DR-016）で定義し直すが、`PathTag` は観測タグとして残す（DR-002 の客観的属性は維持）。Decision 4 の「tagless Candidate を作らない」は DR-002 のタグベースモデルとの整合。
- [DR-007](DR-007-cli-output-design.md): CLI 出力設計。本 DR の Decision 3 で JSON の `score` フィールドを rank index に変更する（DR-007 の `--format json` 出力の意味づけを更新）。
- [DR-012](DR-012-cli-improvements.md): CLI 改善。ルート re-export（`stable_which::find_candidates`）の方針を継承し、Decision 6 でモジュール直パス参照を廃止して完全にルート re-export のみへ寄せる。
- [DR-013](DR-013-module-rename.md): `path_analysis` への module リネーム。本 DR の Decision 5 はその `path_analysis` モジュールを公開面から外し（`pub(crate)` / `mod`）、内部ヘルパーを API 契約から外す。
- [DR-014](DR-014-windows-support.md): Windows サポート。Decision 7 の PATHEXT 直読み（および `find_candidates_with_path_env` への改名理由）は DR-014 の Windows PATH 探索ロジックに由来する。
- [DR-016](DR-016-durability-model.md): Durability モデル。本 DR の `is_stable()` / `durability()` の **API 形状**（カプセル化・アクセサ）を定義し、DR-016 がその **中身（durable-to-pin の判定モデル）** を定義する。
