# Aggregate Weekly Review

3 つの whole-tree レビュー (simplicity / security / architecture) を統合し、週次レビューレポートと構造化 findings JSON を生成する。

ADR-031 § Findings スキーマ + § 採否フロー の input source として findings JSON を produce する設計。skill 側 (Phase C 予定) が JSON を読んで AskUserQuestion で採否を確認するため、本 facet は構造化データの単一 source。

**重要な原則:**

- 読み取り専用 (`edit: false`)。コードの修正は一切行わない (採否は Phase C skill とユーザー判断で行う)
- findings がない場合は「特筆事項なし」で正常終了する。無理に findings を捻出しない
- 重複する findings はマージし、`location` と `rationale` を統合する
- severity の自動配点を最終手段とせず、3 reports の articulation を尊重する
- **各 finding に Severity / Category を必須付与する** (ADR-031 § Findings スキーマ)

---

## Input

### Report Directory (takt が提供)

本 step (`pass_previous_response: false`) は前 step の response を受け取らない。代わりに Report Directory に保存された 3 reports を Read で読み取る:

- `simplicity-whole-review.md` — review-simplicity-whole facet の出力
- `security-whole-review.md` — review-security-whole facet の出力
- `architecture-whole-review.md` — review-architecture-whole facet の出力

### Context

実行日は本 step の wall clock を `YYYY-MM-DD` 形式で取得 (UTC でも JST でも一貫していればよい。findings id の prefix に使う)。

## Phase 1: 3 reports の統合

各 report の findings を抽出し以下のルールで統合する:

1. **重複検出**: 同じ `location.path` + 似た description / 同じ category の finding はマージする (3 facets 間で観点が重なるケースあり、例: simplicity が dead code、architecture が ADR-012 違反として同じ symbol を flag)
2. **rationale 統合**: マージした finding の rationale 部分に複数 facet (simplicity / security / architecture) を併記する
3. **severity 確定**: 各 finding の severity は facet が articulate した severity を尊重する。複数 facet が異なる severity を articulate した場合は **高い方** を採用する (例: simplicity が medium、security が high なら high)
4. **品質フィルタ** (最初から表に乗せない):
   - 一般的なベストプラクティスの押し付け (具体的な file / line evidence なし)
   - すでに hooks-config.toml / custom-lint-rules.toml / cli-docs-lint で機械的に検出される pattern (Read で確認可能)
   - 対象ファイルが read-only zone (`.takt/runs/`, `.claude/feedback-reports/` 等の generated artifact) のみで意味のある編集箇所が示せないもの

## Phase 2: 各 finding に Severity / Category / Recommendation を確定する

各 finding について以下の rubric に基づいて判定列を埋める。**この評価は採用判定をユーザーへ委ねるための材料**であり、AI が判定を独占しない。明確に判定できない場合は中庸な値 (`medium` / `🤔 様子見`) を選び、rationale で不確実性を明示する。

> **AI agent への明示禁則**: 本 report の生成完了 = ユーザーへの提示完了に過ぎず、Claude / Codex / Opencode 等の agent は `✅ 採用候補` を読んだだけで採用処理 (`docs/todo*.md` への entry 追加 / 実装着手 / ADR 編集 等) に進んではならない。**必ずユーザーの明示承認 (AskUserQuestion 回答 or テキスト承認のいずれか) を待つこと**。本 report の Recommendation 列は analyzer 推奨であり、確定判断ではない。

### Severity rubric (ADR-031 § Findings スキーマ準拠)

| 値 | 該当する状況 |
|---|---|
| `critical` | data loss / security 脆弱性 / 致命的バグ / production-down リスク |
| `high` | 機能 bug / silent failure / data integrity 違反 / systemic harness drift |
| `medium` | UX 低下 / 累積複雑度 / dead code / 局所 ADR drift |
| `low` | style / micro-optimization / docs typo |

### Category rubric

simplicity / security / architecture facets が emit する category を以下に正規化する:

- `harness-duplication` — rule / pipeline / hook 重複
- `adr-alignment` — ADR と実装の drift
- `docs-internal` — docs 間 cross-ref drift (cli-docs-lint で取れない meta pattern)
- `docs-source-drift` — docs と source の矛盾
- `module-boundary` — モジュール境界違反
- `cyclic-dep` — 循環依存
- `layer-violation` — レイヤ侵犯
- `adr-naming` — ADR-012 命名違反
- `test-anti-pattern` — TDD anti-pattern / 境界欠落
- `cumulative-complexity` — 累積複雑度
- `dead-code` — 未参照コード
- `overspec` — overspec'd abstraction
- `secret-exposure` — 機密漏出パターン
- `injection` / `auth-flaw` / `crypto-weak` / `unsafe-no-safety` / `path-traversal` / `prompt-injection` — security category

category が複数該当する場合は最も特徴的な 1 つを採用、補助 category は description で言及する。

### Recommendation rubric

3 種類のいずれかを必ず emit する:

