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
- [ ] 本エントリ削除 + todo-summary2.md 行削除

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
- [ ] 本エントリ削除 + todo-summary2.md 行削除

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
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 大きな transcript の PR でも session-analysis facet が token limit に衝突せず、Grep 避難措置なしで分析が完了すること。

---

### post-merge-feedback の分析ソース選定を対象 PR の commit/bookmark 照合ベースに修正

> **動機**: post-merge-feedback の `find_latest_prepush_reports_dir` と session transcript 選定が**時刻範囲のみ**で対象 PR を照合しないため、同日並行 push 運用 (#311/#312/#313) で他 PR の pre-push レポート・transcript を誤って分析ソースに取り込む。#311/#312 の feedback aggregation で実地検証済み (#311 の feedback に #313 の `summary_line_new_path` 等・現行 repo に不在のコードが混入)。post-merge-feedback の分析範囲欠陥として過去 3 回 recurrence した先行 todo と同型かつ、一部祖先未レビューでなく無関係 PR 知見の丸ごと誤帰属という**より深刻な形態**。
>
> **対処案**: `cli-merge-pipeline` の `context.json` 生成で、対象 PR の commit range / bookmark 名と pre-push run・transcript を突き合わせて選定する。不一致は該当 section を unverified 表示に落とす (fail-open な助言層)。先行 todo は `prepush_reports_dir` のみ言及だが、session transcript 側も同種の照合を要する。ADR-042 の decision framework では mechanizable=Yes (commit range 照合で構造的検出可) / FP 緩和=Yes (不一致は unverified 表示) で仕組み化が推奨される。
>
> **参照**: `.claude/feedback-reports/311.md` Tier1 #1、`.claude/feedback-reports/312.md` Tier2 #1、`src/cli-merge-pipeline/src/feedback/context.rs` (`find_latest_prepush_reports_dir` + transcript 時刻範囲フィルタ)。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (誤 PR への feedback 誤帰属 = データ整合性違反) / Frequency High (並行 push は記録済みの日常運用) / Effort M / Adoption Risk: runner 複雑化。#311 feedback は ✅ 採用候補・#312 feedback は 🤔 様子見と判定が割れたが、両者とも実害を実地確認済み。

#### 作業計画

- [ ] `context.json` 生成で対象 PR の commit range / bookmark と pre-push run・transcript を照合するロジックを追加
- [ ] 照合に外れた run/transcript は分析ソースから除外 or unverified 表示に落とす
- [ ] #311/#312 で観測した混入シナリオの回帰テストを追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 並行 push された PR の post-merge-feedback が、時刻範囲でなく対象 PR の commit/bookmark 照合で pre-push run・transcript を選定し、他 PR 知見の混入が起きないこと (混入シナリオの回帰テストで seal)。

---

### 並行テストで thread::spawn 結果を collect 後に判定するパターンを custom lint 強制

> **動機**: #312 で 8-thread stress test の遅延イテレータ (`map`/`filter`/`count`) が、まだ実行中のスレッドを傍から drop して「2 Acquired」偽陽性を生んだ実績あり (`pipeline_lock/tests.rs` で `Vec::collect` により回避)。`thread::spawn` は `lib-jj-helpers` / `cli-pr-monitor` / `hooks-stop-quality` 等 8 ファイルで使用され、同型の偽陽性が再発しうる。
>
> **対処案**: `.claude/custom-lint-rules.toml` に regex rule を追加し、`thread::spawn` 近傍で join 結果を遅延イテレータ chain に直結して判定する形を検出し `Vec::collect` を促す。false positive リスクは対象を concurrent test file 近傍に限定して軽減する。
>
> **参照**: `.claude/feedback-reports/312.md` Tier1 #1、`src/lib-jj-helpers/src/pipeline_lock/tests.rs` (collect 回避例とコメント)、`.claude/custom-lint-rules.toml`。
>
> **実行優先度**: 🔧 Tier 2 (analyzer の `Tier 1: Hooks/Linter` = mechanical enforcement のため `feedback_tier_classification` per project Tier 2 に再分類) — Severity Medium / Frequency Medium / Effort M / Adoption Risk: false positive (対象限定で軽減可能)。

#### 作業計画

- [ ] `thread::spawn` 近傍の遅延イテレータ判定 pattern を検出する regex rule を追加 (対象を concurrent test 近傍に限定)
- [ ] `rule_test_coverage_check` で positive/negative test を機械強制
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 並行テストで `thread::spawn` 結果を collect せず遅延イテレータで判定する pattern が pre-push / PostToolUse で決定論的に検出されること。

---

### CodeRabbit rate-limit format の fixture ライブラリ化 + 新世代 format 検出の定期 CI 検証

> **動機**: CodeRabbit は 2026-01 → 05 → 07 で 3 回 format を変更しており、新世代 format が silent に不適合を起こす drift を proactive に検知する仕組みが無い。#311 で ADR-049 準拠の実 incident fixture 化は実施済みだが、CI での定期検証まで拡張されていない。
>
> **対処案**: ADR-034 の既知 CR format 一覧を fixture 化し、`.github/workflows/coderabbit-format-check.yml` (新規) で本リポジトリの PR が得る実 CR walkthrough が既知 marker/regex いずれかにマッチすることを検証する。あわせて `check-ci-coderabbit` の `decide`/`rate_limit` に fixture tests を追加。新世代対応手順は ADR-034 の SOP 化 (別エントリ) と相補。
>
> **参照**: `.claude/feedback-reports/311.md` Tier1 #3、`adr/adr-034-coderabbit-auto-monitoring.md`、`src/check-ci-coderabbit/src/{decide,rate_limit}.rs`。
>
> **実行優先度**: 🔧 Tier 2 (analyzer の `Tier 1` だが ci_step = automation のため project Tier 2) — Severity Medium / Frequency Medium (3 世代実績) / Effort M / Adoption Risk None。本リポジトリは cargo test 用 CI 自体が未整備のため WP-16 (CI matrix) と連動して検討。

#### 作業計画

- [ ] ADR-034 の既知 CR format 一覧を fixture 化
- [ ] `coderabbit-format-check.yml` を新設し、実 CR walkthrough が既知 marker/regex にマッチするか検証
- [ ] 新世代 format 追加を fixture 更新と紐付け (ADR-034 SOP 化エントリと相補)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- CR format が既知 marker/regex いずれにもマッチしなくなった時点で CI が検知し、silent な format drift を land 前に捕捉できること。

---

### 3 世代 CR format × 4 parse path × CR state の複合マトリックステスト

> **動機**: CodeRabbit nitpick (`verdict_*_takes_precedence` 不足) は #311 の 7 テストで解消済みだが、**format 世代軸** (old / new / next / fallback の 4 parse path) × CR state の組合せ網羅は未実施で、新世代 format 追加時の回帰防止に不足がある。
>
> **対処案**: `check-ci-coderabbit/src/decide.rs` の `#[cfg(test)]` に parametrized matrix test を追加し、3 世代 CR format × 4 parse path × 主要 CR state の組合せを網羅する。
>
> **参照**: `.claude/feedback-reports/311.md` Tier2 #1、`src/check-ci-coderabbit/src/decide.rs`。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium (format 世代は今後も増える見込み) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] format 世代 × parse path × CR state の parametrized matrix test を追加
- [ ] 新世代 format 追加時に fixture を足す運用と紐付け
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 全 CR format 世代 × parse path × state の組合せがテストで固定され、新世代追加時のリグレッションを機械検知できること。

---

### decide.rs/main.rs の境界値・parameter threading テスト拡充

> **動機**: 前回 incident の根本原因は parameter threading の欠落 (`parse_rate_limit()` はするが `decide()` に渡さない) だった。同クラスのリグレッションを防ぐテストが、インシデント発生ドメイン (rate-limit 判定) 直下で不足している。positive evidence の複合シナリオ、呼び出し側 (`main.rs`) が `decide()` に `rate_limit` を正しく構成することの検証が未固定。
>
> **対処案**: `check-ci-coderabbit` の `decide.rs`/`main.rs` の `#[cfg(test)]` に、(a) rate-limit + critical finding 等の複合境界、(b) `main.rs` で `decide()` に `rate_limit` が正しく構成されること (呼び出し側が ignore しない) の単体テストを追加する。
>
> **参照**: `.claude/feedback-reports/311.md` Tier2 #2、`src/check-ci-coderabbit/src/{decide,main}.rs`。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (インシデントドメイン直下) / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `decide` の複合境界テスト追加 (positive evidence × rate_limit の組合せ)
- [ ] `main.rs` で `decide()` への `rate_limit` 構成を固定する単体テスト
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- parameter threading の欠落・呼び出し側 ignore を含む同クラスのリグレッションがテストで検知されること。

---

### Silent Fallback 排除原則を開発 convention に明文化

> **動機**: #311/#309 の実インシデント (rate-limit marker を検知しつつ wait 解析失敗で `None` に落ち「対象外」と誤認 = fail-open) の再発防止。自動 lint 化は意味論解析 (複数 parse 試行 + `Option` 返却の組合せ) が必要で false positive 過多のため却下され、人間向けガイドラインで担保する方針。
>
> **対処案**: `CLAUDE.md` の開発 convention に「外部 SaaS / ネットワーク API を parse する関数は `Option<T>` の曖昧返却 (検知失敗と対象外の同一化) を避け、失敗理由を enum で区別する。失敗時の default 挙動は parse 側でなく呼び出し側が明示選択する」を追記。#311 の「marker 一致 = 制限と判定し待機時間だけ既定埋め」を良い参考実装として cite。
>
> **参照**: `.claude/feedback-reports/311.md` Tier3 #1、`src/check-ci-coderabbit/src/rate_limit.rs` (参考実装)。
>
> **実行優先度**: 💎 Tier 3 — Severity High (実インシデント) / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `CLAUDE.md` 開発 convention に Silent Fallback 排除原則を追記
- [ ] `rate_limit.rs` の設計を参考実装として cite
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 外部 API parse 関数の設計時に「検知失敗 ≠ 対象外」を区別する原則が文書化され、レビュアー (人 / simplicity-review LLM) が参照できること。

---

### Positive Evidence Requirement を CLAUDE.md/ADR に明文化

> **動機**: #311 の incident は「commit status pass を review 実行と同一視」した fail-open が根因。外部システム監視で「成功の定義」を single source (commit status / exit code) に依存させない原則が未整備。今後の外部 tool 監視 (GitHub Actions status 拡張、Slack 通知読取り等 ADR-009/018/034 系列) にも適用可能。
>
> **対処案**: `CLAUDE.md` または新規 ADR に「外部システム監視実装時は成功の定義を明示し、commit status 等の単一ソースで充足させず陽性証拠を別途要求する」を明文化。#311 の `has_review_evidence` (commit status pass でも review 実行の陽性証拠を別要求) を参考実装として cite。
>
> **参照**: `.claude/feedback-reports/311.md` Tier3 #2、`src/check-ci-coderabbit/src/decide.rs` (`has_review_evidence`)、`adr/adr-034-coderabbit-auto-monitoring.md`。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort M / Adoption Risk None。

#### 作業計画

- [ ] Positive Evidence Requirement を `CLAUDE.md` または新規 ADR に明文化
- [ ] `has_review_evidence` を参考実装として cite
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 外部システム監視実装で「成功の定義」を単一ソースに依存させず陽性証拠を要求する原則が文書化されること。

---

### ADR-034 に「新世代 CR format 対応の SOP」セクション追加

> **動機**: CodeRabbit は 3 世代 format 変更実績があり、新世代対応の手順が明文化されていないと missed case のリスクがある。
>
> **対処案**: `adr/adr-034-coderabbit-auto-monitoring.md` に「新世代対応の SOP」節を追加する: (1) 観測時に既知 format table へ行追加 (出典 URL / discovered_date / marker / regex)、(2) 新 extract 関数追加 (テンプレート化、ADR-049 fixture 併設)、(3) 既存 test suite に新 fixture 追加、(4) ADR 更新を done 記録。format-check CI 化エントリと相補。
>
> **参照**: `.claude/feedback-reports/311.md` Tier3 #3、`adr/adr-034-coderabbit-auto-monitoring.md`。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] ADR-034 に新世代対応 SOP 節を追加 (table / extract 関数 / fixture / ADR 更新の 4 手順)
- [ ] format-check CI 化エントリの fixture 追加手順と紐付け
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 新 CR format 観測時の対応手順が SOP として明文化され、手順漏れ (missed case) を防ぐこと。

---

### 並行性バグの root cause 分析で推測を禁止し観測的再現を要求するルール追加

> **動機**: #312 の当初 doc comment は「128-bit token 衝突」という現実的に発生しない条件で root cause を誤って説明していた (3 回推論を外した後、atomic 計装で実測確定して修正)。誤った分析のまま fix すると再発防止にならず Severity High。
>
> **対処案**: `CLAUDE.md` または `docs/dev-conventions.md` に「並行性バグの root cause は推測 (could / might) でなく観測的再現 (race timeline / stress test failure / atomic 計装) で確定してから fix する」ルールを追加。pre-push gate 等の機械強制化は「推論か観測かの判定は semantic / NLP が必要 = 機械化不可」(ADR-042 Step1) に該当するため rule docs 化のみ (mechanism 化は見送り)。
>
> **参照**: `.claude/feedback-reports/312.md` Tier3 #1、`src/lib-jj-helpers/src/pipeline_lock.rs` (実測確定後の doc)、`docs/dev-conventions.md`。
>
> **実行優先度**: 💎 Tier 3 — Severity High (誤分析のまま fix = 再発防止にならない) / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] 並行性バグの root cause は観測的再現を要求するルールを `CLAUDE.md` / `dev-conventions.md` に追加
- [ ] #312 の「128-bit 衝突」誤説明 → atomic 計装での確定を実例として cite
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 並行性バグの修正が観測的再現に基づくことを convention が要求し、レビュアーが「推測ベースの root cause」を land 前に指摘できること。
