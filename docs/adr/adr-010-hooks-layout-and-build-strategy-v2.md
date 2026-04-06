# ADR-010: hooks の配置規則とビルド戦略 v2

## Status

Accepted (2026-04-04)

Supersedes [ADR-003](adr-003-hooks-layout-and-build-strategy.md)

## Context

ADR-003 では hooks のソースコードを `.claude/` 直下に配置していたが、以下の運用上の問題が顕在化した。

### 問題点

1. **Claude Code の実行許可が頻発**: `.claude/` フォルダ内のファイル編集は都度ユーザーの許可確認が発生し、開発効率が大幅に低下する
2. **派生プロジェクトからのソース参照が困難**: 他プロジェクトから hooks のソースコードを確認する際にも `.claude/` フォルダへの Read 許可が頻発する
3. **ソースとランタイム成果物の混在**: ビルド元のソースコードとビルド済み exe・設定ファイルが同じディレクトリに存在し、関心の分離ができていない

### 制約（ADR-003 から継続）

- Claude Code（Windows）では、hooks の exe は **`.claude/` 直下** に配置する必要がある
- `settings.local.json` の `command` フィールドで `{{PROJECT_DIR}}` 変数を使ってパスを指定する
- exe のランタイム設定ファイル（`hooks-config.toml` 等）も `.claude/` に配置する

## Decision

**hooks のソースコードを `src/` ディレクトリに移動し、ビルド成果物（exe）のみを `.claude/` に配置する。**

### ディレクトリ構成

```text
project-root/
├── src/                                 # hooks ソースコード
│   ├── hooks-pre-tool-validate/         # PreToolUse フックのソース
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── hooks-post-tool-linter/          # PostToolUse フックのソース
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── hooks-stop-quality/              # Stop フックのソース
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── cli-push-pipeline/              # Push Pipeline CLI のソース
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── cli-pr-monitor/                 # PR Monitor CLI のソース
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── hooks-session-start/             # SessionStart フックのソース
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── check-ci-coderabbit/            # CI チェック CLI のソース
│       ├── Cargo.toml
│       └── src/main.rs
├── .claude/                             # ランタイム成果物のみ
│   ├── settings.local.json              # hooks 設定（生成ファイル）
│   ├── settings.local.json.template     # テンプレート
│   ├── hooks-config.toml               # ランタイム設定
│   ├── custom-lint-rules.toml          # カスタムリントルール
│   ├── hooks-pre-tool-validate.exe      # ビルド済み exe（.gitignore）
│   ├── hooks-post-tool-linter.exe
│   ├── hooks-stop-quality.exe
│   ├── cli-push-pipeline.exe           # CLI ツール exe
│   ├── cli-pr-monitor.exe
│   ├── hooks-session-start.exe
│   └── check-ci-coderabbit.exe
└── package.json                         # ビルドスクリプト
```

### 命名規則

- **exe ファイル**: `.claude/<機能名>.exe` — `.claude/` 直下に配置（変更なし）
- **ソースディレクトリ**: `src/<機能名>/` — Cargo プロジェクトとして独立
- **settings.local.json での参照**: `"{{PROJECT_DIR}}\\.claude\\<機能名>.exe"`（変更なし）

### ビルド戦略

- `package.json` に個別ビルドコマンドと一括ビルドコマンドを定義:
  - `pnpm build:<フォルダ名>` — 各フック/CLI 単体ビルド
  - `pnpm build:all` — 全フック一括ビルド
- 各コマンドは `cd src/<dir> && cargo build --release && cp target/release/<name>.exe ../../.claude/<name>.exe` の形式
- `pnpm deploy:hooks` で派生プロジェクトへの exe 配布（変更なし）

### バージョン管理

- `.gitignore` で除外するもの:
  - ビルド済み exe（`pnpm build:all` で再生成可能）
  - `src/*/target/` ディレクトリ（Rust ビルド成果物）
- バージョン管理するもの:
  - `src/*/Cargo.toml` と `src/*/src/` 以下のソースコード
  - `.claude/settings.local.json.template`（hooks 設定テンプレート）
  - `.claude/hooks-config.toml`（ランタイム設定）

## Consequences

### Positive

- **Claude Code の許可確認が不要**: `src/` 配下のソースコード編集に `.claude/` フォルダへのアクセス許可が発生しない
- **関心の分離**: ソースコード（`src/`）とランタイム成果物（`.claude/`）が明確に分離される
- **派生プロジェクトからの参照が容易**: ソースコードが通常のディレクトリにあるため Read 許可なしで参照可能
- ADR-003 の利点はすべて維持される（一括ビルド、独立 Cargo プロジェクト、exe はバージョン管理外）

### Negative

- ADR-003 と同様: フック追加時に `package.json` と `.gitignore` の両方を更新する必要がある
- ADR-003 と同様: クローン直後は `pnpm build:all` を実行しないと hooks が動作しない
- ビルド出力先が2階層上（`../../.claude/`）になるため、ビルドスクリプトがやや複雑

### 新しいフックを追加する手順

1. `src/<機能名>/` に Cargo プロジェクトを作成
2. `package.json` に `build:<フォルダ名>` スクリプトを追加し、`build:all` にチェーン
3. `.gitignore` に `.claude/<機能名>.exe` と `src/<機能名>/target/` を追加
4. `.claude/settings.local.json.template` の該当フックイベントに exe のパスを登録
5. `pnpm build:all` でビルド確認

## References

- [ADR-003](adr-003-hooks-layout-and-build-strategy.md) — 本 ADR の前身（Superseded）
- README.md — `.claude/` 直下配置が必要という初期知見
- `.claude/settings.local.json.template` — hooks 設定の実例
- `package.json` — ビルドスクリプト定義
