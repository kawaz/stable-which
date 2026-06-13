# launchd / systemd サービス登録におけるバイナリパスの制約と推奨事項

## 1. launchd (macOS)

### ProgramArguments のパス推奨

- **絶対パスが必須**。launchd は PATH を検索しない。
- `Program` キーも同様に絶対パスが必要。

### シンボリックリンクの扱い

- **使用可能**。launchd はシンボリックリンクを解決して実行する。
- リンク先が消えた場合（brew upgrade 等）、サイレントに起動失敗する。

### PATH 環境変数の扱い

- launchd のジョブは最小限の環境変数しか持たない（`/usr/bin:/bin:/usr/sbin:/sbin` 程度）。
- `/opt/homebrew/bin` や `/usr/local/bin` は含まれない。
- ProgramArguments に絶対パスを書くべきなので PATH 依存設計自体が非推奨。

### brew upgrade 後の挙動

| パス | 例 | 特徴 |
|---|---|---|
| シンボリックリンク（安定パス） | `/opt/homebrew/bin/nginx` | upgrade で自動更新される |
| Cellar 内の実体パス | `/opt/homebrew/Cellar/nginx/1.25.3/bin/nginx` | upgrade 後にディレクトリごと消える可能性 |

- Homebrew 自身の plist は Cellar パスを使うが、`brew services restart` で再生成される前提。

### パスの使い分け

| パス | 用途 | 注意点 |
|---|---|---|
| `/opt/homebrew/bin/` | Apple Silicon Mac の Homebrew（symlink） | upgrade に追従するがリンク先は不安定 |
| `/usr/local/bin/` | Intel Mac の Homebrew、手動インストール | Apple Silicon ではデフォルトでない |
| `/Applications/XXX.app/Contents/MacOS/` | GUI アプリのバイナリ | 比較的安定 |
| `/usr/bin/`, `/usr/sbin/` | OS 標準コマンド | SIP 保護下で最も安定 |

## 2. systemd (Linux)

### ExecStart のパス制約

- **絶対パスが必須**。`/` でなければ unit invalid。
- systemd は PATH を使わない。

### シンボリックリンクの扱い

- **使用可能**。リンク先消失時は起動失敗。

### Nix 環境

- `/nix/store/<hash>-<name>-<version>/bin/<binary>` — 更新で必ずパス変更。
- NixOS: unit は Nix が自動生成。問題にならない。
- 非 NixOS: `~/.nix-profile/bin/<binary>` のプロファイルリンクを使う。
- home-manager: `systemd.user.services` で宣言的管理、パス自動解決。

## 3. 共通の落とし穴

### バージョンマネージャ配下のパスでサービス登録

| ツール | パスの例 | 問題 |
|---|---|---|
| asdf | `~/.asdf/installs/nodejs/20.11.0/bin/node` | バージョン切り替え・アンインストールで消失 |
| mise | `~/.local/share/mise/installs/node/20.11.0/bin/node` | 同上 |
| nvm | `~/.nvm/versions/node/v20.11.0/bin/node` | 同上 |
| pyenv | `~/.pyenv/versions/3.12.1/bin/python` | 同上 |
| rbenv | `~/.rbenv/versions/3.3.0/bin/ruby` | 同上 |
| rustup | `~/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc` | toolchain 更新で変更 |
| Homebrew Cellar | `/opt/homebrew/Cellar/node/21.5.0/bin/node` | `brew upgrade` で削除 |
| Nix store | `/nix/store/abc123-nodejs-20.11.0/bin/node` | ハッシュ変更 |

### shim をサービスに指定した場合の問題

| ツール | shim パス | 実態 |
|---|---|---|
| asdf | `~/.asdf/shims/node` | シェルスクリプト（`.tool-versions` 参照） |
| mise | `~/.local/share/mise/shims/node` | 同上 |
| rbenv | `~/.rbenv/shims/ruby` | 同上 |

問題点:
1. **カレントディレクトリ依存**: shim は `.tool-versions` 等を上方探索。サービスの WorkingDirectory では意図しないバージョンになる
2. **環境変数不足**: `ASDF_DIR` 等が launchd/systemd の最小環境にない
3. **起動オーバーヘッド**: シェルスクリプトなので毎回シェル起動コスト
4. **予期しないバージョン切り替え**: `asdf global` 変更がサービスに波及

## まとめ

| 観点 | launchd | systemd |
|---|---|---|
| 絶対パス必須 | 必須 | 必須 |
| シンボリックリンク | 可（リンク切れ注意） | 可（リンク切れ注意） |
| PATH の影響 | なし | なし |
| バージョンマネージャ | 管理外の固定パスに配置すべき | 同左（NixOS は宣言的管理） |
| shim | 使うべきでない | 使うべきでない |

**鉄則**: サービスに登録するバイナリは「自分で明示的に管理し、勝手に消えないパス」に置く。
