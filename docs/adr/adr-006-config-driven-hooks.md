# ADR-006: hooks の設定駆動型アーキテクチャ

## ステータス

承認済み (2026-03-19)

## コンテキスト

hooks (Rust 製 exe 4 本) を複数の派生プロジェクト (auto-review-fix-vc, techbook-ledger 等) に転用していた。
プロジェクト間の差分はすべてデータレベル（lint/test コマンド名、対象拡張子、保護ファイルリスト、品質チェックステップ）であり、ロジックは共通だったが、各プロジェクトに Rust ソースをコピーしてカスタマイズしていたため、本家の更新を反映するたびに O(N) の作業コストが発生していた。

## 決定

**1 セットの共通バイナリ + プロジェクトごとの `hooks-config.toml`** で全プロジェクトに対応する。

### 設定ファイル (`hooks-config.toml`)

- exe と同じディレクトリ (`.claude/`) に配置
- `[pre_tool_validate]`: ブロックパターンのプリセット選択、追加保護ファイル
- `[post_tool_linter]`: 拡張子ごとのリンターパイプライン定義
- `[stop_quality]`: 品質チェックステップとタイムアウト
- 設定ファイルが存在しない場合は各 hook がデフォルト動作にフォールバック

### プリセット方式 (pre_tool_validate)

ブロックパターンを `"default"`, `"git"`, `"jj-immutable"`, `"jj-main-guard"`, `"electron"` のプリセット名で選択的に有効化。プリセット名以外の文字列はカスタム正規表現として扱う。

### Stop hook 統合

`hooks-stop-quality` と `hooks-stop-quality-py` を 1 つの exe に統合。ステップは TOML で定義するため、言語に依存しない汎用的な品質ゲートとして機能する。

### 配布

- `pnpm build:all` で本家でビルド
- `pnpm deploy:hooks` で `scripts/deploy-targets.json` に登録された派生プロジェクトに exe を一括コピー
- 派生プロジェクトは `hooks-config.toml` のみを管理

## 影響

- 派生プロジェクトから Rust ソースと cargo ビルド環境を撤去可能
- hooks の更新は本家で 1 回ビルド → `pnpm deploy:hooks` で完了 (O(1))
- 新規プロジェクトへの転用は `deploy-targets.json` にパスを追加し、`hooks-config.toml` を作成するだけ
- `toml` crate 追加による exe サイズ増加は約 100KB 程度 (許容範囲)
