# 順位 8 (週次レビュー Phase B) 着手用 引き継ぎ資料

> **本ファイルの位置付け**: 試験運用 ephemeral 計画書。順位 8 の Phase B 〜 Phase C land 完了で役割を終え、retire 時に永続価値 (= reusable rationale / pattern 等) は ADR-031 本体 / `docs/adr/` / `~/.claude/rules/common/` に移管する (グローバル `docs-governance.md` § Retirement Workflow 適用)。
>
> **作成日**: 2026-05-27 (PR #177 land 直後、コンテキスト圧迫により別セッション引き継ぎのため作成)。
>
> **対象 task**: docs/todo-summary.md 順位 8「週次レビュー (ADR-031) Phase B 実装」。詳細 entry は `docs/todo.md` § 「週次プロジェクト全体レビューパイプラインの導入 (ADR-031 起案 + 実装)」(line 219 周辺)。

## 1. ゴールと scope

ADR-031 で設計済の **週次プロジェクト全体レビューパイプライン** の Phase B (takt workflow + 4 facets + persona) を実装する。Phase A (ADR-031 起案) は land 済 ([docs/adr/adr-031-weekly-review-pipeline.md](adr/adr-031-weekly-review-pipeline.md))。

**MVP 構成 (3 facets、本リポジトリで合意済 2026-05-26)**:

- `review-simplicity-whole` (whole-tree、ADR-027 制約解除)
- `review-security-whole` (whole-tree)
- `review-architecture-whole` (新 persona、ADR 整合性 / モジュール境界 / ADR-012 命名 / 循環依存)
- `aggregate-weekly` (3 reports → findings JSON + markdown)

並列構成: 3 review facets を `parallel:` block → `aggregate-weekly` で統合 ([post-merge-feedback.yaml](../.takt/workflows/post-merge-feedback.yaml) 構造流用、fix loop は不要)。

## 2. 7 観点責務 mapping (2026-05-26 AskUserQuestion 経由ユーザー合意)

facet 数は増やさず prompt 重点配分で対応。MVP 優先観点は **① ハーネス遵守 + ⑥ テストロジック**。

| 観点 | 担当 facet | prompt 重点 |
|---|---|---|
| ① ハーネス遵守 (rule < pipeline < hook 重複) | architecture-whole | **MVP 最優先** — facet criteria の筆頭、rule/pipeline/hook 重複検出、順位 146-151 Bundle 既存ルール仕組み化の継続的発見源 |
| ② docs 内整合性 | architecture-whole の sub criterion | ADR 間 supersedes / cross-reference / todo routing、順位 95 / 96 と補完 |
| ③ docs-source 矛盾 | architecture-whole の sub criterion | 重要 ADR 限定リスト (ADR-007 / 012 / 021 / 022 等) で context 圧迫回避 |
| ④ セキュリティ | security-whole | ADR-031 設計通り、変更なし |
| ⑤ Todo 妥当性 | **MVP 対象外** (順位 136 land 済 hook へ委譲) | hook = 編集時 immediate guard / 週次 = batch 棚卸し で責務分離。Phase B+1 で順位 154 facet として再評価 |
| ⑥ テストロジック (振る舞い vs 実装詳細、境界) | simplicity-whole | **MVP 最優先** — facet criteria の筆頭、TDD anti-pattern + 境界欠落、順位 38 (cargo-mutants L3 weekly) と cross-validate |
| ⑦ ファイルサイズ (50KB) | aggregate 前の Rust 機械 pre-step (Phase B+1) | facet 不要、機械検査で十分。順位 154 で順位 95 / 147 と scope 整理 |

## 3. 依存タスク現状

| 順位 | タスク | 状態 | 順位 8 着手への影響 |
|---|---|---|---|
| **136** | working copy staleness 検出 hook 2 段構え | ✅ **land 済 (PR #177)** | 観点 ⑤ 責務分離が完成、Phase B MVP の 6 観点 scope が clean |
| 20 | ADR-032 PR-β 実装 (compensating check) | ⚠️ 未着手 (ADR-032 自体が未起案) | 着手前提として entry が言及しているが hard blocker ではない。順位 8 完了後の Phase E (dogfood + 本採用判断) で順位 20 整合性を見直す程度で OK |
| 38 | cargo-mutants L3 weekly | ⚠️ 未着手 | bundle 化推奨だが必須ではない。Phase B land 後、Phase B+1 (順位 153 / 154 と並列) で着手判断 |
| **95** | preamble file count 自動照合 CI | ⚠️ 未着手 | **着手前推奨** (順位 8 着手前の docs 機械整合性層、案 A プラン残り) |
| **96** | Markdown cross-reference validator CI | ⚠️ 未着手 | **着手前推奨** (順位 8 着手前の docs 機械整合性層、案 A プラン残り) |

**推奨実装順序**:

1. 順位 95 + 96 を bundle 化して land (案 A プラン残り、観点 ② docs 内整合性 を機械層で先行確保) — **本セッションで未着手、別セッションで先行推奨**
2. 順位 8 Phase B 着手 (本資料の主目的)
3. Phase B dogfood 2-3 週運用 (試験運用 flag、ADR-039 bounded lifetime)
4. Phase B+1 (順位 38 / 153 / 154 のいずれか or bundle、dogfood 結果次第)

## 4. Phase B 実装計画 (ADR-031 + todo.md 順位 8 entry より)

### Phase B 工程 (PR 2 として実装、PR 1 = Phase A は land 済)

1. **`architecture-reviewer` persona 定義**
   - allowed_tools: Read / Glob / Grep のみ (`edit: false`、ADR-022 原則 1 準拠)
   - knowledge: architecture
   - 既存 persona の場所を調査 (`.takt/personas/` または config 内) して同様に追加
2. **`.takt/facets/instructions/review-simplicity-whole.md`** 新規作成
   - 既存 `review-simplicity.md` から派生コピー
   - diff 局所制約を whole-tree 向けに改変 (主要 dir Glob 順読、累積複雑度視点)
   - **観点 ⑥ テストロジック (TDD anti-pattern + 境界欠落) を criteria 筆頭に配置** (MVP 重点)
3. **`.takt/facets/instructions/review-security-whole.md`** 新規作成
   - 既存 `review-security.md` から派生、whole-tree 版
4. **`.takt/facets/instructions/review-architecture-whole.md`** 新規作成
   - 観点: ADR 整合性 / モジュール境界 / ADR-012 命名規約 / 循環依存 / レイヤ侵犯
   - **観点 ① ハーネス遵守 (rule/pipeline/hook 重複検出) を criteria 筆頭に配置** (MVP 重点)
   - 観点 ② docs 内整合性 / ③ docs-source 矛盾 を sub criterion に組込 (重要 ADR 限定リストで context 圧迫回避)
5. **`.takt/facets/instructions/aggregate-weekly.md`** 新規作成
   - 既存 `aggregate-feedback.md` を参考に、3 reports を統合し finding JSON + markdown を出力
6. **`.takt/workflows/weekly-review.yaml`** 新規作成
   - `parallel: [simplicity-whole, security-whole, architecture-whole]` → `aggregate-weekly` の 2 step
   - [post-merge-feedback.yaml](../.takt/workflows/post-merge-feedback.yaml) の構造をテンプレート流用
7. **takt 単体 dry-run 検証**
   - `takt run weekly-review.yaml` で 4 レポートが `.takt/runs/<ts>-weekly-review/reports/` に生成されることを確認
8. **PR 作成・マージ** (本資料の対象範囲)

### Phase C 工程 (PR 3 として実装、Phase B と別 PR 推奨)

skill `/weekly-review` + SessionStart hook reminder の実装。詳細は [docs/todo.md 順位 8 entry](todo.md) § Phase C 参照。

### Phase D 〜 E 工程

e2e 検証 + dogfood (Phase B/C merge 後)。詳細は同 entry 参照。

## 5. 重要な設計判断 (順位 8 entry 内 「ユーザー判断記録 (本タスク策定時に合意済 — 2026-04-27)」より)

| 質問 | 回答 |
|---|---|
| トリガー方式 | 手動 `/weekly-review` + SessionStart hook reminder (前回実行から 7 日経過で promote)、強制起動なし |
| レビュー対象スコープ | 毎回ソースツリー全体、サブツリー分割は MVP 不要 |
| 承認フロー | レポート提示 → 採否を一括選択 (pending JSON 経由) |
| Architecture facet 実装 | 新 `architecture-reviewer` persona 作成 |
| アーキテクチャ形態 | hybrid (takt workflow + skill)、ADR-030 の 3 層分離パターン継承 (4 例目) |
| PR 分割 | PR 1 (ADR、land 済) → **PR 2 (takt、本資料対象)** → PR 3 (skill + hook) → PR 4 (dogfood + 本採用判断) |
| 失敗ポリシー | best-effort (`.failed` marker + SessionStart hook reminder で再実行誘導、must-run ではない) |
| アンチパターン | whole-tree 用 facet を diff 用 facet と共通化しない (ADR-027 で diff 局所が本質要件のため separation 必須) |

## 6. 参照リソース

- **本体 ADR**: [docs/adr/adr-031-weekly-review-pipeline.md](adr/adr-031-weekly-review-pipeline.md) (Phase A、land 済)
- **todo entry**: [docs/todo.md](todo.md) § 「週次プロジェクト全体レビューパイプラインの導入」(line 219 周辺、7 観点責務 mapping table 含む)
- **summary table 行**: [docs/todo-summary.md](todo-summary.md) 順位 8
- **関連 ADR**:
  - [ADR-027](adr/adr-027-push-review-simplicity-focus.md) (push-time = simplicity 限定、本 ADR が補完する空白の根拠)
  - [ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) (post-pr-review 責務範囲)
  - [ADR-020](adr/adr-020-takt-facets-sharing.md) (facets 共通化判断基準)
  - [ADR-022](adr/adr-022-automation-responsibility-separation.md) (`edit: false` 方針、副作用範囲)
  - [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (3 層分離パターンの 3 例目、本 ADR は 4 例目)
- **参考実装 (構造流用元)**:
  - [.takt/workflows/post-merge-feedback.yaml](../.takt/workflows/post-merge-feedback.yaml) (analyze 3 並列 → aggregate 構造、本 Phase B の workflow テンプレ)
  - [.takt/facets/instructions/aggregate-feedback.md](../.takt/facets/instructions/aggregate-feedback.md) (aggregate facet 参考)
  - [.takt/facets/instructions/review-simplicity.md](../.takt/facets/instructions/review-simplicity.md) (派生元、whole-tree 版とは **共通化不可** — 別物として派生コピー必須)
  - [.takt/facets/instructions/review-security.md](../.takt/facets/instructions/review-security.md) (同上)
- **Phase B+1 follow-up entries**:
  - [docs/todo9.md](todo9.md) 順位 153 (`review-harness-whole` facet、観点 ① 独立 facet 化)
  - [docs/todo9.md](todo9.md) 順位 154 (`review-todo-whole` facet + aggregate 前 file size pre-step、観点 ⑤ ⑦ 拡張)

## 7. 適用すべき memory rule (運用上の重要 constraint)

- **`feedback_test_dry_antipattern`**: テストで DRY を適用しない、各 variant 独立 fixture (Phase B での facet test 実装時に適用)
- **`feedback_review_severity_auto_fix`**: Critical/High/Major は無条件自動修正 (PR review で発見された場合)
- **`feedback_coderabbit_no_actionable_merge_signal`**: CR が "No actionable comments were generated in the recent review. 🎉" 表示でユーザーに最終確認、追加 wakeup 停止
- **`feedback_pnpm_push_permission`**: pnpm push は foreground 実行 OK、`pnpm create-pr` / `pnpm merge-pr` は auto mode でも実行前にユーザー許可必須
- **`feedback_global_config_backup`**: ~/.claude/* に触る前に snapshot 取得 (本 Phase B は repo 内のみで完結、適用不要見込み)
- **`feedback_no_unenforced_rules`**: mechanical 検知できないルール案は即却下 (Phase B では機械強制可能な layer に限定)
- **`feedback_pipeline_over_rules`**: パイプライン設計で機械的に解決を優先 (本 Phase B 自体がこの方針の体現)
- **`feedback_skill_flow_user_scope`**: skill のデフォルト flow よりユーザー指示の scope が優先 (Phase B では skill 起動なし、takt workflow のみ)
- **`feedback_no_empty_change_before_push`**: jj describe 後そのまま pnpm push する、空 @ を挟まない

## 8. Auto mode + ユーザー preference pattern (Bundle 1-3 + 順位 136 で実証)

- **CR Major/High auto-fix**: ヒアリングなしで即修正、AskUserQuestion options に「修正しない」を含めない
- **CR Minor**: AskUserQuestion で判断確認 (ユーザーは過去 4 回連続「修正する」を選択、auto-mode でも明示確認推奨)
- **CR Nitpick (💤 Low value)**: skip してマージが推奨パターン (PR #176 で実証)
- **CR rate-limit パターン**: time-throttle (~30 分) のみなら待機 → `@coderabbitai review` で trigger / credit 枯渇なら即 merge 判断
- **bookmark 自動命名**: 自動採番 OK
- **`pnpm merge-pr`**: ユーザーから「`<bookmark-name>` ブランチをマージしてください」or 「マージ可」の明示テキスト承認が必要 (AskUserQuestion 答えだけでは sandbox 拒否されるケースあり)

## 9. 推奨 first action (新セッションで)

1. **本資料を読む** (この document)
2. **ADR-031 を読む** ([docs/adr/adr-031-weekly-review-pipeline.md](adr/adr-031-weekly-review-pipeline.md))
3. **todo.md 順位 8 entry を読む** (line 219 周辺、特に「7 観点責務 mapping」表)
4. **既存 facet 構造を確認** (`.takt/facets/instructions/` の現状確認、特に `review-simplicity.md` / `review-security.md` / `aggregate-feedback.md`)
5. **persona 配置場所を調査** (`.takt/personas/` または config、grep で探す)
6. **順位 95 + 96 が land 済かを確認** (未 land なら先に bundle land 推奨、land 済なら順位 8 直接着手)
7. **Phase B 実装着手** (上記 § 4 工程順、PR diff target 250-800 行を意識して fit するか確認)
8. **着手前に AskUserQuestion で MVP 範囲の最終確認** (3 facets で start vs 5 facets + pre-step、ユーザーは Bundle 1-3 land で patterns 確立済のため 3 facets 推奨想定)

## 10. PR diff 想定規模

ADR-031 entry の 「Effort: 中-高」+ 本資料 § 4 の 8 工程 = ~600-900 行想定 (facets 3 + workflow 1 + persona 1 + aggregate 1 = 6 files、test と config を加味)。PR diff target 250-800 行を超える場合は Phase B 自体を 2 PR に分割検討 (例: PR 2-A = persona + workflow + 2 facets / PR 2-B = 残り facets + aggregate)。

## 11. 本資料の retirement 条件

順位 8 (Phase B) が land し、Phase C / D / E のいずれも着手判断が ADR-031 + todo entry で十分 trackable な状態になったタイミングで本資料を retire:

1. **永続価値の移管**: 7 観点責務 mapping は既に todo.md 順位 8 entry に codify 済 (重複)。本資料 § 5 ユーザー判断記録は ADR-031 に未記載なら追記
2. **残タスクの確認**: Phase C / D / E 移行時に follow-up が残っていれば todo.md に登録
3. **永続参照リンクの除去**: 本資料への永続参照が無いことを `grep -rn 'handoff-rank-8' docs/` 等で確認
4. **削除**: 本ファイル (`docs/handoff-rank-8-weekly-review-phase-b.md`) を物理削除

retire 候補時期: Phase B land 直後 (Phase C / D は別 PR で独立追跡可能のため、Phase B 完了で本資料の役割は終わる)。
