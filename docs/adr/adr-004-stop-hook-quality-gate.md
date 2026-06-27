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

### takt subsession skip (2026-06-26 追加、PR-W1 follow-up)

takt workflow が起動する subsession (例: weekly-review の whole-tree reviewer / post-merge-feedback の analyze-pr / analyze-session / analyze-prepush-reports) は **`edit: false` で起動される read-only な分析セッション** が多い。これらの subsession で Stop フックが品質ゲート失敗を返すと、subsession は `edit: false` 制約と矛盾する「直せ」指示を受け取り、稀に **stray edit を試みる事故** が発生する (2026-06-26、PR #221 で観測。post-merge-feedback subsession が `src/lib-report-formatter/src/lib.rs` を意図せず編集)。

そもそも品質ゲートの趣旨は **本対話セッションの品質担保** であり、takt subsession に適用すべきではない (= 本 ADR の責務範囲外)。よって以下の条件で品質ゲートを skip する:

- `.takt/runs/*/meta.json` を scan し、いずれかが **`status: "running"` であれば skip**
- 1 件目が見つかった時点で短絡 return (= I/O 最小化)
- malformed JSON / read error は defensive に skip (`status == "running"` と誤判定しない fail-closed)

#### 同 marker の他用途

`.takt/runs/<slug>/meta.json` の `status` field は ADR-030 (= 決定論的 post-merge-feedback) の `.failed` marker 経路と、`hooks-session-start` の reaper module (ADR-030 §L2 out-of-process orphan run 検出) でも使われており、本 ADR の追加判定は既存 marker の **読み取り側責務拡張** のみで実装される (新規 marker 不要、既存設計を再利用)。

#### 実装の所在

[src/hooks-stop-quality/src/main.rs](../../src/hooks-stop-quality/src/main.rs) の `should_skip_quality_gate()` で `stop_hook_active` チェック直後に `takt_subsession_active()` を呼ぶ 2 段判定。test 9 件で各種ケース (no runs dir / no meta / status=completed のみ / status=running 混在 / malformed JSON 等) を網羅。

#### 由来事例

PR-3a 系統で複数 PR を local で iterative に merge していた最中、新 PC で `.jj/repo/config.toml` の `auto-track-bookmarks` 設定欠落により merge-pipeline の `sync_local()` が stale local master を base にしたことが root cause。働きとして stale tree 上で `cargo clippy` が `unnecessary_sort_by` warning を flag し、後続の post-merge-feedback subsession に Stop hook 経由で「修正せよ」指示が伝達された (= 連鎖の半分)。merge-pipeline 側の根本修正は [ADR-013](adr-013-merge-pipeline.md) § sync_local の前提条件 を参照。本 ADR の subsession skip は **同型事故の多層防御** として導入。

### 出力形式

品質ゲート失敗時:
```json
{
  "decision": "block",
  "reason": "品質ゲートが失敗しました。以下の問題を修正してください:\n\n**lint** failed:\n```\n...\n```"
}
```

全チェック成功時: 何も出力しない（exit 0 → 停止許可）

### Python 版 Stop フック (hooks-stop-quality-py)

TypeScript 版と同じアーキテクチャで Python プロジェクト向けの品質ゲートを提供する。

| 順番 | コマンド | 目的 |
|------|---------|------|
| 1 | `pnpm py-lint` | ruff check によるリント |
| 2 | `pnpm py-test` | pytest によるユニットテスト |
| 3 | `pnpm py-typecheck` | mypy による型チェック |

TypeScript 版と共通の設計:
- fail-closed: stdin 読み込みエラー/JSON パース失敗時は block 判定を出力
- ステップごとのタイムアウト（120秒）でハング防止
- `stop_hook_active` による無限ループ防止

利用側プロジェクトでは、不要な言語の Stop フックを `settings.local.json.template` から削除して運用する。

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
- `src/hooks-stop-quality/src/main.rs`