| 値 | 該当する状況 |
|---|---|
| `✅ 採用候補` | `severity ∈ {medium, high, critical}` AND `category が systemic (= harness-duplication / adr-alignment / test-anti-pattern / secret-exposure 等)` AND `Adoption Risk が弱い`。**ユーザー承認後に採用確定**。 |
| `🤔 様子見` | 採用根拠は弱いが将来発生時に再評価したい (Severity 高だが location が局所的、Adoption Risk が中間、Phase C/D で再評価したい等)。✅ にも ❌ にも振り切れない場合の中庸 |
| `❌ 却下推奨` | `severity ∈ {low}` AND `category が局所 (= docs typo / style)` OR `(Adoption Risk が strong: 過剰一般化 / NLP 必要 / false positive リスク / takt test infra 未調査)` OR `(実害観測前の preventive over-engineering)`。**ユーザー承認後に却下確定** (Claude 単独で却下処理しない)。 |

## Phase 3: findings id 採番と JSON 生成

各 finding に id を採番:

- format: `WR-<YYYY-MM-DD>-<facet_initial><sequence>`
- facet_initial: `S` (simplicity) / `C` (security) / `A` (architecture) / `M` (multi-facet merged)
- sequence: 同 facet 内で 01 から連番 (`01` / `02` / ...)

例: `WR-2026-05-29-A03` = 2026-05-29 実行、architecture facet 由来、3 番目。

JSON は ADR-031 § Findings スキーマ準拠で `findings.json` というファイル名で write する (workflow の output contract では `name: findings.json` + `format: findings-json` として宣言されている — `findings.json` がファイル名、`findings-json` が契約 (format) 名):

```json
{
  "run_date": "2026-05-29",
  "report_path": ".claude/weekly-reviews/2026-05-29.md",
  "findings": [
    {
      "id": "WR-2026-05-29-A03",
      "facet": "architecture",
      "severity": "high",
      "category": "harness-duplication",
      "location": { "path": "src/foo.rs", "line_range": "120-145" },
      "description": "...",
      "proposal": "...",
      "decision": "pending",
      "recommendation": "✅ 採用候補",
      "rationale": "..."
    }
  ]
}
```

`decision` field は常に `pending` で出力 (Phase C skill が AskUserQuestion 経由で `adopted` / `rejected` / `deferred` に書き換える)。`recommendation` / `rationale` は analyzer 推奨を保持する補助 field。

## Phase 4: Markdown report 生成

Markdown は人間 / Claude が読む summary 層。findings table を severity 順 (critical → low) で並べ、facet ごとの観察メモを追記する。

### Required output (Markdown)

```markdown
## Weekly Review Report (<YYYY-MM-DD>)

### スコープ
- 対象ツリー: `src/` / `scripts/` / `.claude/` / `.takt/` / `docs/`
- レビューファセット: simplicity-whole / security-whole / architecture-whole
- 採否方針: Phase C skill `/weekly-review` で AskUserQuestion 経由

### 統合 findings

#### Severity: critical / high

| ID | Facet | Category | Location | Description | Proposal | Recommendation | Rationale |
|---|---|---|---|---|---|---|---|

#### Severity: medium

| ID | Facet | Category | Location | Description | Proposal | Recommendation | Rationale |
|---|---|---|---|---|---|---|---|

#### Severity: low

| ID | Facet | Category | Location | Description | Proposal | Recommendation | Rationale |
|---|---|---|---|---|---|---|---|

### Facet 観察メモ

- **simplicity-whole**: <observable patterns / クライテリア 0-3 で目立った傾向>
- **security-whole**: <observable patterns / hotspots>
- **architecture-whole**: <observable patterns / 観点 ① ハーネス遵守 + ② ③ sub criterion>

### 次のアクション

**重要**: 本 report の Recommendation 列はすべて analyzer の推奨であり、ユーザー明示承認なしに採用・却下を確定してはならない。Claude / 他 AI agent は report を読んだだけで `docs/todo*.md` への entry 追加、実装着手、ADR 編集等を実行してはならず、**必ずユーザー承認 (AskUserQuestion 回答 or テキスト承認のいずれか) を待つこと**。

- `✅ 採用候補`: Phase C skill `/weekly-review` での AskUserQuestion 採用、`docs/todo*.md` 系列への登録または直接実装
- `🤔 様子見`: Phase C/D の dogfood トリガで再評価、現時点で action なし
- `❌ 却下推奨`: ユーザー承認後に却下確定、`docs/todo*.md` への登録不要 (Claude 単独で却下処理しない)
- このレポートは `.claude/weekly-reviews/<run_date>.md` に保存される (`.gitignore` 除外、内部 artifact)
- 構造化 findings は `findings.json` として並置保存される (Phase C skill 入力)
```

findings がゼロ severity (= 該当 finding なし) の section は省略する。

findings 全体がゼロの場合は以下を出力:

```markdown
## Weekly Review Report (<YYYY-MM-DD>)

### スコープ
- 対象ツリー: `src/` / `scripts/` / `.claude/` / `.takt/` / `docs/`
- レビューファセット: simplicity-whole / security-whole / architecture-whole

特筆すべき findings なし。3 facet いずれも whole-tree レビューで blocking concern を発見しませんでした。

決定論層 + diff-local レビュー + post-pr-review が現状の coherence を保っている状態と解釈できます。
```

最後に `aggregation complete` で終了する。
