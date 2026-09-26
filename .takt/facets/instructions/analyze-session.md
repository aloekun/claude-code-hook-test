# Analyze Session Transcript

PR の commit 期間に該当するセッション transcript を分析し、実装時の学び・トラブル・ユーザー指示を抽出する。

ADR-030 §transcript 抽出戦略に基づく Phase 0 で確認済の方針:
- transcript ファイルは Rust 側 (cli-merge-pipeline) で時刻 range filter 済 (時刻 range は PR の `first_commit_time` 〜 `merged_at`)
- 本 facet は filter 済 jsonl を読むだけ。生 file を直接 grep しない

**重要な原則:**
- secrets / PII は要約から除外する (トークン、API キー、パスワード、個人情報、長文の生ログ全文)
- 生ログ全文は出力しない (要約のみ)
- 不確実な値は除外する
- **ファイルを一切書かない。** 解析用の中間ファイル・スクリプトも、レポートファイルも作らない。transcript は Read の範囲指定と Grep で分割して読む
  - **レポートは応答本文として返す。** takt は本 step の後にレポートフェーズを別に走らせ、そこで応答本文を受け取って Report Directory へ保存する (レポートフェーズではツールが使えない)。本 step 中に Write でレポートファイルを作っても後段で上書きされるだけで、手数を浪費する
  - 由来 1: 本 facet が `parse_transcript.py` / `analyze_transcript.py` をリポジトリ直下に残す事象が 2 か月で 3 回発生した (2026-06-29 / 08-14 / 09-05)。jj は新規ファイルを自動追跡するため、気づかないまま次のコミットへ混入する
  - 由来 2 (2026-09-25 insights): 編集禁止の step 73 セッション中 15 セッションで Write が実行されていた (Report Directory のほか `/tmp` 等リポジトリ外も)。旧文面が「書き出しが要る場合は `.takt/` 配下と最終レポートの保存先に限る」と書き込みを許す形だったことが一因
  - **この規約は保証ではない。** `cli-merge-pipeline` が post_steps 後に作業ツリーを確認して残骸を列挙する (todo13.md 順位 232 の (2))。指示は発生源を減らす層、検知が保証層である

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

**最初から `offset` / `limit` で分割して読む。全体を 1 回で Read しない。** 縮約後でも数百 KB〜1MB 超あり、1 回の Read の上限 (25,000 tokens、256KB) を超えて失敗する (2026-09-26 実測: 407,838 バイト / 394 行が約 37,700 tokens、1,272,209 バイト / 1,139 行はサイズ上限で拒否)。

- **100 行ずつを目安にする。** 実測では 1 行の中央値 432 バイト、最大 21.7KB で、100 行の区間は最も重いもので 140KB (約 13,000 tokens) だった。ただし保証ではない — 縮約で切り詰めるのは正常な `tool_result` だけで、エラーの `tool_result`・ユーザー発話・`tool_use` の入力は全文残り、1 行の長さに上限は無い
- **Read が上限で失敗したら、同じ `offset` で `limit` を半分にして読み直す。** 1 行ずつにしても失敗する行 (1 行だけで上限を超える) は読み飛ばし、その行番号を Report の末尾に「読めなかった行」として書く。欠落を黙って報告しない
- 特定の事象 (エラー、ユーザーの修正指示など) を探すときは、先に Grep で行を絞ってから該当範囲を Read する

各 entry のスキーマ (Rust 側で縮約済みの形):

```json
{
  "type": "user" | "assistant",
  "timestamp": "2026-04-25T05:44:35.040Z",
  "sessionId": "<uuid>",
  "message": {
    "role": "user" | "assistant",
    "content": [
      { "type": "text", "text": "..." },
      { "type": "tool_use", "name": "Bash", "input": {...} }
    ]
  }
}
```

注意:
- Rust 側で縮約済み。各 entry が持つのは `type` / `timestamp` / `sessionId` / `message` (`role` / `content`) だけで、`thinking` block (暗号化済みで本文は空) は除去されている
- **正常な `tool_result` の本文が 2,000 字を超える場合は、先頭 1,000 字と末尾 1,000 字だけが残り、間に `[… N 文字省略 …]` が入る。** 省略は Rust 側の意図した処理であり、データ欠落として報告しない。`is_error: true` の `tool_result`、ユーザー発話、`tool_use` の入力は全文残る
- 画像 block は `[画像省略]` の text block に置き換わっている
- `type: queue-operation` / `type: attachment` は Rust 側で除外済の想定だが、出現したら無視する
- 縮約後も数百 KB / 数百行になり得る。重要な箇所だけ要約する

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

## 出力言語

- **レポート本文は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンドはもちろん、**本 facet が出力する固定トークンも訳さない** — 完了条件の `analysis complete` (`post-merge-feedback.yaml` の `rules.condition` が英語リテラルで照合)、および Required output の section 見出しと表の列名
