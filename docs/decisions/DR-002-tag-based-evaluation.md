# DR-002: タグベース評価モデル

## 決定

候補にタグ（PathTag enum）を付与し、スコアリングはタグの組み合わせで計算する。

- タグは客観的な属性の列挙（Input, InPathEnv, SameCanonical, BuildOutput 等）
- スコアは ScoringPolicy に応じた主観的な重み付け（SameBinary / Stable）
- タグは `candidate.path`（エントリパス）に対して評価。`canonical` ではない

## 理由

タグとスコアを分離することで、ライブラリ利用者が独自の選択ロジックを組める。authsock-warden のような消費者はタグを見て自前の UI を構築する。
