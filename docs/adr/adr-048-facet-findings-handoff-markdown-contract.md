# ADR-048: reviewers→fix findings handoff の output-contract 標準化 (markdown 統一・JSON 却下)

## ステータス

試験運用 (2026-07-06)

> WP-07 (`docs/harness-improvement-plan.md`) の実装 ADR。計画の当初案 (findings の JSON 化) を
> takt 公式仕様の調査結果に基づき却下し、takt idiomatic な markdown output-contract の標準化に方針転換した記録。

## コンテキスト

WP-07 の目的は、pre-push review の **reviewers → fix 間の findings 受け渡し**で発生しうる
「parse 事故・読み落とし」を防ぐことである。当初計画 (`docs/harness-improvement-plan.md` の WP-07) は
次の 2 ステップを想定していた:

1. findings スキーマ定義 (file / line / severity / rationale / suggested_fix) を **JSON 化**。
2. Rust 側 (cli-push-runner) に schema 検証 pre-step を追加、parse 失敗時は markdown fallback。

この当初案は計画策定セッション (Claude Fable 5) が「未検証」と明記した部分であり、実装着手前に
takt の公式仕様・ベストプラクティスを調査した。

### 調査結果 (takt 0.35.3 公式仕様、ADR-017 で pin)

takt に同梱の公式スタイルガイド・スキーマ・builtins を精査した結果、以下が判明した:

- **takt の idiomatic な facet 間 handoff は markdown レポートファイル**である。step は
  `output_contracts.report[].format` で markdown レポートを出力し、実行ごとの `reports/` ディレクトリに
  保存され、後続 step が `{report:filename}` で参照する (`builtins/skill/references/engine.md`)。
- 公式 **`OUTPUT_CONTRACT_STYLE_GUIDE.md`** は output-contract を「必ず ` ```markdown ` コードブロックで
  囲む」ことを要求し、DO/DON'T 表で **「プレーンテキストで出力契約を書く」を DON'T** と明記している。
- output-contract のスキーマ (`schema-base.js` の `OutputContractItemSchema`) に `type` / `schema` /
  `json` フィールドは存在せず、`format` は**検証されない markdown テンプレート文字列**にすぎない
  (Phase 2 プロンプト末尾に verbatim 展開されるのみ、`ReportInstructionBuilder.js`)。
- **29 の builtin output-contract・36 の builtin workflow・251 の report 宣言はすべて markdown (`.md`)**。
  スキーマ・builtins・docs のいずれにも JSON producer/consumer は存在しない。JSON handoff は takt の
  流儀に反する。
- 構造化 findings の blessed な形は既に markdown テーブルで存在する: builtin `security-review` の
  finding テーブル (`finding_id / family_tag / Severity / Type / Location / Issue / Fix Suggestion`)。

### parse 事故の真因 (JSON でないことではない)

調査で判明した「読み落とし」の実体は「markdown だから」ではなく、**片方のレビュアーに output-contract が
存在せず、reviewer 間で finding テーブルの列が不統一**なことだった:

- `format: security-review` → takt builtin の finding テーブル契約に解決される (構造あり)。
- `format: simplicity-review` → **対応する contract ファイルが存在せず** (`.takt/facets/output-contracts/`
  にも builtin にも無い)、takt はリテラル文字列 `"simplicity-review"` に degrade する
  (`faceted-prompting/dist/resolve.js` の literal fallback)。結果、simplicity reviewer は
  **構造の強制がゼロ**で free-form 出力していた (実 run の `simplicity-review.md` が裏付け)。
- WP-06 で追加した `refutation-report` (project) は finding テーブルを持つが、列が上記と微妙に不統一。

### アーキテクチャ制約 (当初案の実現不能性)

当初案の「Rust 側 (cli-push-runner) に検証 pre-step」は**構造的に実現不可能**である:

- takt の step はすべて **LLM persona 呼び出し**であり、command/exec/script step 種別は存在しない
  (`workflow-schemas.js` の zod schema で確認)。純粋な Rust 検証ノードを workflow 内に置けない。
- cli-push-runner は takt workflow **全体を 1 つの不透明なサブプロセス** (`pnpm exec takt -w <workflow>`)
  として呼ぶ (`src/cli-push-runner/src/stages/takt.rs`)。`reviewers → fix` の handoff は takt 内部で
  起き、runner からは見えないため、その境界に pre-step を挟めない。

## 決定

当初の JSON 化を**却下**し、takt idiomatic な **markdown output-contract の標準化**で WP-07 の目的
(parse 事故・読み落とし防止) を達成する。

1. **`.takt/facets/output-contracts/simplicity-review.md` を新設**し、builtin `security-review` の
   finding テーブル構造を踏襲する。これにより simplicity reviewer にも構造が強制され、真因
   (片方に契約が無い) を直接解消する。`format: simplicity-review` は既存 workflow の記述のまま
   新規 project ファイルに解決されるため、**workflow YAML の変更は不要**。
2. **reviewer 間で finding テーブルの列を統一**する。canonical 列:
   `finding_id | family_tag | Severity | Type | Location (file:line) | Issue | Fix Suggestion`。
   security は builtin 契約 (同一列) をそのまま使用し、simplicity が同構造を mirror する。
3. **`refutation-report.md` の Survived Findings テーブルに `family_tag` 列を追加**し、reviewer 契約と
   整合させる (fix の family_tag ベース同時修正に資する)。`refute-finding.md` の carry-over 指示も更新。
4. 契約は公式スタイルガイド準拠 (` ```markdown ` ブロック + 認知負荷軽減ルール + `finding_id` 必須の
   Rejection Gate)。

