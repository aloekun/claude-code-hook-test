# ADR-002: PostToolUse リンターフックで Biome + oxlint の二段階構成を採用

## Status

Accepted (2026-03-16)

## Context

Claude Code の PostToolUse フックで、TypeScript/JavaScript ファイル編集後に自動的にコード品質を担保する仕組みが必要。
参考記事（ハーネスエンジニアリング実装ガイド）の「4つのhookパターン」に基づき、
実行速度を最優先としたリンター構成を検討した。

### 速度階層の原則

参考記事が定義する検証レイヤー:

| レイヤー | 実行時間 | 用途 |
|---------|--------|------|
| PostToolUse | ミリ秒 | フォーマット・リント自動修正 |
| プリコミット | 秒 | 型チェック・リント |
| CI | 分 | 全テストスイート |
| 人間レビュー | 時間以上 | コードレビュー |

PostToolUse フックはミリ秒〜秒単位で完了する必要があり、
ツール選定では **Rust 製の高速ツール** が前提となる。

### 検討した構成

1. **ESLint + Prettier**: 従来の標準構成。Node.js 製で起動が遅い（秒単位）
2. **Biome 単体**: フォーマット + リントを1ツールで。高速だがリントルールのカバレッジがやや狭い
3. **oxlint 単体**: リントに特化。ESLint 互換ルールが豊富だがフォーマット機能はない
4. **Biome (フォーマット) + oxlint (リント)**: 役割分担で両者の強みを活かす

## Decision

**Biome をフォーマッタとして、oxlint をリンターとして併用する二段階構成を採用する。**

実行フロー:

```text
ファイル編集 (Write/Edit)
  ↓
1. biome format --write  … フォーマット自動修正
  ↓
2. oxlint --fix          … リント違反の自動修正
  ↓
3. oxlint                … 残存違反の診断取得
  ↓
4. additionalContext      … 診断結果を Claude にフィードバック
```

選定理由:

- **実行速度優先**: 両ツールとも Rust 製で、ESLint/Prettier の 50〜100 倍高速
- **自動修正率の最大化**: まず Biome でフォーマットを統一し、次に oxlint で論理的な問題を修正。二段階にすることで自動修正率が向上する
- **コンテキスト消費の削減**: 自動修正を先に実行し、「残った違反だけ」を Claude にフィードバックすることで、トークン使用量を最小化
- **参考記事の推奨構成に準拠**: ハーネスエンジニアリング実装ガイドの推奨パターンに沿っている

## Consequences

### Positive

- 編集のたびに自動フォーマット + リントが走り、コード品質が常に担保される
- Claude が残存違反を `additionalContext` で受け取り、自己修正サイクルを回せる
- npx 経由で実行するため、プロジェクトごとのバージョン固定が容易

### Negative

- npx の初回実行時にパッケージダウンロードが発生し、数秒〜数十秒の遅延がある
- biome と oxlint のルールが一部重複する可能性がある（フォーマット関連）
- Windows 環境では `cmd /c npx` 経由の呼び出しが必要

### 将来の検討事項

- biome のリントルールが成熟した段階で oxlint との統合を再検討
- プロジェクトルートに `biome.json` / oxlint 設定を置いてルールをカスタマイズ
- 診断結果の構造化（行番号・ルールID の JSON 化）でより精密なフィードバック

## References

- [ハーネスエンジニアリング実装ガイド - 4つのhookパターン](https://nyosegawa.github.io/posts/harness-engineering-best-practices-2026/#4%E3%81%A4%E3%81%AEhook%E3%83%91%E3%82%BF%E3%83%BC%E3%83%B3)
- [Biome](https://biomejs.dev/) — Rust 製フォーマッタ/リンター（10〜25倍高速）
- [oxlint](https://oxc.rs/docs/guide/usage/linter.html) — Rust 製リンター（50〜100倍高速）
- `src/hooks-post-tool-linter/src/main.rs`
