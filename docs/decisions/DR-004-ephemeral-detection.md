# DR-004: Ephemeral 判定は case insensitive ワードマッチ

## 決定

`path.parent()` に対して `/\b(cache|tmp|temp|temporary)\b/i` でマッチ。`.app` バンドル内のパスは `.app` より上のコンポーネントを除外。

## 理由

固定パス（`~/.cache/` 等）のリストではなく、ディレクトリ名の「臭い」で判定する方が汎用性が高い。`cache-warden` のようなバイナリ名やアプリ名への誤検出を `.app` 除外と parent() チェックで防ぐ。