### output-contract 設計原則 (次回 contract 追加・編集時の参照基準、2026-07-07 追記 / WP-07 feedback)

新規 output-contract を追加・編集する際は以下の 2 原則を守る (WP-07 で列不整合が CodeRabbit Major 指摘となった再発防止):

1. **全 finding セクションで同一列セット**: 1 contract 内の Current Iteration / Carry-over / Reopened 等の finding テーブルは同じ finding 列 (`finding_id` / `family_tag` / ...) を共有する。セクション間で列が漂流すると下流の fix / refute facet の parse を壊す。
2. **builtin `security-review` を mirror**: reviewer contract は takt builtin `security-review` の列構造・casing を踏襲する。casing の混在 (snake_case の field id `finding_id`/`family_tag` + Title Case の表示ラベル `Severity`/`Type`/...) は builtin 由来の**意図的な区別**であり「不整合」ではない (PR #252 で CodeRabbit が誤指摘)。意図的に mirror しない contract を作る場合は、その設計意図を contract 冒頭コメントに明記する。

適用範囲は **pre-push review の reviewers** (通常 `pre-push-review.yaml` と refute variant
`pre-push-review-refute.yaml` の両方が同じ `format:` 名を共有するため自動的に裨益)。post-pr-review は
simplicity/security reviewer を持たず (CodeRabbit findings 駆動の analyze/fix 構成) 対象外。

## 却下した代替案

- **JSON handoff (当初計画案)**: takt 公式スタイルガイドの明示的 DON'T に反し、builtin (全 markdown) や
  将来の takt アップグレードと不整合を生む。機械可読性の利得は、後述の理由で markdown テーブルでも
  十分得られる。→ 却下。
- **Rust 検証 pre-step を cli-push-runner に追加 (当初計画 step 2)**: takt が LLM step 専用かつ runner が
  workflow 全体を不透明に呼ぶため、reviewers→fix 境界に介入できない。→ 実現不能。
- **reviewers と fix の間に検証専用 takt movement を新設**: takt に非 LLM step が無いため LLM step を
  1 つ増やすことになり、コスト/レイテンシ増 + refute variant の verify step と干渉する。→ 却下。
- **markdown finding テーブルを parse する Rust validator を Bash 経由で fix から呼ぶ**: takt 自体は
  contract を検証しないため machine 検証層を自前で足す選択肢 (cli-finding-classifier の Bash 呼び出し
  前例あり)。有効だが、まず契約標準化で構造を揃える効果を観測し、必要なら follow-up とする。
  → 今回は見送り (順位化候補)。

## 帰結

### 利点

- simplicity reviewer に構造が強制され、reviewer 間で finding 列が統一される (真因の解消)。
- takt 公式の idiomatic な機構 (`output_contracts` + `{report:}`) の上に乗るため、builtin・将来の
  アップグレード・他 workflow と整合する。
- `finding_id` 必須の Rejection Gate により「finding_id 無しの finding」を契約レベルで無効化 (読み落とし
  防止)。
- ADR-020 (`fix.md` は入力形式非依存) の不変条件を維持 — fix は引き続き markdown レポートを読むだけ。

### 欠点 / 留意点

- takt は contract を機械検証しないため、reviewer が契約に**従わない**リスクは残る (LLM 出力の性質)。
  完全な machine 検証が必要なら Bash 経由 Rust validator を後日追加する (却下案参照)。
- security 契約は takt builtin に依存する。takt アップグレード時に builtin の列が変われば simplicity 側の
  mirror も追随が必要 (ADR-017 の pin により当面は安定)。

### ADR-039 との関係

本 ADR は prompt-contract の改善であり**ランタイム機能ではない**ため、ADR-039 の config opt-in /
kill-switch はそのままは適用されない。可逆性は「contract ファイルの revert (simplicity は free-form に
戻る)」で担保する。bounded lifetime として、dogfood 期間で reviewer findings が一貫した構造で出力され
parse 事故が減ることを確認したら `試験運用` を解除する。未達 (契約無視が頻発) なら Bash 経由 Rust
validator の追加を再検討する。

### 関連 ADR

- [ADR-020](adr-020-takt-facets-sharing.md) — facet 共有 (`fix.md` の入力形式非依存原則を維持)
- [ADR-036](adr-036-bundle-z-three-layer-review.md) — Bundle Z 3 層 review (reviewer 出力の構造化)
- [ADR-047](adr-047-prepush-refute-facet.md) — refute facet (`refutation-report` の列を本 ADR で整合)
- [ADR-017](adr-017-takt-version-pinning.md) — takt バージョン固定 (builtin 契約の安定性の前提)
