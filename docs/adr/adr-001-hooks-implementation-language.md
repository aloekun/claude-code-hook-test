# ADR-001: Claude Code hooks の実装言語として Rust を採用

## Status

Accepted (2026-03-16)

## Context

Claude Code の hooks（PreToolUse / PostToolUse）をWindows環境で動作させる必要がある。
公式ドキュメントや参考記事のサンプルは bash スクリプトで書かれているが、
Windows 環境では以下の問題が発生した。

### bash スクリプトで発生した問題

- **`jq` が未インストール**: hooks スクリプト内で JSON パースに `jq` を使っていたが、Windows にはデフォルトで入っていない
- **`cat` が見つからない**: `set -euo pipefail` の厳格モードで `cat: command not found` エラーが発生
- **PATH の不整合**: Claude Code がフックを実行する際のシェル環境と、ユーザーのターミナル環境で PATH が異なる
- **`npx` の呼び出し**: Windows では `npx` は `npx.cmd` バッチファイルであり、bash から直接呼べない場合がある

### 検討した選択肢

1. **bash スクリプト + 依存ツールのインストール**: `jq` 等を winget でインストールし PATH を通す
2. **PowerShell スクリプト**: Windows ネイティブだが Claude Code の hooks 仕様が bash 前提
3. **Node.js スクリプト**: クロスプラットフォームだが起動が遅い（PostToolUse はミリ秒単位が理想）
4. **Rust ネイティブ exe**: 外部依存なし、高速、クロスプラットフォームビルド可能

## Decision

**全プロジェクト共通で、Claude Code hooks は Rust でネイティブ exe としてビルドする。**

理由:

- **外部依存ゼロ**: `jq`、`cat`、`bash` の有無に左右されない。stdin から JSON を読み、stdout に JSON を書くだけ
- **実行速度**: 参考記事（ハーネスエンジニアリング実装ガイド）が「PostToolUse はミリ秒単位で完了する必要がある」と強調しており、Rust の起動速度はこの要件に最適
- **既存実績**: PreToolUse フック（`hooks-pre-tool-validate.exe`）で同じアプローチが既に安定稼働している
- **serde_json による堅牢な JSON 処理**: `jq` のような外部ツールに頼らず、型安全な JSON パース/生成が可能

## Consequences

### Positive

- Windows 環境で hooks が確実に動作する
- PreToolUse / PostToolUse で実装パターンが統一される
- ビルド済み exe を `.gitignore` で除外し、ソースコードだけをバージョン管理できる
- `pnpm build:hooks` 一発で全 hooks を再ビルドできる

### Negative

- Rust のビルド環境（`cargo`）が必要。チームメンバーに Rust 未経験者がいる場合は学習コストが発生する
- bash スクリプトと比較して記述量が多い（ただし型安全性とのトレードオフ）
- 外部コマンド（`npx biome`、`npx oxlint`）の呼び出しには `cmd /c` 経由が必要

## References

- [ハーネスエンジニアリング実装ガイド - 4つのhookパターン](https://nyosegawa.github.io/posts/harness-engineering-best-practices-2026/#4%E3%81%A4%E3%81%AEhook%E3%83%91%E3%82%BF%E3%83%BC%E3%83%B3)
- `src/hooks-pre-tool-validate/` (PreToolUse: hooks-pre-tool-validate.exe)
- `src/hooks-post-tool-linter/` (PostToolUse: hooks-post-tool-linter.exe)
