# ADR-003: hooks の配置規則とビルド戦略

## Status

Superseded by [ADR-010](adr-010-hooks-layout-and-build-strategy-v2.md) (2026-04-04)

Originally: Accepted (2026-03-16)

## Context

Claude Code の hooks を複数管理する上で、ファイル配置・ビルドフロー・バージョン管理のルールを統一する必要がある。

### 配置に関する制約

- Claude Code（Windows、2026/02 時点）では、hooks スクリプト/exe は **`.claude/` 直下** に配置する必要がある
- `.claude/hooks/` のようなサブディレクトリに置くと認識されない
- `settings.local.json` の `command` フィールドで `%CLAUDE_PROJECT_DIR%` 変数を使ってパスを指定する

### 現在の hooks 一覧

| フック名 | 種別 | exe 名 | ソースディレクトリ |
|---------|------|--------|----------------|
| hooks-pre-tool-validate | PreToolUse | `hooks-pre-tool-validate.exe` | `.claude/hooks-pre-tool-validate/` |
| hooks-post-tool-linter | PostToolUse | `hooks-post-tool-linter.exe` | `.claude/hooks-post-tool-linter/` |
| hooks-stop-quality | Stop | `hooks-stop-quality.exe` | `.claude/hooks-stop-quality/` |

## Decision

**以下の配置規則とビルド戦略を標準とする。**

### ディレクトリ構成

```text
.claude/
├── settings.local.json          # hooks 設定
├── hooks-pre-tool-validate.exe   # ビルド済み exe（.gitignore）
├── hooks-post-tool-linter.exe    # ビルド済み exe（.gitignore）
├── hooks-stop-quality.exe        # ビルド済み exe（.gitignore）
├── hooks-pre-tool-validate/      # PreToolUse フックのソース
│   ├── Cargo.toml
│   └── src/main.rs
├── hooks-post-tool-linter/       # PostToolUse フックのソース
│   ├── Cargo.toml
│   └── src/main.rs
└── hooks-stop-quality/           # Stop フックのソース
    ├── Cargo.toml
    └── src/main.rs
```

### 命名規則

- **exe ファイル**: `.claude/<機能名>.exe` — 直下に配置
- **ソースディレクトリ**: `.claude/<機能名>/` — Cargo プロジェクトとして独立
- **settings.local.json での参照**: `"%CLAUDE_PROJECT_DIR%\\.claude\\<機能名>.exe"`

### ビルド戦略

- `package.json` に個別ビルドコマンドと一括ビルドコマンドを定義:
  - `pnpm build:hooks-pre-tool-validate` — PreToolUse フック単体ビルド
  - `pnpm build:hooks-post-tool-linter` — PostToolUse フック単体ビルド
  - `pnpm build:hooks-stop-quality` — Stop フック単体ビルド
  - `pnpm build:all` — 全フック一括ビルド
- 各コマンドは `cd .claude/<dir> && cargo build --release && cp target/release/<name>.exe ../<name>.exe` の形式

### バージョン管理

- `.gitignore` で除外するもの:
  - ビルド済み exe（`pnpm build:all` で再生成可能）
  - `target/` ディレクトリ（Rust ビルド成果物）
- バージョン管理するもの:
  - `Cargo.toml` と `src/` 以下のソースコード
  - `settings.local.json`（hooks 設定）

## Consequences

### Positive

- フックの追加時に命名規則・配置が明確で迷わない
- `pnpm build:all` 一発で全フックを再ビルドできる
- exe はバージョン管理外なのでリポジトリサイズが肥大化しない
- 各フックが独立した Cargo プロジェクトなので、依存関係の競合が起きない

### Negative

- フック追加時に `package.json` の scripts と `.gitignore` の両方を更新する必要がある
- クローン直後は `pnpm build:all` を実行しないと hooks が動作しない

### 新しいフックを追加する手順

1. `.claude/<機能名>/` に Cargo プロジェクトを作成
2. `package.json` に `build:hooks-<機能名>` スクリプトを追加し、`build:all` にチェーン
3. `.gitignore` に `.claude/<機能名>.exe` と `.claude/<機能名>/target/` を追加
4. `settings.local.json` の該当フックイベントに exe のパスを登録
5. `pnpm build:all` でビルド確認

## References

- README.md — `.claude/` 直下配置が必要という初期知見
- `.claude/settings.local.json` — hooks 設定の実例
- `package.json` — ビルドスクリプト定義
