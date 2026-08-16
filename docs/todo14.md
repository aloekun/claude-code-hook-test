# TODO (Part 14)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に到達したため、2026-07-19 週次レビュー WR-2026-07-19-T02 採用時に新設した。**本ファイル自体も約 70KB に到達したため新規エントリは追加しない** — 2026-08-04 以降の新規エントリは [docs/todo20.md](todo20.md) へ記録していたが、todo20.md も 50KB 超過で 2026-08-08 以降は todo21.md、その todo21.md も 50KB 超過で 2026-08-11 以降は todo22.md へ移った。**現在の追加先は [docs/todo.md](todo.md) preamble の routing 表が正である** (2026-08-16 時点は todo24.md)。todo.md / todo3.md 〜 todo22.md の既存エントリは引き続き有効、相互に独立 (todo2.md は 2026-08-12 退役) (2026-07-20 に todo13.md→todo15/16/17・todo10.md→todo18/19 の物理分割で todo15-19 を新設)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### cli-telemetry-report コード堅牢化 + 回帰テスト (月次 ROI レビュー PR #336/#337 post-merge feedback 採用)

> **動機**: 月次ハーネス ROI レビュー実装 (PR #335-338) の post-merge feedback で cli-telemetry-report に決定論的な堅牢化余地が挙がった。特に Phase C (snapshot 保持) の根本シナリオ = 月中に無効化→翌月再有効化される機構が既存 3 テストで未カバー (High)。Phase D の `confirmed_streak = zero_streak - partial` は前提 (`partial ⇒ zero_streak≥1`) が別関数依存で局所防御が無く、将来のリファクタで release build の silent underflow wrap により誤った deactivation 判定を招き得る。加えて `zero_firing_list` の MD/JSON 二重計算は出力乖離の温床。
>
> **対処案**: 下記 作業計画。いずれも既存テストモジュール / 局所コードへの追加で Effort 小。
>
> **参照**: PR #337 (Phase C+D) / PR #336 (Phase A) / PR #338 (Phase E)、`.claude/feedback-reports/337.md` (Tier1 #1 / Tier2 #1,#2)、`.claude/feedback-reports/336.md` (Tier1 #1 / Tier2 #1)、`.claude/feedback-reports/338.md` (Tier2 #2)、[ADR-062](adr/adr-062-monthly-harness-roi-review.md)、`src/cli-telemetry-report/src/{aggregate,verdict,report,registry}.rs`。
>
> **実行優先度**: 🔧 Tier 2 — Severity High〜Low (越境テストのみ High) / Frequency Low〜Medium / Effort S〜XS / Adoption Risk None。

#### 作業計画

- [ ] [High] aggregate.rs: 月中 無効化→翌月 再有効化トグルが `resolve_snapshot` を正しく通過する月間境界越境テストを追加 (#337 Tier2-1)
- [ ] verdict.rs: `confirmed_streak` 計算を `checked_sub` ベースの明示処理にし release build でも underflow を防止、`debug_assert!` (`partial ⇒ zero_streak≥1`) は診断用に併設 (#337 Tier1-1 + CodeRabbit PR #339)
- [ ] verdict.rs tests: `current_month_partial==0` の境界を明示テスト化し `confirmed_streak==zero_streak` 等価を保証 (#337 Tier2-2)
- [ ] report.rs: `zero_firing_list` の共有計算を `render()` で一度だけ実行し MD/JSON フォーマッタへ結果を渡す構造に抽出 (#336 Tier1-1)
- [ ] report.rs tests: MD/JSON 両出力で zero_firing (id集合 / provenance / last_fired_month) + source_failures 一致の回帰テスト (#336 Tier2-1)
- [ ] registry.rs tests: rule / preset / hook の 3 供給源が空・欠落時に同一 fail-open 挙動 (source_failures 計上) をすることを 1 テストで統一検証 (#338 Tier2-2、実装本体は #336 で対応済)

#### 完了基準

- 月中トグルシナリオ・streak 不変条件 (release-mode での判定保証を検証する回帰テストを含む)・MD/JSON 一致・3 供給源 fail-open が回帰テストで固定され、`cargo test --workspace` / clippy を通過すること。

---

### telemetry 時間語義・不変条件・degraded 運用の文書補強 (ADR-062 / CLAUDE.md)

> **動機**: 月次 ROI レビュー実装で判明した「snapshot は実行時点の状態≠対象期間の状態」という時間語義の混同 (Phase C の根本原因) と、streak 不変条件 (`partial ⇒ zero_streak≥1`) が未文書化。また「main root からの monthly review は常に degraded、回避は secondary workspace 実行」という運用事実が ADR-062 amendment / memory / SKILL.md の 5+ 箇所に分散し、PR #335-338 で反復的に扱われた (Frequency High)。
>
> **対処案**: 下記 作業計画。いずれも Effort XS の文書追記。採用後は分散していた degraded 記述を ADR 参照に集約可能。
>
> **参照**: `.claude/feedback-reports/337.md` (Tier3 #1,#2)、`.claude/feedback-reports/338.md` (Tier3 #1)、[ADR-062](adr/adr-062-monthly-harness-roi-review.md)、`CLAUDE.md`。
>
> **実行優先度**: 🔧 Tier 3 — Severity Medium〜Low / Frequency Medium〜High / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] CLAUDE.md に「Snapshot は実行時点の状態であり対象期間の状態ではない」を明示する Temporal Semantics セクションを新設 (日次ロールアップ等の同型バグ予防) (#337 Tier3-1)
- [ ] ADR-062 に streak 不変条件 (`current_month_partial=true ⇒ zero_streak≥1`) を追記 (verdict の debug_assert と対) (#337 Tier3-2)
- [ ] ADR-062 に帰結節を新設し「main root からの monthly review は常に degraded、回避は secondary workspace 実行」を集約 (分散記述を ADR 参照に一本化) (#338 Tier3-1)

#### 完了基準

- snapshot 時間語義・streak 不変条件・degraded 運用が ADR-062 / CLAUDE.md に一元化され、分散記述が ADR 参照に集約されること。

---

### jj workspace/bookmark semantics の文書化 + pr-monitor 回帰テスト

> **動機**: PR #335-338 の 5 段階 stacked push で、jj bookmark の `@` semantics (並行操作での位置変化・shared store 挙動) と、stacked commit で `@` が最上段にあると下段 bookmark を見失う pr-monitor 検出限界が反復観測された。既存 memory (`pr-monitor-bookmark-detection-pitfalls` / `parallel-workspace-shared-store-changes-under-you`) に続き 3 件目の類似事象で systemic pattern と判断。
>
> **対処案**: 状態図での文書化 + 既知限界の回帰テスト固定化。検出ロジック改善 (副作用リスクあり) は本エントリでは扱わず、まず限界を test で seal する。
>
> **参照**: `.claude/feedback-reports/336.md` (Tier2 #2 / Tier3 #2)、`.claude/feedback-reports/337.md` (Tier3 #3)、`.claude/feedback-reports/338.md` (Tier2 #1)、memory `pr-monitor-bookmark-detection-pitfalls` / `parallel-workspace-shared-store-changes-under-you`、`src/cli-pr-monitor/`、`docs/dev-conventions.md`。
>
> **実行優先度**: 🔧 Tier 2〜3 — Severity Medium / Frequency High / Effort S〜M / Adoption Risk None。

#### 作業計画

- [ ] docs/dev-conventions.md に jj bookmark / workspace の `@` semantics (並行操作での位置変化・shared store 挙動) を状態図形式で整理 (#336 Tier3-2 + #337 Tier3-3)
- [ ] cli-pr-monitor: stacked commit + 複数層 bookmark 時の PR 検出限界を regression test で固定化 (既知の false negative を明示記録、検出改善はスコープ外) (#336 Tier2-2 + #338 Tier2-1)

#### 完了基準

- jj bookmark semantics が dev-conventions.md に構造化され、pr-monitor の stacked-commit 検出挙動が回帰テストで固定されること。

---

### 開発ワークフロー規約の補強 (polling 禁止 / CodeRabbit→ADR timing)

> **動機**: `polling-anti-pattern` hook は稼働中だが dev-conventions.md に [ADR-016](adr/adr-016-long-running-command-strategy.md) / [ADR-018](adr/adr-018-pr-monitor-takt-migration.md) への導線が無く、本セッションでも block 後に手探りで代替パターンを発見した。また PR #338 で「設計決定 4 の ADR 未記載」を merge 直前の simplicity reviewer 監査で発見し E commit で急遽補足した (類似事象 #336 / #338)。外部レビュー指摘対応の ADR 反映タイミング規約が無い。
>
> **対処案**: dev-conventions.md / CLAUDE.md への軽量な規約・導線追加 (仕組みは既存の hook が担い、本エントリは導線と timing 規約のみ、ADR-042 整合)。
>
> **参照**: `.claude/feedback-reports/337.md` (Tier3 #4)、`.claude/feedback-reports/338.md` (Tier3 #4)、[ADR-016](adr/adr-016-long-running-command-strategy.md) / [ADR-018](adr/adr-018-pr-monitor-takt-migration.md)、`docs/dev-conventions.md`、`CLAUDE.md`。
>
> **実行優先度**: 🔧 Tier 3 — Severity Medium / Frequency Medium / Effort XS〜S / Adoption Risk None。

#### 作業計画

- [ ] docs/dev-conventions.md に polling 禁止 + `run_in_background` 必須の規約を追加し ADR-016/018 への参照リンクを付与 (block 前の自己解決を促進) (#337 Tier3-4)
- [ ] CLAUDE.md に「CodeRabbit 等の外部レビュー指摘への実装対応は、ADR への反映を完了させてから PR を merge する」timing 規約を追加 (#338 Tier3-4)

#### 完了基準

- polling 代替パターンへの導線が dev-conventions.md に整備され、外部レビュー指摘の ADR 反映タイミング規約が CLAUDE.md に明記されること。
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
> **実行優先度**: 🔧 Tier 2 (analyzer の `Tier 1` だが ci_step = automation のため project Tier 2) — Severity Medium / Frequency Medium (3 世代実績) / Effort M / Adoption Risk None。CI matrix は `adr/adr-065-ci-matrix-cross-os-regression.md` で整備済 (PR / master push で両 OS の `cargo test` が回る) のため、定期検証の載せ先はこの workflow を土台にできる。

#### 作業計画

- [ ] ADR-034 の既知 CR format 一覧を fixture 化
- [ ] `coderabbit-format-check.yml` を新設し、実 CR walkthrough が既知 marker/regex にマッチするか検証
- [ ] 新世代 format 追加を fixture 更新と紐付け (ADR-034 SOP 化エントリと相補)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- CR format が既知 marker/regex いずれにもマッチしなくなった時点で CI が検知し、silent な format drift を land 前に捕捉できること。

---

### decide.rs/main.rs の境界値・parameter threading テスト拡充 (#311 post-merge feedback 採用)

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

---

### deploy 時の exe/config feature 互換性診断 (内容ベース、mtime 不使用)

> **動機**: deployed `.claude/*.exe` が古く、tracked config (`.claude/hooks-config.toml`) が要求する新 feature (例: `{{CLAUDE_DIR}}` プレースホルダー展開) を満たさないと、silent `command not found` で quality gate が誤 block する。本セッションで 2 回実観測 (PR #307 の `{{CLAUDE_DIR}}` 機能追加時、2026-07-20 WP-15 rebase 時、いずれも MEMORY.md 記録済)。PR #310 post-merge feedback Tier1 #1 で採用。
>
> **対処案**: deploy step で exe 埋め込みバージョン文字列と config 側 `min_exe_version` フィールドを**内容ベースで比較**する診断チェックを追加する。**mtime 比較は使わない** — jj tracked config は `jj workspace add`/checkout で mtime がリセットされ偽陽性/偽陰性を生む (既知の mtime-staleness 問題と同型)。将来的に [ADR-051](adr/adr-051-cross-system-config-coupling.md) の隣接領域として ADR 化も検討可。
>
> **参照**: `.claude/feedback-reports/310.md` Tier1 #1、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、`.claude/hooks-config.toml`、`scripts/deploy-artifacts.mjs`。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (silent command-not-found で quality gate 誤 block) / Frequency Medium (2 回実観測) / Effort M / Adoption Risk None (mtime 回避設計であれば)。

#### 作業計画

- [ ] exe にビルドバージョン文字列を埋め込み、`.claude/hooks-config.toml` に `min_exe_version` フィールドを追加
- [ ] deploy step (`scripts/deploy-artifacts.mjs` or cli-merge-pipeline の deploy 処理) で内容ベースの互換性チェックを実装 (mtime 不使用)
- [ ] 互換性違反時に silent でなく明確なエラーで停止することを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- config が要求する feature を満たさない古い exe が deploy されている場合、内容ベース比較で検出され silent `command not found` にならないこと。

---

### pre-merge checklist に「Deferred Tests Completed」ブロッカー項目を追加

> **動機**: PR #310 自体が workflow_dispatch スモークテスト等の検証を post-merge に defer しており、defer した検証の実施漏れリスクが実在する。PR #310 post-merge feedback Tier2 #2 で採用。
>
> **対処案**: 配置先は `docs/dev-conventions.md` (CLAUDE.md から責務分離済みの既存運用 convention・チェックリスト集、順位261/262 と同型構成) に一本化する。新規ファイル `docs/pre-merge-checklist.md` の起こしは行わず、`CLAUDE.md` (ADR index 専用、チェックリストは非搭載方針) への直接追記も行わない。`docs/dev-conventions.md` に「Deferred Tests Completed」ブロッカー項目の見出しを新設し、defer した検証 (workflow_dispatch スモーク等) の実施をマージ前に確認する運用として位置付ける。
>
> **参照**: `.claude/feedback-reports/310.md` Tier2 #2、[docs/todo17.md](todo17.md) の pr-monitor 重複ガード dogfood タスク (defer した workflow_dispatch 検証の追跡先)、`docs/dev-conventions.md` (配置先、既存チェックリスト集)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に「Deferred Tests Completed」ブロッカー項目の見出しを新設 (`pre-merge-checklist.md` の新規作成・`CLAUDE.md` への追記は行わない)
- [ ] defer した検証 (workflow_dispatch スモーク、pr-monitor 重複ガード dogfood 等) を必須チェック項目として明示的に列挙し、完了基準と対応付ける
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- defer した検証がマージ前に checklist で可視化され、実施漏れが防止されること。

---

### CodeRabbit findings が空のとき fix commit 生成を skip

> **動機**: CodeRabbit findings が空でも fix commit が生成され abandoned になる挙動を PR #310 monitor で実観測した。空 commit → abandon は workflow noise / レビュー時の不確実性という UX 劣化を招く。PR #310 post-merge feedback Tier2 #3 で採用。
>
> **対処案**: **actionable findings が 0 の場合** (findings が空、または全 findings が non-actionable〔nitpick / informational のみ〕) に fix commit 生成・abandon をスキップする。actionable 判定を作業計画とテストで明示する。該当コードパス (cli-merge-pipeline または該当 takt fix step) は実装時に再調査が必要。
>
> **参照**: `.claude/feedback-reports/310.md` Tier2 #3、`src/cli-merge-pipeline` (post_merge_feedback / fix state 処理周辺)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None (該当コードパスは実装時に再調査)。

#### 作業計画

- [ ] 空 fix commit を生成しているコードパスを特定 (cli-merge-pipeline / takt fix step)
- [ ] findings が空、または actionable findings が 0 (全 non-actionable) の場合に commit 作成・abandon をスキップするよう修正 (actionable 判定を明示)
- [ ] 「findings 空」と「全 non-actionable」の両ケースをテストスコープに追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- CodeRabbit findings が空、または全 findings が non-actionable のとき、空 fix commit が作成されず abandon 処理も走らないこと (両ケースを回帰テストで seal)。

---

### CodeRabbit marker / GitHub event state の統合契約 doc + ADR-042 実例追記

> **動機**: PR #310 の pre-push simplicity review が新 gate を「internally consistent」と評価した一方、marker format 変更時の無音失敗リスクが PR analysis で指摘された。CodeRabbit の marker 文字列 (summarize / rate-limited 等) と GitHub event state fields への依存が複数箇所に散在している。PR #310 post-merge feedback Tier3 #1 で採用。
>
> **対処案**: `docs/integration-contracts/coderabbit.md` (新規) に marker 文字列と event state fields の統合契約を集約し、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) に本 PR (#310) を deterministic primary gate + advisory fallback パターンの具体例として参照追記する。
>
> **参照**: `.claude/feedback-reports/310.md` Tier3 #1、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)、`.github/workflows/pr-monitor.yml` (marker/state 依存の実装)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/integration-contracts/coderabbit.md` 新設 (marker 文字列 + event state fields の契約)
- [ ] [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) に本 PR を deterministic-primary/advisory-fallback の実例として参照追記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- CodeRabbit marker / event state に依存する箇所が契約 doc に集約され、marker 変更時の影響範囲が追えること。

---

### pr-monitor.yml に state semantics / if 式 / hardening 意図のインラインコメント追加

> **動機**: 本セッション中に pr-monitor.yml の `state == 'open'` guard が「redundant」と誤認される、折り畳まれた多条件 if 式が誤読される、という混乱が 2 件 (ローカルレビュアー) 実発生した。PR #310 post-merge feedback Tier3 #2 で採用。
>
> **対処案**: `.github/workflows/pr-monitor.yml` の `if:` 近傍に (a) `issue_comment` は closed/merged PR でも fire するが `pull_request_review` はしないという GitHub Actions state semantics、(b) 折り畳まれた多条件 if 式の sub-condition 一覧、(c) additive restriction + prompt guard の fallback demote という hardening パターンの設計意図、をコメントとして追記する。
>
> **参照**: `.claude/feedback-reports/310.md` Tier3 #2、`.github/workflows/pr-monitor.yml` (PR #310 で追加した決定論ガード)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None (実害根拠あり)。

#### 作業計画

- [ ] pr-monitor.yml の `if:` 近傍に (a) state semantics (b) if 式 sub-condition 一覧 (c) hardening 意図のコメントを追記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- pr-monitor.yml の `if:` 条件の意図が inline で追え、state guard / if 式の誤読が起きにくいこと。

---

### 新 config directive と要求最小 exe version の CHANGELOG/FEATURES 記録

> **動機**: 本 batch の deploy 互換性診断エントリ (exe/config feature 互換性チェック機構) と対になる human-readable な契約が無い。新 config directive (`{{CLAUDE_DIR}}` 等) 導入時に要求される最小 exe version が記録されておらず、stale-exe を 2 回実観測した。PR #310 post-merge feedback Tier3 #3 で採用。
>
> **対処案**: `CHANGELOG.md` (新規) または `docs/FEATURES.md` に、新 config directive と要求される最小 exe version を human-readable に記録する。本 batch の内容ベース互換性チェック (機構) と対で運用する。
>
> **参照**: `.claude/feedback-reports/310.md` Tier3 #3、[docs/todo14.md](todo14.md) の deploy 互換性診断エントリ (対となる互換性チェック機構)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk (派生プロジェクト deploy の配布作業レベル、weak)。

#### 作業計画

- [ ] `CHANGELOG.md` 新設 or `docs/FEATURES.md` に config directive + 要求最小 exe version を記録
- [ ] deploy 互換性診断の機構と対で参照できるよう相互リンク
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 新 config directive 導入時に要求 exe version が human-readable に記録され、deploy 側が参照できること。

---

### local LLM review の network 分離制約を明記し unverifiable finding を skip 運用

> **動機**: PR #310 で local LLM review (network 分離) が「summarize マーカーは実在しないかも」という false positive を報告し、author が手動で live PR (#304/#307) の生 body を確認して否定した実例がある。local LLM は live PR body / CodeRabbit marker の存在確認を行えない。PR #310 post-merge feedback Tier3 #4 で採用。
>
> **対処案**: local-review / review-local スキルの運用 doc (新規) または関連 ADR に、network 分離で live 検証ができない制約を明記し、該当 finding を "unverifiable locally" としてスキップする運用を記述する。**注**: 提案原文の Target「ADR-038」は finding-classifier 用 ADR であり本件 (pre-push local LLM diff reviewer) とはスコープが異なるため、実装時に正しい対象へ修正する。
>
> **参照**: `.claude/feedback-reports/310.md` Tier3 #4、review-local / local-review スキル運用 doc。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] review-local / local-review スキル運用 doc に network 分離制約 + "unverifiable locally" skip 運用を明記 (正しい対象 ADR/doc を確認して追記)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- local LLM review が live 検証不可能な finding を "unverifiable locally" として扱う運用が doc 化され、false positive 由来の混乱が減ること。

---

### フェーズ完了時の plan doc → ADR 転記照合チェックリストを dev-conventions.md に追加

> **動機**: PR #333 (Phase 4) で計画文書 `docs/monthly-harness-roi-review-plan.md` の 3 user-decisions + 5 design-decisions (計 8 項目) + 検証観点を ADR-062 へ手動 transpose した際、プラン doc にしか無かった決定が漏れかけ、Phase 4 の照合で発見・補完した。本 repo は 60+ の ADR を Phase 1〜4 等の多段階で運用しており、plan→ADR 同期漏れは今後の phase-completion で再発が見込まれる。#333 post-merge feedback Tier3 #2 で採用。
>
> **対処案**: `docs/dev-conventions.md` に「フェーズ完了 (plan doc 削除) 前に、計画文書の実装決定事項 (user-decisions / design-decisions / 実装上の決定) がすべて最終設計文書 (ADR) に転記済みかを 1 項目ずつ照合する」チェックリスト規約を追加する。lint 化は非現実的 (ADR ごとに記述形式が異なる) だが、既存の番号付きチェックリスト規約 (順位261/262/274 等) と同型で overhead 最小。
>
> **参照**: `.claude/feedback-reports/333.md` Tier3 #2、`docs/dev-conventions.md` (既存チェックリスト集)、[ADR-062](adr/adr-062-monthly-harness-roi-review.md) (Phase 4 で本照合を実施した実例)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に plan→ADR 転記照合チェックリストを追加 (照合単位 = user-decisions / design-decisions / 実装上の決定、plan doc 削除前に全項目の ADR 記載を確認)
- [ ] ADR-062 / Phase 4 を実例として cite
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- フェーズ完了 (plan doc 削除) 前に、plan doc の実装決定が ADR に漏れなく転記されているかを convention が要求し、レビュアーが参照できること。

---

### ADR amendment 時の「§ Amendment」節追加を dev-conventions.md のチェックリストに追加

> **動機**: PR #332 で ADR-062 が ADR-053/055/061 を amend し、PR #333 でも ADR-053/061 への追記を手動で実施したが、被 amend 側 ADR への追記が都度アドホックに行われている。CLAUDE.md の ADR 索引には既に (Supersedes/Superseded by) 注記が多数あり、Amendment 明記の convention 化は低コストで一貫性向上に資する。#332 post-merge feedback Tier3 #2 で採用。
>
> **対処案**: `docs/dev-conventions.md` (または CLAUDE.md convention セクション) に「ADR が他 ADR を override/amend する場合、被 amend 側 ADR に § Amendment セクションを追加し双方向リンクを張る」チェックリスト項目を追加する。
>
> **参照**: `.claude/feedback-reports/332.md` Tier3 #2、`docs/dev-conventions.md`、[ADR-062](adr/adr-062-monthly-harness-roi-review.md) / ADR-053 / ADR-055 / ADR-061 (amendment 実例)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に ADR amendment 時の § Amendment 追加 + 双方向リンクのチェックリストを追加
- [ ] ADR-062 の amendment 群を実例として cite
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- ADR が他 ADR を amend する際、被 amend 側への § Amendment 追記漏れを convention で防げること。

---

### todo ファイル削除・更新時のチェックリストを dev-conventions.md に追加

> **動機**: PR #332 で todo16.md の複数セクション削除時に lint:md を 3 回以上再実行する非効率を観測した。todo ファイルの段階的削除と都度 lint:md 実行の手順が明文化されておらず、削除漏れ・lint 崩れ・summary 行との不整合が起きやすい。#332 post-merge feedback Tier3 #8 で採用。専用スクリプト化 (#332 Tier2 #1) は ADR-033 効果待ちで様子見だが、チェックリスト明記自体は Effort XS の無リスク即応策として独立採用可能。
>
> **対処案**: `docs/dev-conventions.md` に「todo ファイルの削除・更新時は (1) 詳細エントリ (todoNN.md) と summary 行 (**該当順位を収める `docs/todo-summary.md` または `docs/todo-summary2.md`**。順位 220 未満は前者) を対で更新、(2) 段階的に削除し都度 lint:md で整合確認、(3) 削除する順位を指す本文参照を残さない ([ADR-033](adr/adr-033-todo-numbering-simplification.md) § アンチパターン)」のチェックリストを追加する。
>
> **2026-08-16 更新**: 当初の対処案 (3) は「順位番号を本文に書かない」だったが、[ADR-033](adr/adr-033-todo-numbering-simplification.md) § 改訂 が本文参照の禁止を緩和したため、**削除済み順位への参照を残さない**へ置き換えた。相補関係にあった順位番号 lint rule のタスクは同日 retire している。
>
> **参照**: `.claude/feedback-reports/332.md` Tier3 #8、`docs/dev-conventions.md`、[ADR-033](adr/adr-033-todo-numbering-simplification.md)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に todo ファイル削除・更新チェックリストを追加 (詳細/summary の対更新・段階削除+都度 lint:md・削除済み順位への本文参照を残さない)
- [ ] 本エントリ削除 + 該当順位を収める summary index (`docs/todo-summary.md` または `docs/todo-summary2.md`) の行削除

#### 完了基準

- todo ファイルの削除・更新時に段階削除と整合確認の手順が checklist 化され、削除漏れ・lint 崩れ・summary 不整合が防止されること。

---

### 新規スキル作成チェックリストを dev-conventions.md に追加

> **動機**: PR #332 で monthly-review skill を新規作成した際、weekly-review skill を都度参照して構造 (SKILL.md / evals.json / trigger_eval.json の 3 点セット、Phase 構成、deploy 前 sync check) を確認する手戻りを観測した。3 点セット要件を明記したチェックリストがあれば都度の参照往復を削減できる。#332 post-merge feedback Tier3 #9 で採用。
>
> **対処案**: `docs/dev-conventions.md` (スキル開発 convention セクション) に「新規スキル作成時は (1) SKILL.md / evals.json / trigger_eval.json の 3 点セット、(2) Phase 構成、(3) deploy 前の /skill-sync-check、を満たす」チェックリストを追加する。
>
> **参照**: `.claude/feedback-reports/332.md` Tier3 #9、`docs/dev-conventions.md`、skill-sync-check スキル、weekly-review / monthly-review skill (構造 template)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に新規スキル作成チェックリスト (3 点セット / Phase 構成 / deploy 前 sync check) を追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 新規スキル作成時に 3 点セット等の必須要素が checklist で確認でき、template skill への参照往復が削減されること。

---

### weekly/monthly staleness 判定の共通 fixture parametrized test を追加

> **動機**: PR #331 で追加した `monthly_review.rs` の staleness 判定ロジックは `weekly_review.rs` と逐語的に重複しており (`last_run_state_from_content` / staleness 判定 / main-root canonical / 未来 timestamp 等)、片方だけの独立バグ修正で挙動が乖離するリスクがある。#331 post-merge feedback Tier2 #1 で採用。
>
> **対処案**: weekly/monthly 両流路の staleness 判定を、同一 fixture (threshold 境界・Missing・Stale・Unreadable・未来 timestamp・main-root canonical 等) で検証する parametrized test を追加する。両モジュールの inline `#[cfg(test)] mod tests` に配置する (PR report が示した `src/tests/` は不在で、実態は inline test module)。既存 test パターン踏襲のみで Effort S。
>
> **参照**: `.claude/feedback-reports/331.md` Tier2 #1、`src/hooks-session-start/src/monthly_review.rs` / `weekly_review.rs` (inline test module)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] weekly/monthly の staleness 判定を同一 fixture で検証する parametrized test を追加 (threshold 境界 / Missing / Stale / Unreadable / 未来値 / main-root canonical)
- [ ] 片方だけのロジック変更で乖離が検出されることを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- weekly/monthly の staleness 判定が同一 fixture で検証され、片方のロジック変更による挙動乖離がテストで検知されること。

---

### CLAUDE.md の ADR index ステータスタグと ADR 本体ステータスの整合チェックを追加

> **動機**: PR #340 で CLAUDE.md の ADR-047 index タグが `*(試験運用)*` のまま、ADR-047 本体のステータス「却下 (2026-07-19 確定)」と乖離して残存していることを、pre-push simplicity review と post-merge 分析が独立に指摘した (実害継続を Read で確認済み)。index タグと本体ステータスの整合は手動更新に依存しており、ステータス遷移 (試験運用 → 採用/却下) のたびに再発しうる。#340 post-merge feedback Tier1 #1 で採用。
>
> **対処案**: CLAUDE.md の ADR index 行のステータスタグと、対応 ADR ファイル本体の「ステータス」見出しの一致を検証する doc-consistency チェックを pre-push 経路に追加する。[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の正規表現層/AST 層はいずれも単一ファイル起点設計のため、`custom-lint-rules.toml` への追加ではなく独立チェック (cli-docs-lint 拡張 or 専用スクリプト/test) として実装する。**責務分界 (PR #341 CodeRabbit 指摘で明文化)**: 本 entry はステータスタグ整合のみを扱い、採番重複/索引存在/番号一致は順位 272 の責務。実装は同一 validator module への同居が可能で相補。
>
> **参照**: `.claude/feedback-reports/340.md` Tier1 #1、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md)、[ADR-047](adr/adr-047-prepush-refute-facet.md)、順位 272 (同居実装候補)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort M / Adoption Risk None。

#### 作業計画

- [ ] CLAUDE.md の ADR-047 タグを `*(試験運用)*` → `*(却下)*` に修正 (実残存の不整合解消、着手時の即修正)
- [ ] ADR index タグと ADR 本体「ステータス」見出しの整合チェックを実装 (cli-docs-lint 拡張 or 独立スクリプト、順位 272 と同居検討)
- [ ] pre-push 経路 (lint:docs) への組込みと、不整合 fixture での検知確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- CLAUDE.md の ADR index タグと ADR 本体ステータスの乖離が pre-push で機械検知されること (採番/索引存在/番号一致の検知は順位 272 の完了基準で扱い、本 entry の対象外)。

---

### Cross-File Reference Lifecycle (ephemeral→permanent 移行手順) を dev-conventions.md に明文化

> **動機**: PR #340 の計画書スリム化で、CodeRabbit から「WP-14 の永続移管先未記載」「外部 SaaS 事実の移管方針」「WP-02 の todo 移管先未記録」の 3 件が指摘された。ephemeral 計画文書から permanent 成果物への知識移行の手順は「見送り」ケース限定の順位 261 convention にしか存在せず、完了/委譲ケースの移管先明記が規約の空白だったことが構造要因。#340 post-merge feedback Tier3 #1 で採用。
>
> **対処案**: `docs/dev-conventions.md` の順位 261 convention (spike 見送り 3 点セット) を拡張し、Cross-File Reference Lifecycle として明文化する: (1) permanent 成果物を先に作成・validate、(2) permanent→ephemeral 方向の参照を除去し、移管先 (ADR / todo 順位 / crate doc 等) を ephemeral 側の状態列に明記 (完了/委譲/見送りの全ケース対象)、(3) 計画文書の退役条件 (全状態確定 + 永続成果物からの参照ゼロ + 残タスクの lifecycle 整合) を含める。
>
> **参照**: `.claude/feedback-reports/340.md` Tier3 #1、`docs/dev-conventions.md` (順位 261 convention)、`docs/harness-improvement-plan.md` (退役手順の実例)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に Cross-File Reference Lifecycle の checklist を追加 (順位 261 convention の拡張として整理)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- ephemeral 計画文書の完了/委譲/見送りいずれのケースでも、永続移管先の明記と参照方向の規律が checklist で確認できること。

---

### WP-16 系 post-merge feedback 文書系 10 件の docs バッチ (dev-conventions 集中)

> **動機**: PR #342 (CI matrix / ADR-065)・#343 (監視 CI 観測修正)・#344 (pipeline_lock レース修正) の post-merge feedback で採用確定した文書系 10 件を、1 本の docs バッチ PR に集約する (2026-08-02 方針決定。per-PR の細切れ doc PR を避け milestone でまとめる運用)。全件 `docs/dev-conventions.md` 中心の追記で、GitHub 仕様の gotcha など Severity High 2 件を含む。
>
> **内容 (10 件)**:
>
> 1. `[workspace] default-members` 不在で `cargo test` = `cargo test --workspace` という暗黙不変条件の明記 + `push-runner-config.toml` への inline comment (#342 T3-1。順位 360 のテストと対)
> 2. ADR-051 (cross-system config coupling) をチェックリストに登録 — 3 系統以上のインフラ設定を跨ぐ変更時の確認項目 (#342 T3-3)
> 3. ambient/auto-detect 環境状態 (git ブランチ名・`GH_REPO`・cwd) に依存せず明示引数を使う設計原則 — PR #238/#247/#343 の 3 例目で systemic (#343 T3-1)
> 4. 並行バグ調査の標準テストパターン — 決定論再現テスト + stress の aggregate 計測 + low-core CI でのみ再現するレースの扱い (#344 T2-1)
> 5. TOCTOU + 2 層防御 (verify-before-destroy + deferred cleanup、最終 fallback は `create_new` 排他へ収束) の設計原則 (#344 T3-1)
> 6. CI 失敗の introduced-by-this-change / pre-existing を diff で切り分けるチェックリスト (#344 T3-2)
> 7. `paths:` フィルタ付き check を required 化すると skip が pending 扱いで PR が永久ブロックされる GitHub gotcha + early-success 代替 (#342 T3-2、Severity High)
> 8. exe-spawn テストは exe + deploy 済 config を temp dir へ staging する規約 — 既存 bounded wait 規約と対 (#342 T3-4、Severity High)
> 9. cross-platform matrix の `fail-fast: false` 既定 (#342 T3-5)
> 10. shell 抽象化確認済み cfg ガードの除去可否ガイダンス + レビュー層がコード内コメント根拠で false positive 判定する際の前提再検証手順 (#342 T3-7)
>
> **参照**: `.claude/feedback-reports/342.md` / `343.md` / `344.md` (各 Rationale)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、[ADR-063](adr/adr-063-linux-portability-release-binaries.md)。
>
> **実行優先度**: 💎 Tier 3 — 各件 Effort XS〜S・合計 M / Adoption Risk None。Severity High 2 件 (#7, #8) を含むため docs バッチとしては早めの実施が望ましい。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ 10 件を追記 (既存 convention の書式に合わせる)
- [ ] `push-runner-config.toml` へ #1 対応の inline comment を追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 10 件すべてが dev-conventions.md (+ inline comment 1 箇所) に反映され、`pnpm lint:docs` / markdownlint が clean であること。

---

### push-runner と ci.yml の cargo test コマンド等価性検証テスト

> **動機**: `push-runner-config.toml` の rust-test group は `cargo test`、`.github/workflows/ci.yml` は `cargo test --workspace` を使い、両者は root `Cargo.toml` に `[workspace] default-members` が**無い**ことに依存して偶然等価になっている。ADR-065 § 決定 2 は「CI とローカルでコマンドが違うと、どちらかの緑が嘘になる」を設計原則とするが、この等価は機械検証されていない。#342 T2-1 と #343 T2-3 が連続 2 PR で独立に指摘し Frequency High。
>
> **対処案**: `Cargo.toml` に `default-members` が導入されたら fail する検証テストを追加する (例: cargo metadata で default-members 不在を assert、または両コマンドの対象 crate 集合の一致を比較)。実装位置は `src/cli-push-runner` の config 検証テスト近傍が候補。
>
> **参照**: `.claude/feedback-reports/342.md` Tier2 #1 / `.claude/feedback-reports/343.md` Tier2 #3、`push-runner-config.toml` (rust-test group)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md) § 決定 2。順位 359 の #1 (文書化) と対。順位 361 と同一 PR (A 系統) にまとめてよい。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency High / Effort S / Adoption Risk None。

#### 作業計画

- [ ] default-members 不在 (または両コマンドの対象集合一致) を assert する検証テストを追加
- [ ] 意図的に default-members を足した状態で fail することを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- `cargo test` (push-runner) と `cargo test --workspace` (CI) の等価が破れる変更が、テスト失敗として land 前に検出されること。

---

### JJ_VERSION の ci.yml / cloud-setup.sh 一致検証テスト

> **動機**: jj バージョン (0.42.0) は `.github/workflows/ci.yml` と `scripts/cloud-setup.sh` の 2 ファイル + ローカル検証環境の 3 箇所論理結合 (ADR-065 § 決定 3、ADR-051 型)。「上げるときは必ず揃える」の手動運用に依存しており、片方だけの更新は「テストが緑でも本番挙動が違う」を生む。#342 T2-2 採用 (supervisor 補正: 版文字列を持つのは 2 ファイルのみ、3 箇所目 = ローカル環境は静的検出不可)。
>
> **対処案**: 2 ファイルから jj 版文字列を抽出して一致を assert するテストを追加する (repo-root `tests/` の配備検証系 or CI step)。
>
> **参照**: `.claude/feedback-reports/342.md` Tier2 #2、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md) § 決定 3、[ADR-051](adr/adr-051-cross-system-config-coupling.md)。順位 360 と同一 PR (A 系統) にまとめてよい。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] 2 ファイルの版文字列一致を assert するテスト/step を追加
- [ ] 片方だけ変更した状態で fail することを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- ci.yml と cloud-setup.sh の jj 版が乖離した変更が land 前に機械検出されること。

---

### git subprocess のブランチ名依存引数を検出する custom lint rule

> **動機**: PR #343 で、exe 内部の `git branch --show-current` subprocess が jj colocated 環境 (git HEAD が detached) で常に空文字を返し、CI 観測が恒久 pending 化する silent 欠陥が実在した。PreToolUse hook は Claude の tool 呼び出し層にのみ効き exe 内部の subprocess には無効なため、`src/**/*.rs` を対象とする custom lint rule (正規表現層、ADR-007) で同型再発を防ぐ。#343 T1-1 採用 (supervisor が PreToolUse 案から再構成済み)。
>
> **対処案**: `.claude/custom-lint-rules.toml` に new rule — git subprocess 呼び出しのブランチ名依存引数 (`branch` + `--show-current` 等) を検出する。ADR-049 に従い incident fixture (bad/good) + `[rules.incident]` provenance (pr = 343) + incident_eval CASES entry を整備する。
>
> **参照**: `.claude/feedback-reports/343.md` Tier1 #1、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md)、[ADR-049](adr/adr-049-incident-eval-regression-suite.md)、[ADR-064](adr/adr-064-monitor-success-positive-evidence.md) Amendment (欠測と正常が同じ出力になる構成の教訓)。順位 363 と同一 PR (B 系統) にまとめてよい。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] custom-lint-rules.toml に rule を追加 (regex + 除外条件の設計)
- [ ] ADR-049 の 3 点セット (bad/good fixture + `[rules.incident]` + CASES entry) を整備
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- `src/**/*.rs` にブランチ名依存の git subprocess が追加された場合に custom lint が発火し、good fixture では発火しないこと (incident_eval で回帰保証)。

---

### check-ci-coderabbit の detached HEAD 回帰統合テスト

> **動機**: PR #343 で修正した「jj colocated (detached HEAD) 環境で CI 状態が恒久 pending 化する」バグの regression test が皆無 (`src/check-ci-coderabbit/tests/` 自体が不在)。再発時は監視の自律ループが再び silent に破綻する。#343 T2-1 採用。
>
> **対処案**: temp dir に jj colocated repo (detached HEAD) を組み、CI 状態解決が statusCheckRollup ベースで機能すること (旧経路のようにブランチ名解決依存で空にならないこと) を検証する統合テストを新設する。gh 呼び出しは実 API に依存しない形 (parse 層の既存単体テスト + 経路の構造検証) を基本とし、実 jj spawn が必要な部分は lib-jj-helpers の `#[ignore]` + 直列実行パターンを踏襲する。
>
> **参照**: `.claude/feedback-reports/343.md` Tier2 #1、`src/check-ci-coderabbit/src/main.rs` (`fetch_ci`) / `src/check-ci-coderabbit/src/parsers.rs` (`parse_ci_rollup` 単体テスト群 = 既存資産)。順位 362 と同一 PR (B 系統) にまとめてよい。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Low / Effort M / Adoption Risk None。

#### 作業計画

- [ ] detached HEAD 環境での CI 状態解決を検証する統合テストを新設
- [ ] ブランチ名依存の旧経路への回帰が fail することを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- detached HEAD 環境で CI 状態が pending に固着する回帰が、テスト失敗として land 前に検出されること。

---

### ADR-054 scope guard の pre-push 展開 — fix diff の allowlist 照合 (ADR-068 残課題)

> **動機**: pre-push の takt fix step には「finding 由来 allowlist との fix diff 照合」の決定論層が無く、instruction (fix.md の scope allowlist) 頼み。2026-08-02 の WP-17 PR 2a incident (fix が finding 対象外の lib crate 2 つを丸ごと削除し gate 全 PASS で push) で顕在化した。[ADR-068](adr/adr-068-fix-step-authority-boundary.md) の後退検知 backstop は削除系 (ファイル脱落 / 追加行削減) のみ検知する 80/20 の暫定で、**追加系の injection (finding 対象外ファイルへの書き込み・config 書き換え) は検知できない**。PR #348 security review の non-blocking 注記 (fix step が push-runner-config.toml の `max_added_line_shrink_pct` / `enabled` を書き換えて backstop 自体を自己弱体化できる経路が instruction 頼み) もこれで閉じる。
>
> **対処案**: `cli-push-runner` の post_takt_regate 段 (または直前の専用 stage) で、`.takt/runs/` の最新 findings レポートから `Location` 列を抽出して allowlist を導出し、takt 前後の diff 差分の変更ファイルを照合する。判定コアは `lib-scope-guard` (WP-17 再分割 PR で land 予定) を再利用し、cli-pr-monitor の post-pr 経路と判定の同一性を保つ (ADR-054 の drift 防止)。violation は ADR-068 の `[FIX_REGRESSION]` と同様の loud block + 独立 kill-switch。findings レポートのパース失敗は fail-closed。
>
> **参照**: [ADR-068](adr/adr-068-fix-step-authority-boundary.md) § 決定 3 / 残課題、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) § 欠点 (pre-push 展開の予告元)、`src/cli-pr-monitor/src/stages/scope_guard.rs` (post-pr 側の先行実装)、PR #348 security review 注記。依存: WP-17 再分割 PR (lib-scope-guard の land) 後が効率的。
>
> **実行優先度**: 🔧 Tier 2 — Severity High (injection 防御の穴) / Frequency Low (fix 発生時のみ) / Effort M / Adoption Risk Low (既存 stage への追加、kill-switch つき)。

#### 作業計画

- [ ] findings レポート (.takt/runs/ 最新 run) から Location 列を抽出する parser (fail-closed)
- [ ] lib-scope-guard で allowlist 照合、violation は loud block + 独立 kill-switch
- [ ] incident 再現テスト: (a) finding 対象外ファイルへの変更が**変更種別 3 種 (追加 = 新規ファイル作成 / 書き換え = 既存ファイル編集、config 自己弱体化含む / 削除) のいずれでも** block されること、(b) `ALWAYS_ALLOWED` 対象ファイル (`.takt/review-diff.txt` 等) への変更は finding allowlist 外でも block されないこと、の両方を固定する (完了基準の「追加・書き換え・削除いずれも」に対応)
- [ ] fix.md / fix-supervisor.md の「pre-push は後退検知のみ」記述を更新
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- fix step が finding 対象外ファイルを変更 (追加・書き換え・削除いずれも) した push が、決定論的に block されること。ADR-068 の後退検知では通ってしまう「追加系 injection」ケースがテストで固定されていること。
- `ALWAYS_ALLOWED` (post-pr 側の先行実装 `src/cli-pr-monitor/src/stages/scope_guard.rs` で定義済み、現状 `.takt/review-diff.txt` のみ) は、fix step が本 instruction (fix.md 「Pre-completion diff refresh」) に従って正当に書き換える中間ファイルの例外リストである。pre-push 側の実装も finding allowlist に加えてこのリストを常に許可し、post-pr 側と同一のリストを共有すること (ADR-054 の drift 防止)。この例外により (a) の block 判定が誤って `.takt/review-diff.txt` 自体の正当な refresh まで block しないことをテストで固定する。
