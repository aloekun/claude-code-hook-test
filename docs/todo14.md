# TODO (Part 14)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に到達したため、新規エントリは本ファイルに記録する (2026-07-19 週次レビュー WR-2026-07-19-T02 採用)。**新規エントリの追加先は本ファイル**。todo.md / todo2.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立 (2026-07-20 に todo13.md→todo15/16/17・todo10.md→todo18/19 の物理分割で todo15-19 を新設)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### VSCode 拡張が hook `systemMessage` を UI 描画するかの調査 (ADR-059 dogfood / 削除条件 2)

> **動機**: [ADR-059](adr/adr-059-hook-system-message-visibility.md) (systemMessage 可視化) の dogfood で、2026-07-19 に PR-N1〜N3 を land し reminder 起点で weekly review を実行したが、**VSCode 拡張環境では systemMessage の 1 行が UI に独立描画されたか確証が持てなかった** (観測できたのは additionalContext 経由のモデル言及のみ)。VSCode 拡張が hook の `systemMessage` をターミナル CLI と異なる扱いにしている可能性がある。ADR-059 の bounded-lifetime 判定 (期限 2026-08-16) と `docs/weekly-review-notification-plan.md` 削除条件 2 の前提であり、未確認のままでは段階展開の採否も計画書削除も判断できない。
>
> **対処案**: (1) **ターミナル CLI 版 Claude Code で新セッションを起動**し systemMessage が UI 描画されるか切り分ける (CLI で出るなら実装は正しく、VSCode 固有の表示差と特定できる)、(2) VSCode 拡張での描画有無・スタイルを確認、(3) 結果を ADR-059 § Dogfood 観測 (2026-07-19) に追記し 2026-08-16 判定 (第 2 弾展開 or 却下) の材料にする。描画されない場合も additionalContext 明示指示 (defense-in-depth) が backstop のため**実装は revert しない**。
>
> **参照**: [ADR-059 § Dogfood 観測 (2026-07-19)](adr/adr-059-hook-system-message-visibility.md)、`docs/weekly-review-notification-plan.md` (削除条件 2)、`src/hooks-session-start/src/main.rs` (`build_session_start_json` = systemMessage 出力元)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (ADR-059 bounded-lifetime 判定と計画書削除の blocker) / Frequency Low (一度切り分ければ済む) / Effort S (CLI で新セッション起動 + 目視)。期限 2026-08-16 に間に合うよう実施。

#### 作業計画

- [ ] ターミナル CLI 版 Claude Code で新セッションを起動し systemMessage の描画を確認 (last-run を stale にするか failed marker を置いて reminder を発火させる)
- [ ] VSCode 拡張での描画有無・スタイルを確認し CLI との差を切り分け
- [ ] 結果を ADR-059 § Dogfood 観測 (2026-07-19) に追記 + 削除条件 2 の可否を判定
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- VSCode 拡張 (と CLI) で hook `systemMessage` が描画されるかが切り分けられ、ADR-059 削除条件 2 の判定 (計画書 `docs/weekly-review-notification-plan.md` の削除可否) が下せること。

---

### docs/todo*.md 本文の順位番号表記を検出する custom lint rule (ADR-033 使用禁止の仕組み化)

> **動機**: [ADR-033](adr/adr-033-todo-numbering-simplification.md) (2026-04-29 試験運用) が「絶対番号は table のみに保持し、本文中の順位番号表記は使用禁止」と規定し、「将来の展望」節で pre-push hook の custom_lint_rule 追加を検討済みと明記したが、未実装のまま約 3 ヶ月経過。#303 の CodeRabbit 対応でも本文参照の drift が問題化した文脈。#303 post-merge feedback で採用。
>
> **対処案**: `.claude/custom-lint-rules.toml` に regex rule を追加し、`docs/todo*.md` の本文 (table 行を除く) に残る順位番号の literal 表記を検出する。ADR-033 の検証用 grep が既に動作実証済みのため rule 化の Effort は S。既存の literal-ban 系 custom rule (rule⑥/⑪) と同型。
>
> **参照**: `.claude/feedback-reports/303.md` Tier1 #2、[ADR-033](adr/adr-033-todo-numbering-simplification.md) (§ 将来の展望)、`.claude/custom-lint-rules.toml`。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None (ADR-033 で既に禁止規定 + 検証 grep 実証済み)。

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に `docs/todo*.md` 本文の順位番号表記を検出する regex rule を追加 (table 行を除外)
- [ ] 既存本文の違反を洗い出し修正 (ADR-033 の grep を流用)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `docs/todo*.md` 本文に順位番号表記が混入した場合、pre-push / PostToolUse で決定論的に検出されること (ADR-033 の規定が仕組みで強制される)。

---

### post-merge-feedback の transcript 分析を cli-merge-pipeline 生成の summary index に置換

> **動機**: post-merge-feedback の session-analysis facet が、大きな transcript (#303 マージ時は約 1.5MB / 427 行) で 25K token limit に衝突し、Grep + 手動パースの避難措置を要した (aggregate 工程の自己観測)。cli-merge-pipeline は既に transcript filter を実施済みのため、index 出力の追加は自然な拡張。#303 post-merge feedback で採用。
>
> **対処案**: cli-merge-pipeline の Phase 0 (transcript filter) で summary index (timestamp / message_type / tool_name / outcome) を事前生成し、session-analysis facet の入力を raw transcript からこの index に置換する。token limit 衝突を構造的に回避。
>
> **参照**: `.claude/feedback-reports/303.md` Tier2 #1、`src/cli-merge-pipeline` (Phase 0 transcript filter 出力)、`.takt/facets/instructions/analyze-session.md` (消費側 facet)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency High (毎回のマージ feedback で発生し得る) / Effort M / Adoption Risk None (既存 filter の自然な拡張)。

#### 作業計画

- [ ] cli-merge-pipeline の Phase 0 で transcript summary index を生成 (timestamp / message_type / tool_name / outcome)
- [ ] session-analysis facet の入力を index に切替 + token 消費が threshold 内に収まることを確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 大きな transcript の PR でも session-analysis facet が token limit に衝突せず、Grep 避難措置なしで分析が完了すること。
