# DR-013: version_manager モジュールを path_analysis にリネーム

## 決定

`version_manager` モジュールを `path_analysis` にリネーム。

## 理由

version_manager.rs はバージョンマネージャ検出（detect_version_manager）以外にも、ビルド成果物検出（is_build_output）、一時パス検出（is_ephemeral）、ファイル比較（files_have_same_content）、実行ビットチェック（is_executable）を含む。モジュール名と実態が乖離しており、ライブラリ利用者が `stable_which::path_analysis::files_have_same_content` を見たときに意味不明になる。crates.io publish 前にリネームする。
