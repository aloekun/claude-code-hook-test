# Analyze Session Transcript

PR の commit 期間に該当するセッション transcript を分析し、実装時の学び・トラブル・ユーザー指示を抽出する。

ADR-030 §transcript 抽出戦略に基づく Phase 0 で確認済の方針:
- transcript ファイルは Rust 側 (cli-merge-pipeline) で時刻 range filter 済 (時刻 range は PR の `first_commit_time` 〜 `merged_at`)
- 本 facet は filter 済 jsonl を読むだけ。生 file を直接 grep しない

**重要な原則:**
- secrets / PII は要約から除外する (トークン、API キー、パスワード、個人情報、長文の生ログ全文)
- 生ログ全文は出力しない (要約のみ)
- 不確実な値は除外する

---

## Input

`.takt/post-merge-feedback-context.json` を Read で読み、`transcript_path` を確認する:

```json
{
  "pr_number": 123,
  "transcript_path": ".takt/post-merge-feedback-transcript.jsonl",
  "first_commit_time": "2026-04-25T08:00:00Z",
  "merged_at": "2026-04-25T10:00:00Z"
}
```

`transcript_path` が空 / file が存在しない / file が空の場合は:

```markdown
## Session Analysis Report

### Status

セッション transcript が見つかりませんでした (該当期間のデータなし)。
```

を出力し `analysis complete` で次へ進める。

## Phase 1: Transcript の読み取り

`transcript_path` を Read で読む。**JSONL** 形式 (1 行 1 entry)。

各 entry のスキーマ (Phase 0 確認済):

```json
{
  "type": "user" | "assistant",
  "timestamp": "2026-04-25T05:44:35.040Z",
  "sessionId": "<uuid>",
  "message": {
    "role": "user" | "assistant",
    "content": [
      { "type": "text", "text": "..." },
      { "type": "thinking", "thinking": "", "signature": "<encrypted>" },
      { "type": "tool_use", "name": "Bash", "input": {...} }
    ]
  }
}
```

注意:
- `thinking` の content は encrypted (`thinking` field は空)。chain-of-thought は抽出不可
- `type: queue-operation` / `type: attachment` は Rust 側で除外済の想定だが、出現したら無視する
- 1.7 MB / 数百行になり得る。重要な箇所だけ要約する

## Phase 2: 知見抽出

以下の観点で抽出する。各観点は **要約のみ**、原文引用は最小限。

### 抽出観点

1. **実装の困難**: 何度も試行錯誤した箇所、アプローチを変更した箇所
   - 例: 「lib-pending-file の atomic write で 3 回 retry した」
2. **ユーザー修正指示**: ユーザーから「そうじゃない」「こうして」と指摘された箇所
   - 例: 「DRY を試みたがユーザーから却下された (テスト独立性優先)」
3. **バグ発見**: 開発中に発見・修正したバグ
   - 例: 「percent encode が `?` `#` を素通りしていた」
4. **ワークアラウンド**: 本来の方法ではなく回避策を適用した箇所
5. **混乱を招いたパターン**: コードの読み間違いや誤った前提

### Plankton 優先度による分類

各知見に対して、再発防止策を以下の Tier で分類する (analyze-pr facet と同じ Tier 体系):

- **Tier 1**: hooks/linter 改善 (`block_pattern`, `custom_lint_rule`, `linter_pipeline`)
- **Tier 2**: テスト/自動化 (`test_addition`, `ci_step`)
- **Tier 3**: ドキュメント/ルール (`claude_md_rule`, `adr`)

---

## Required output

```markdown
## Session Analysis Report

### 実装の学び (要約)

1. {観点 (実装の困難 / ユーザー修正指示 / バグ発見 / ワークアラウンド / 混乱)}: {要約}
   - 推定原因: {原因の推定}
   - 防止策: Tier {N} - {具体的な提案}

### 再発防止候補 (Plankton 分類)

#### Tier 1: Hooks/Linter 改善

| # | Type | Description | Target | Effort | Rationale |
|---|------|-------------|--------|--------|-----------|

#### Tier 2: テスト/自動化

| # | Type | Description | Target | Effort | Rationale |
|---|------|-------------|--------|--------|-----------|

#### Tier 3: ドキュメント/ルール

| # | Type | Description | Target | Effort | Rationale |
|---|------|-------------|--------|--------|-----------|
```

提案がない Tier はセクションごと省略する。

知見がない場合は以下:

```markdown
## Session Analysis Report

### Status

セッションから特筆すべき知見は抽出できませんでした (transcript は読み取り済みだが、再発防止に値する事象なし)。
```

最後に `analysis complete` で終了する。
