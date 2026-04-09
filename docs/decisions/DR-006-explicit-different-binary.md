# DR-006: DifferentBinary タグを明示的に保持

## 決定

SameCanonical/SameContent の不在で暗黙判定するのではなく、DifferentBinary を明示タグとして付与する。

## 理由

CLI 表示で赤色タグとして視認性が重要。タグ一覧を見て「別バイナリ」が一目で分かるべき。authsock-warden の出力でも `different-binary` が赤色表示されており、ユーザー体験として必要。
