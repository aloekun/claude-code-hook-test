# ADR-013: Merge Pipeline — PR マージ + ローカル同期の専用パイプライン

## ステータス

承認済み (2026-04-06) / 改訂 (2026-04-23: `ai` ステップの実装方式を ADR-029 に分離して参照)

## コンテキスト

Push Pipeline (ADR-008) と同様の「ガード + 専用 CLI」パターンで、PR マージ操作も管理したい。

### 現状の問題

1. **`gh pr merge` の直接実行**: マージ後にローカルの jj 環境を同期し忘れるリスクがある
2. **手動ステップの多さ**: マージ → fetch → new master を毎回手動で実行するのは煩雑
3. **将来の拡張**: マージ後に「直前の PR から学びを抽出し、次の開発に活かす」機能を追加する余地を確保したい

### 検討した選択肢

1. **Claude に直接 `gh pr merge` + `jj git fetch` を実行させる**
   - 同期忘れのリスクがあり、将来のステップ追加に対応できない

2. **PreToolUse でブロック + スタンドアロン CLI exe**
   - Push Pipeline と同じパターン。一貫性があり、config-driven で拡張可能
   - マージ戦略（squash）を固定できるため、master の履歴がクリーンに保たれる

3. **Skill (`/merge`) として実装**
   - AI ステップとの親和性は高いが、ハーネス部分は exe のほうが確実
   - 将来 Skill が exe を呼び出す形で統合可能

## 決定

**選択肢 2 を採用する。** PreToolUse の `gh-pr-merge-guard` プリセットで `gh pr merge` をブロックし、`cli-merge-pipeline` (スタンドアロン Rust exe) でマージパイプラインを実行する。

### アーキテクチャ

```text
Claude が "gh pr merge" を実行しようとする
       │
       ▼
PreToolUse (hooks-pre-tool-validate)
  ├─ "gh-pr-merge-guard" プリセットでブロック
  └─ エラーメッセージ: 「pnpm merge-pr を使用してください」
       │
       ▼
Claude が "pnpm merge-pr" を実行する
       │
       ▼
cli-merge-pipeline.exe (スタンドアロン)
  ├─ hooks-config.toml [merge_pipeline] を読み込み
  ├─ jj bookmark → gh pr list --head で PR を自動検出
  ├─ pre_steps を順次実行（マージ前チェック）
  ├─ gh pr merge --squash --delete-branch を実行
  ├─ jj git fetch && jj new master でローカル同期
  └─ post_steps を順次実行（学び提案等の拡張ポイント）
```

### 設計上の決定

| 項目 | 決定 | 理由 |
|---|---|---|
| マージ戦略 | squash 固定 | master の履歴を 1 PR = 1 コミットに保つ |
| PR 検出 | jj bookmark から自動検出 | `pnpm push` / `pnpm create-pr` と同じ方式で一貫性がある |
| ブランチ削除 | `--delete-branch` で自動削除 | マージ済みブランチの残留を防ぐ |
| ローカル同期 | `jj git fetch` + `jj new master` | マージ後すぐに master 最新から作業を開始できる |
| ステップ分離 | `pre_steps`（マージ前）/ `post_steps`（マージ後） | 学び提案等の post-merge 処理を正しいタイミングで実行 |
| 学び提案機能 | 将来実装（`post_steps` に `type = "ai"` ステップ） | config に追加するだけで拡張可能 |

### 設定例

```toml
[merge_pipeline]
step_timeout = 120

# マージ前チェック
# [[merge_pipeline.pre_steps]]
# name = "ci_check"
# type = "command"
# cmd = "gh pr checks --required"

# マージ後の学び提案機能（将来実装）
# [[merge_pipeline.post_steps]]
# name = "post_merge_learnings"
# type = "ai"
# prompt = "analyze_pr_learnings"
```

## 影響

### Positive

- マージ後のローカル同期が自動化され、手動ステップによるミスがなくなる
- Push Pipeline と同じ「ガード + CLI」パターンで一貫性がある
- `pre_steps` / `post_steps` の分離により、学び提案等の post-merge 処理を正しいタイミングで実行可能

### Negative

- 新しい exe のビルドが `build:all` に追加される（ビルド時間の微増）

### 将来の展望 (2026-04-23 追加)

- **`ai` ステップの実装方式 (ADR-029)**: `[[merge_pipeline.post_steps]]` の `type = "ai"` スロット (現状 [src/cli-merge-pipeline/src/main.rs:313-322](../../src/cli-merge-pipeline/src/main.rs#L313-L322) で SKIP 実装) は、[ADR-029: Post-Merge Feedback の自動起動](adr-029-post-merge-feedback-auto-trigger.md) に従って「`.claude/post-merge-feedback-pending.json` への atomic 書き込み」として実装する。新規 Stop hook が pending file を検出して `additionalContext` 経由で Claude に skill 起動を指示する構成のため、exe 自体は AI を spawn しない。ADR-022 原則 1 (新規 artifact への自己記述) の枠内で完結する
- **pre_steps 拡張**: CI 必須チェック、コンフリクト事前検出、secret scan 等を `type = "command"` で追加可能

## References

- [ADR-008: Push Pipeline ハーネスの実装](adr-008-push-pipeline-harness.md) — 同じ「ガード + CLI」パターンの先行例
- [ADR-012: src/ ディレクトリの命名規約](adr-012-src-naming-convention.md) — `cli-` プレフィックスの命名根拠
- [ADR-014: Post-Merge Feedback](adr-014-post-merge-feedback.md) — `ai` ステップで呼び出す skill のフロー定義
- [ADR-029: Post-Merge Feedback の自動起動](adr-029-post-merge-feedback-auto-trigger.md) — `ai` ステップの具体実装仕様 (2026-04-23 追加)
