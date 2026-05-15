# ADR-039: Experimental feature 標準パターン (config opt-in + kill-switch + bounded lifetime)

## ステータス

試験運用 (2026-05-10)

## コンテキスト

本プロジェクトでは試験運用 ADR が systemic に蓄積している (本表は **lineage = 過去の試験運用 ADR の網羅列挙**、本 ADR を land する際の遡及 cross-link は **本 ADR では行わない** = §帰結 「欠点 / 留意点」参照):

| ADR | 試験対象 | 開始 |
|---|---|---|
| [ADR-014](adr-014-post-merge-feedback.md) | post-merge-feedback ループ | 2026-04-22 |
| [ADR-023](adr-023-coderabbit-reject-thread-skill.md) | CodeRabbit reject thread skill | — |
| [ADR-025](adr-025-cwd-restore-drop-guard.md) | CwdRestore Drop guard | — |
| [ADR-029](adr-029-post-merge-feedback-auto-trigger.md) | Post-Merge Feedback 自動起動 | — |
| [ADR-030](adr-030-deterministic-post-merge-feedback.md) | takt 経由の同期実行 | — |
| [ADR-031](adr-031-weekly-review-pipeline.md) | 週次プロジェクト全体レビュー | 2026-04-27 |
| [ADR-033](adr-033-todo-numbering-simplification.md) | todo 採番管理の簡素化 | — |
| [ADR-034](adr-034-coderabbit-auto-monitoring.md) | CodeRabbit 監視自動化 | — |
| [ADR-036](adr-036-bundle-z-three-layer-review.md) | Bundle Z 3 層 review | — |
| [ADR-037](adr-037-takt-fix-trust-shortcut.md) | takt fix-trust shortcut | — |
| [ADR-038](adr-038-local-llm-finding-classification.md) | ローカル LLM finding classification | 2026-05-06 |

> **本 PR で back-link を追加した範囲**: 上記 11 ADR のうち、**ADR-031 / ADR-036 / ADR-038** の 3 件のみに本 ADR への blockquote 参照を冒頭に追加した (**§ 既存試験運用 ADR で観測される共通パターン** で適合状況を分析した 3 ADR)。残り 8 ADR への遡及更新は **後続 PR で個別追補** とする (§ 帰結 / 欠点 参照)。

各試験運用 ADR は個別判断で導入されてきたが、PR #123 (ADR-038 Phase 5: P-0 classifier opt-in + §10 ブランチ分離運用) の post-merge-feedback で、**3 点セット** (config opt-in / kill-switch / bounded lifetime) が systemic に反復していることが確認された (Tier 3 #1 採用)。

### 既存試験運用 ADR で観測される共通パターン

| 観点 | ADR-031 | ADR-036 | ADR-038 |
|---|---|---|---|
| **Config opt-in** | 週次トリガはデフォルト disabled | gate を flag で制御 | `[lint_screen] enabled = false` (default OFF) |
| **Kill-switch** | レビューパイプ停止可能 | gate 経路を revert で停止 | revert PR で `enabled = false` |
| **Bounded lifetime** | 「採用判定で本採用に昇格」 | dogfood 完了で判定 | ADR-038 採用昇格 = 2026-05-15 (Phase D 6 PR / 9 data points で採用条件充足) |

3 点とも個別 ADR で都度設計されてきたが、**新規試験運用 ADR を策定するたびに同じ判断を再発明している**。

## 決定

試験運用 feature を導入する際の **標準パターン** として 3 点セットを以下の通り規定する。新規試験運用 ADR は本 ADR を **参照** し、3 点を満たすことを default とする。

### 1. Config opt-in (デフォルト無効)

- 設定ファイル (`*.toml`) または env var で `enabled = false` をデフォルトとする
- 明示有効化 (`enabled = true`) で feature 発動
- env var / config 値での切り替えを必ず提供 (config-only より env override 可能な方が望ましい)
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc 等) への deploy 時にも default OFF が継承されるよう、`[feature]` section の追加を必須化

### 2. Kill-switch (停止経路の事前明文化)

- revert PR で `enabled = false` に戻す経路を **PR body / ADR で明文化**
- crate / module の物理削除は **dogfood 失敗判定後にまとめて実施**。途中段階での部分削除は機械的損傷の risk が高い
- ADR-038 §10.6 の C 案 (採用 / 簡易版 / 完全版の階層化) が良いテンプレ
- kill-switch 経路の table を ADR / PR body に必ず含める (項目: 起動経路 / 停止コマンド / 影響範囲)

### 3. Bounded lifetime (試験期限と採否判定基準)

- 試験期限を **ADR 冒頭** または **計画書冒頭** に明記
  - 例: 「6 ヶ月経過しても採用判定未達なら却下とみなす」
  - 例: 「3-5 PR で dogfood 後に採否判定」(ADR-038 / Phase d)
- retirement workflow (`~/.claude/rules/common/docs-governance.md`) との接続を明示
  - **採用**: 試験運用 → 本採用に昇格 (新規 ADR 不要、本 ADR の status 更新)
  - **却下**: revert PR で feature 削除 + 本 ADR を「却下」に更新 + 計画書 (`docs/<topic>-analysis.md`) を retirement workflow で削除
  - **継続**: 期限内に判定が出ない場合、計画書側に新たな期限と判定基準を記述 (1 回まで延長可)
- bounded lifetime を欠いた試験運用は「永遠の試験運用」化し、累積複雑度の温床になる

## 帰結

### 利点

- 新規試験運用 ADR の判断が標準化され、設計議論の重複が削減される
- kill-switch 経路の事前明文化で、dogfood 失敗時のロールバックが decision-free に進む
- bounded lifetime で試験運用の「忘却された負債化」を防ぐ

### 欠点 / 留意点

- 既存試験運用 ADR (014/023/025/029/030/031/033/034/036/037/038) の 3 点セット適合状況は再評価対象。本 ADR の land 後、各 ADR の reflect は **後続 PR での追補** として進める (本 ADR では遡及更新しない)
- 本 ADR 自体も試験運用扱い: 3-5 個の新規試験運用 ADR で本パターンを適用し、適合率と運用負荷を確認後に本採用に昇格する

### 想定される運用

新規試験運用 ADR (例: 仮称 ADR-040) を策定する際は ADR 冒頭近くに以下を記載:

```markdown
## ステータス

試験運用 (YYYY-MM-DD)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。
```

PR body にも kill-switch table を含める (起動経路 / 停止コマンド / 影響範囲)。

## 関連

- [ADR-031](adr-031-weekly-review-pipeline.md) — 試験運用、3 点セット部分適合
- [ADR-036](adr-036-bundle-z-three-layer-review.md) — 試験運用、3 点セット部分適合
- [ADR-038](adr-038-local-llm-finding-classification.md) — 試験運用、3 点セット完全適合 (本 ADR の trigger 事例)
- `~/.claude/rules/common/docs-governance.md` — Document Lifecycle Classification / Retirement Workflow
- `~/.claude/CLAUDE.md` — グローバルルール index
