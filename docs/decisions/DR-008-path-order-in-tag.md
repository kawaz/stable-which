# DR-008: PATH 発見順序を InPathEnv タグに持たせる

## 決定

`InPathEnv(usize)` として PATH 上の発見順序をタグ自体に保持する。同スコアの候補は `path_order()` メソッドで取得した値の昇順で tie-break する。

Input 候補は `InPathEnv` を持たないため `path_order()` は `usize::MAX` を返し、tie-break で常に最低優先になる。

## 理由

- 同スコアの候補は PATH の先頭にあるもの（= ユーザー/システムの優先度が高い）を選ぶべき
- 暗黙の安定ソートに依存するのは脆い。明示的に値化することで決定的なソートが保証される
- `path_order` は「PATH 上で何番目に見つかったか」なので `InPathEnv` タグの属性にするのが意味的に正しい
- Candidate のフィールドではなくタグに持たせることで、Input（PATH 検索外）と PATH 候補の区別が自然に表現される

## 不採用案

- `Candidate` にフィールド `path_order: usize` を持たせる案: Input に `0` を与えると PATH 先頭の候補より優先されてしまう。`usize::MAX` にしても意味的に不自然
- スコアに微小値として織り込む案: 意味が混ざる
