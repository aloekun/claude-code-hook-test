# Analyze Pre-Push Reports

PR がマージされる前の最終 push 時に生成された pre-push-review レポート (simplicity / security) を集約し、再発防止に値する指摘をまとめる。

**重要な原則:**
- 読み取り専用。コードの修正は一切行わない
- pre-push-review レポートが見つからない / 空の場合は「対象データなし」で正常終了する
- 既に push 時に APPROVED されている指摘でも、「再発防止策に転用できそうな知見」がある場合は抽出する

---

## Input

`.takt/post-merge-feedback-context.json` を Read で読み、`prepush_reports_dir` を確認する:

```json
{
  "pr_number": 123,
  "prepush_reports_dir": ".takt/runs/20260425-094925-pre-push-review/reports"
}
```

`prepush_reports_dir` が空 / dir が存在しない場合は:

```markdown
## Pre-Push Reports Analysis

### Status

pre-push-review の reports が見つかりませんでした。
```

を出力し `analysis complete` で次へ進める。

## Phase 1: レポートの収集

Glob で `<prepush_reports_dir>/*.md` を列挙し、それぞれ Read で内容を取得する。

典型的なレポート:
- `simplicity-review.md` — 簡潔性レビューの指摘
- `security-review.md` — セキュリティレビューの指摘
- `supervisor-validation.md` — supervisor 判定 (任意)
- `summary.md` — 統合サマリ (任意)

各レポートは markdown 形式で、findings / verdict / recommendations が含まれる想定。

## Phase 2: 集約・整理

以下の観点で要約する:

1. **明示された finding**: 各レビューで `REJECT` / `needs_fix` 判定だった指摘
2. **修正完了済の事象**: takt の fix loop で APPROVE に至った修正の系統
3. **supervise 判定の警告**: supervisor の警告 / コメント

各 finding に対して、**Plankton 優先度 (Tier 1〜3)** で再発防止策を提案する。

注意点:
- supervisor が `ready to push` で APPROVE した場合、**コードレベルの修正は不要** だが、それでも「同じパターンを次回検出するための仕組み」を Tier 1 候補として検討する
- 個別の review コメントは要約のみ (原文引用は最小限)

---

## Required output

```markdown
## Pre-Push Reports Analysis

### 集約サマリ

- 対象 reports: {ファイル名のリスト}
- simplicity verdict: {APPROVE / REJECT / N/A}
- security verdict: {APPROVE / REJECT / N/A}
- supervisor verdict: {APPROVE / REJECT / N/A}

### 主要 findings (要約)

1. {要約} (出典: {report ファイル名})
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

該当なしの場合は以下:

```markdown
## Pre-Push Reports Analysis

### Status

pre-push reports は読み込めましたが、再発防止に値する findings は見つかりませんでした。
```

最後に `analysis complete` で終了する。
