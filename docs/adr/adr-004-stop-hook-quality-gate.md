# ADR-004: Stop フックによる品質ゲートの実装

## Status

Accepted (2026-03-16)

## Context

Claude Code のエージェントが作業完了を宣言しても、lint エラーやテスト失敗が残っている場合がある。
特にコンテキストが消費された長いセッションでは、エージェントが品質チェックを忘れたまま停止する傾向がある。

参考記事（ハーネスエンジニアリング実装ガイド）では、Stop フックによる「予防的品質ゲート」を推奨しており、
「何度目かのセッションでコンテキストが消費されていても、決定論的検証により品質が落ちない」ことを保証する仕組みとしている。

### 検討事項

1. **無限ループのリスク**: 品質ゲート失敗→作業継続→再停止→再失敗のループ
2. **実行時間**: lint, test, e2e, build の全チェックは数十秒〜数分かかる
3. **E2E テスト環境の有無**: `.env.e2e` が存在しない環境では E2E をスキップすべき

## Decision

**Stop フックで4段階の品質チェックを実行し、失敗時は `decision: "block"` で作業継続を強制する。
無限ループは `stop_hook_active` フラグで防止する。**

### 実行ステップ

| 順番 | コマンド | 目的 |
|------|---------|------|
| 1 | `pnpm lint` | oxlint による静的解析 |
| 2 | `pnpm test` | vitest によるユニットテスト |
| 3 | `pnpm test:e2e` | E2E テスト（`.env.e2e` 不在時は自動スキップ） |
| 4 | `pnpm build` | TypeScript ビルド確認 |

### 無限ループ防止

Claude Code の Stop フック入力には `stop_hook_active` フラグが含まれる:

- **`false`（初回停止）**: 品質ゲートを実行
- **`true`（フックがブロックした後の再停止）**: 品質ゲートをスキップして停止を許可

これにより最大1回のリトライで収束する。エージェントは1回目の停止で品質チェックの結果を受け取り、
修正を試みた後に再度停止を試みる。2回目は `stop_hook_active: true` なのでそのまま停止が許可される。

### 出力形式

品質ゲート失敗時:
```json
{
  "decision": "block",
  "reason": "品質ゲートが失敗しました。以下の問題を修正してください:\n\n**lint** failed:\n```\n...\n```"
}
```

全チェック成功時: 何も出力しない（exit 0 → 停止許可）

## Consequences

### Positive

- エージェントが lint エラーやテスト失敗を残したまま停止できなくなる
- `stop_hook_active` フラグにより無限ループが構造的に防止される
- E2E テストは `.env.e2e` の有無で自動スキップされ、環境に依存しない
- 失敗内容が `reason` に含まれるため、エージェントが具体的な修正アクションを取れる

### Negative

- 全品質チェックの実行に数十秒〜数分かかる（timeout: 300秒に設定）
- `stop_hook_active: true` の2回目はチェックをスキップするため、修正が不完全でも停止を許可してしまう
- npx の初回ダウンロードで追加の遅延が発生する可能性がある

## References

- [Claude Code Hooks リファレンス - Stop 入力](https://code.claude.com/docs/ja/hooks#stop-%E5%85%A5%E5%8A%9B)
- [ハーネスエンジニアリング実装ガイド - フィードバックループの設計](https://nyosegawa.github.io/posts/harness-engineering-best-practices-2026/#%E3%83%95%E3%82%A3%E3%83%BC%E3%83%89%E3%83%90%E3%83%83%E3%82%AF%E3%83%AB%E3%83%BC%E3%83%97%E3%81%AE%E8%A8%AD%E8%A8%88%3A-hooks%E3%81%AE%E6%B4%BB%E7%94%A8)
- `.claude/hooks-stop-quality/src/main.rs`
