# Analyze Pre-Push Reports

PR がマージされる前の最終 push 時に生成された pre-push-review レポート (simplicity / security) を集約し、再発防止に値する指摘をまとめる。

**重要な原則:**
- 読み取り専用。コードの修正は一切行わない
- pre-push-review レポートが見つからない / 空の場合は「対象データなし」で正常終了する
- 既に push 時に APPROVED されている指摘でも、「再発防止策に転用できそうな知見」がある場合は抽出する

---

## Input

`.takt/post-merge-feedback-context.json` を Read で読み、`prepush_reports_dirs` を確認する:

```json
{
  "pr_number": 123,
  "prepush_reports_dirs": [
    ".takt/runs/20260425-094925-pre-push-review/reports",
    ".takt/runs/20260425-153012-pre-push-review/reports"
  ]
}
```

**配列である。** 同じ PR に複数回 push した場合、その回数だけ pre-push run が存在し、すべてが分析対象になる。**古い順 (run の開始時刻の昇順) に並んでいる。**

`prepush_reports_dirs` が空配列の場合は:

```markdown
## Pre-Push Reports Analysis

### Status

この PR に紐づく pre-push-review の reports が見つかりませんでした。
```

を出力し `analysis complete` で次へ進める。

**空配列は異常ではない。** 対象 PR の branch と照合できた run だけが渡される仕様で、照合できない run は意図的に除外されている。**存在しないレポートを他の場所から探しに行かないこと** — 別 PR の run を拾うと、誤った PR の知見がレポートに混入する。

## Phase 1: レポートの収集

`prepush_reports_dirs` の**各要素について** Glob で `<dir>/*.md` を列挙し、それぞれ Read で内容を取得する。

複数ある場合は push の回数だけレビューが行われたことを意味する。**後の run で解消された指摘を「未解決」として報告しない**よう、同じ指摘が複数 run に現れる場合は最新の状態を優先する。

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

## 出力言語

- **レポート本文は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンドはもちろん、**本 facet が出力する固定トークンも訳さない** — 完了条件の `analysis complete` (`post-merge-feedback.yaml` の `rules.condition` が英語リテラルで照合)、および転記する verdict の値 `APPROVE` / `REJECT` / `N/A` / `needs_fix`。verdict は上流 facet の出力をそのまま写す欄であり、訳すと集約側で元の判定と突き合わせられなくなる
