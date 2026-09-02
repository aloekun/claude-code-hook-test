# TODO (Part 17)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に達したため、順位 319〜332 のエントリを本ファイルに分離した (2026-07-20 docs 50KB 超過解消の物理分割)。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo3.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### 順位 321: ADR-019/WP-03 クォータ設計の前提 stale + 初回レビュー処理中 push のレビュー欠落穴

> **動機**: PR #287 の rate-limit 調査で、WP-03 (ADR-019 amendment) のクォータ設計に **2 つの前提ズレ**が判明した。
>
> **(a) 前提が stale**: `.coderabbit.yaml` 冒頭は「**無料枠レートリミット (3〜4 レビュー/時)** の解除待ちを構造的に削減する」と書かれているが、CR の実際の応答は **`Plan: Pro`**。かつ課金プランのレート制限は固定値ではなく **adaptive per-developer limit** (CR docs: 直近の PR レビュー活動が全ユーザーの 95 パーセンタイル以上に達すると追加レビューの解放が緩やかになる)。**ADR-040 の GPU 前提が stale だった件と同型**で、設計根拠が現状と食い違っている。本件では #276〜#287 の **12 PR を約 24 時間**で投入したことが引き金と強く示唆される (CR 内部カウンタは外部から不可視のため断定はできない)。WP-03 は *PR あたり*のレビュー回数は減らせるが、*developer 単位の rolling window* 枯渇には効かない。
>
> **(b) レビュー欠落穴**: `auto_incremental_review: false` と「初回レビュー処理中の push」が組み合わさると、**新 head が誰にもレビューされない**状態になる。PR #287 の実際の経緯: 12:44 時点で CR は初回レビューを処理中 (`Currently processing new changes... please wait`) → その直後に手動 push で head 差し替え → 新 head は増分レビュー対象外 (設定どおり) → 初回レビューは宙に浮く → 手動 `@coderabbitai review` が必要になり、そこで rate limit に到達。ADR-019 は「**手動 push 後は `@coderabbitai review` を手動投稿**」(§ 手動 fix push は手動トリガーが必要) と規定しているが、**規約 (人間の記憶) に依存**しており仕組み化されていない。
>
> **参照**: `.coderabbit.yaml` 冒頭コメント、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § WP-03 / § 手動 fix push は手動トリガーが必要、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)、`docs/dev-conventions.md` 順位 262 (外部 SaaS 無料枠 / 制限の調査チェックリスト)、PR #287。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Effort S。

#### 作業計画

- [ ] **(a) 前提の是正**: 現行プラン (Pro) と adaptive limit の実態を調査し (`docs/dev-conventions.md` 順位 262 のチェックリストを適用)、`.coderabbit.yaml` 冒頭と ADR-019 § WP-03 の根拠記述を実態に合わせて更新する。**「無料枠 3〜4 レビュー/時」を前提にした設計判断が今も妥当かを再評価する** (adaptive limit なら「PR あたりの削減」より「PR 投入ペース」の方が支配的な可能性)。
- [ ] **(b) 欠落穴の仕組み化を検討**: 手動 push 後の `@coderabbitai review` 投稿は現状「規約」。ADR-042 の境界基準で仕組み化の是非を判定する。候補: push-runner の push stage 後に「CR 再トリガーが必要」を**警告表示**する (助言層 / fail-open)、または `head_already_reviewed()` を使って未レビュー head を検出し警告する (`review_trigger.rs` に既存の照会ロジックあり)。**自動投稿はレート枠を消費するため慎重に** — ADR-019 § 同一 HEAD への再投稿はレート枠の無駄 と整合させること。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- `.coderabbit.yaml` / ADR-019 のクォータ設計根拠が実プラン・実制限と一致していること。
- 手動 push で新 head が未レビューのまま放置される経路に、警告または仕組みによる検出があること。

---

### 順位 326: 並列設計レビュアー (design-fit reviewer) の実験起案 — 見落とし実績の事前調査付き (R4/ADR-047 却下分析の代替案)

> **動機**: R4 の ADR-047 採否判定分析 (2026-07-19、[ADR-047](adr/adr-047-prepush-refute-facet.md) 「却下理由の補強」節) から。直列 refute (verify step) は同日導入の [ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) anomaly policy が **inline 反証** (fact-check 義務) として上流で FP を枯らしたため、**26 run で却下 0 件・便益 0** となり却下推奨。これで precision 側 (FP 除去) は ADR-056 が担う体制になったが、**recall 側 (見落とし) は post-PR CodeRabbit 頼みのまま**。一方 reviewers step は並列実行であり、simplicity execute (実測 avg 203s / max 416s) を律速上限として **第 3 の並列レビュアーを wall-clock 追加ゼロで足せる**見込みがある (security execute avg 92s が simplicity の陰に収まっている実績)。観点は「実装内容」ではなく「**設計内容**」— 見落としやすいポイントの指摘・プロジェクト適合性 (ADR / dev-conventions との整合)。
>
> **重要な区別**: これは反証 (precision フィルタ) の代替ではなく**多視点化 (recall 拡張)**。機能軸が逆であり、「refute の後継」ではなく独立の新実験として評価する。最大リスクは **fix loop 率の再上昇** (現行 8.3% は「finding が減った」直接効果。設計・適合性指摘は anomaly 指摘より主観的で FP を出しやすく、規律なしでは T10 以前の 20〜45% へ逆行し得る)。
>
> **対処案 (2 phase 構成、Phase 0 必須先行)**:
>
> - **Phase 0 — 需要の実証 (ADR-042 の流儀)**: 「simplicity/security が APPROVE した後に、CodeRabbit または post-merge feedback 分析で初めて検出された**設計起因の見落とし**」の実績数を数える。データソースは実在する 3 系列 — (1) `.claude/feedback-reports/*.md` (post-merge-feedback 蓄積、`.takt/runs` に 54 run 分の生成履歴あり)、(2) merged PR の CodeRabbit resolved threads (`gh api` の reviewThreads で path/body 取得可、PR #294 で手順実証済)、(3) `docs/adr/` の「実害後に塞いだ」記録 (ADR-058 の PR #224 等)。**実績ゼロなら見送り** (negative result は dev-conventions 順位 261 convention で永続化)。あわせて weekly-review ([ADR-031](adr/adr-031-weekly-review-pipeline.md) architecture facet) / post-PR CodeRabbit との役割重複を確認し、並列レビュアーでしか埋まらない穴かを判定する。
> - **Phase 1 — 実験導入 (Phase 0 で需要が実証された場合のみ、[ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) 3 点セット)**: `pre-push-review.yaml` の reviewers step に design-review sub-step (sonnet) を**並列追加**。規律は ADR-056 と同一 + 追加 1 点 — (a) fact-check 義務 (実コード・実 ADR で検証してから raise)、(b) articulable 要件、(c) [ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md) output contract、(d) **指摘には根拠ソース (対象 ADR / dev-conventions / 実コードの file:line) の引用を必須**とし、実データ・実ソースに基づかない speculation を禁止、(e) **blocking にできるのは実害を具体的に示せた場合のみ**、それ以外は non-blocking warning (fix loop 再上昇の抑止)。
>
> **受け入れ基準 (Phase 1)**: ①採用された設計 finding ≥1 件/実験期間、②fix loop 率が現行 8.3% から有意に悪化しない、③wall-clock が simplicity 律速のまま (design execute ≤ simplicity execute を `scripts/analyze-takt-timings.ps1` で確認 — 別コミットの観測ツール)。計測は R3 の `push-runs-*.jsonl` (総時間・fix 発生) + step 別 timing 抽出で機械的に行う。
>
> **参照**: [ADR-047](adr/adr-047-prepush-refute-facet.md) §却下理由の補強 (一般反証機構との構成差・本案の出自)、[ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) (inline 反証 = 規律の移植元)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) (Phase 0 需要調査の根拠)、`docs/takt-step-timings.md` (step 別実測、別コミット)、[push-pipeline-fix-plan2.md](push-pipeline-fix-plan2.md) R4。
>
> **実行優先度**: 🔧 Tier 2 — Severity Low〜Medium (現行に実害はない: recall 穴は post-PR CodeRabbit が受けている。改善余地の探索) / Effort: Phase 0 = S、Phase 1 = M (条件付き)。

#### 作業計画

- [ ] Phase 0: feedback-reports / CodeRabbit resolved threads / ADR 実害記録の 3 系列から「pre-push 通過後に検出された設計起因の見落とし」を集計し、需要の有無を判定する (ゼロなら見送り + negative result 永続化で本エントリ完了)。
- [ ] Phase 0: weekly-review architecture facet / post-PR CodeRabbit との役割重複を確認し、並列レビュアー固有の担当領域を定義できるか判定する。
- [ ] Phase 1 (条件付き): design-review facet 作成 + pre-push-review.yaml へ並列追加 (ADR-039 3 点セット、上記規律 (a)〜(e))。
- [ ] Phase 1 (条件付き): 受け入れ基準 ①〜③ を dogfood で計測し、採否判定を ADR 化する。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- Phase 0 の需要調査結果 (実績数と判定) が記録されていること。見送りなら negative result が dev-conventions convention で永続化されていること。
- Phase 1 に進んだ場合: design-review が並列で動き、受け入れ基準 ①〜③ の計測データに基づく採否判定が ADR に記録されていること。

---

### 順位 327: 多段コミットの ADR / observability 更新チェックリストを dev-conventions に追加 (#295/#296 post-merge feedback 採用)

> **動機**: R4 (ADR-047 却下 / ADR-056 延長) を「判定ドラフト → 却下理由補強 → plan2.md 反映 → 却下確定・撤去 → 観測ツール」と複数コミット・複数 PR に分割して進めた際、齟齬が複数回発生した — (a) timing doc が ADR-047 を「却下」と断定したが該当ブランチの ADR status header は未確定だった (PR #295 の pre-push review が REJECT → fix step が訂正)、(b) timing doc の `docs/takt-step-timings.md` への参照を markdown link にすると中間コミットで cross-ref が壊れるため plain-text に統一する必要があった、(c) ADR status 行と「採否判定」セクションの同期。ADR 58 件超・活発な多段階判定運用の本 repo では同型の反復が見込まれる。#295 と #296 の post-merge feedback がいずれも採用候補と判定。
>
> **対処案**: `docs/dev-conventions.md` に「多段コミット/多段 PR で ADR・観測 doc を更新するときのチェックリスト」を追加する。項目案: ① doc が外部 ADR の status (試験運用/却下等) に言及する場合は、参照先 ADR の**現行 status header と同期**しているか (未確定を「確定」と書かない)、② 別コミット/別 PR にまたがるファイルへの参照は **markdown link ではなく plain-text パス**にして中間コミットの cross-ref 破壊を避ける (docs-lint cross-ref は markdown link のみ検査)、③ ADR の status 行と「採否判定」セクションの記述を同時更新する。dev-conventions には WP-06/07/08 由来の同種 checklist 先例が複数あり同形式で追加可能。
>
> **参照**: `.claude/feedback-reports/295.md` Tier3 #2 / `.claude/feedback-reports/296.md` Tier3 #2、`docs/dev-conventions.md`、[ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md) (plain-text 参照統一の先例は本 R4 で ADR-047/056 に適用済)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)。
>
> **実行優先度**: 🔧 Tier 3 — Severity Low / Frequency Medium / Effort S (doc checklist の追加のみ、機械化はしない)。実害は未観測 (齟齬は各 PR の review / feedback で捕捉できている) のため、より重い自動化 (custom lint / pre-push facet checklist) は再発観測後にエスカレーション。

#### 作業計画

- [ ] `docs/dev-conventions.md` に上記 3 項目のチェックリストを追加 (WP-06/07/08 の既存 checklist と同形式)。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 多段コミットで ADR/observability doc を更新する運用者が、status 同期・plain-text 参照・セクション同期の 3 点を dev-conventions のチェックリストで確認できること。

---

### 順位 329: 新規 ADR 起案時の「判断根拠 × 既存 ADR 定義」矛盾チェックリストを追加 (#301 post-merge feedback 採用)

> **動機**: PR-N3 (#301) で、ADR-055 初版が**自ら定義した `decision` 軸 (block/warn = 発火の重み)** と矛盾する除外根拠 (「nudge は block/warn に乗らない」) を採用しており、本 PR で Amendment を追加して除外根拠を撤回する手戻りが発生した。ADR は既に 59 件超を相互参照しており、新規 ADR が既存 ADR の定義・原則と衝突する見落としは他 ADR でも再発しうる。#301 の post-merge feedback が採用候補と判定 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。
>
> **対処案**: `CLAUDE.md` または `docs/dev-conventions.md` に「新規 ADR 起案時のチェックリスト」を追加する。項目案: ① ADR が用いる用語・軸 (例 `decision` = 発火の重み) が**既存 ADR の定義と衝突していないか**、② 除外/非除外・採用/却下などの判断根拠が、参照先 ADR が既に定義した原則から**演繹的に導けるか** (別解釈を新設していないか)、③ 衝突が**新規 ADR 初版の誤り**由来なら起案時に初版で解消する。ただし既存 ADR が陳腐化した等で**方針を意図的に変更・supersede する**正当なケースは別扱いとし、Amendment / superseding ADR による明示的更新を妨げない (「初版の誤り」と「既存方針の意図的変更」を区別する項目を設ける)。#327 (多段コミットの ADR/observability 更新チェックリスト) と対をなす doc-only 対処で、同セクションにまとめると発見性が良い。
>
> **参照**: `.claude/feedback-reports/301.md` Tier3 #1、[ADR-055](adr/adr-055-firing-telemetry-collection.md) (§計装スコープ の `decision` 軸定義と Amendment (2026-07-19) の除外根拠撤回)、`docs/dev-conventions.md`、#327 (関連 checklist)。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort S (doc checklist のみ、機械化はしない。ADR 相互参照数が多く同型見落としが再発しうるが、実害は各 PR review/feedback で捕捉できているため機械化は再発観測後にエスカレーション)。

#### 作業計画

- [ ] `CLAUDE.md` または `docs/dev-conventions.md` に上記 3 項目のチェックリストを追加 (#327 と同セクションにまとめる)。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 新規 ADR 起案者が、用語・軸の既存 ADR 定義との整合と判断根拠の演繹可能性をチェックリストで確認でき、ADR-055 型の初版自己矛盾 → Amendment 撤回の手戻りを防げること。

---

### 順位 330: 「行動要求 nudge は 2 チャネル返却」+「多義的戻り値は struct 化」convention の明文化 (#299 post-merge feedback 採用)

> **動機**: PR-N1 (#299) で、ユーザー行動を要求する nudge (weekly reminder) を `additionalContext` (モデル向け) だけでなく `systemMessage` (ユーザー向け) の 2 チャネルで返す設計 ([ADR-059](adr/adr-059-hook-system-message-visibility.md)) を確立し、その過程で `compute_weekly_review_reminder_nudge` の戻り値を「additional_context + system_message」の struct (`WeeklyReviewNudge`) に変更した。ADR-059 の第2弾展開 (PR monitor catch-up / post-merge recovery / failed marker) で同型パターンの再利用が見込まれる。weekly reminder が 4 週間気付かれなかった実害 (Severity Medium) の再発防止として設計原則を明文化する。#299 の post-merge feedback が採用候補と判定 (Effort XS / Adoption Risk None)。
>
> **対処案**: `docs/dev-conventions.md` に 2 点を追記する。① **ユーザーの行動を要求する nudge は systemMessage (ユーザー可視) と additionalContext (モデル可視) の 2 チャネルで返す** (ADR-059 の可視化チャネル分離)、② **戻り値が複数の意味役割を持つ場合は tuple/多値 flag ではなく struct 化して役割を命名する** (`WeeklyReviewNudge { additional_context, system_message }` の先例)。
>
> **参照**: `.claude/feedback-reports/299.md` Tier3 #1、[ADR-059](adr/adr-059-hook-system-message-visibility.md)、`src/hooks-session-start/src/weekly_review.rs` (`WeeklyReviewNudge`)、`docs/dev-conventions.md`。
>
> **実行優先度**: 💎 Tier 3 — Severity Medium / Frequency Medium / Effort XS (dev-conventions への 1 節追記のみ、ADR-059 第2弾展開で再利用見込み)。

#### 作業計画

- [ ] `docs/dev-conventions.md` に上記 2 点の convention を追記。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 行動要求 nudge を実装する運用者が、2 チャネル返却と多義的戻り値の struct 化を dev-conventions で確認できること。

---

### 順位 331: hooks-session-start に systemMessage を含む JSON 出力の exe-spawn E2E テストを追加 (#299 post-merge feedback 採用)

> **動機**: PR-N1 (#299) で systemMessage 可視化 (ADR-059) を追加したが、テストは `build_session_start_json` の pure function レベルに留まり、**実 config パースを含む exe 実駆動レベルの検証がない** (`src/hooks-session-start/tests/` 自体が未作成)。ADR-059 の第2弾展開で同型の 2 チャネル JSON contract が複製される見込みで、JSON contract の regression を exe レベルで seal する価値がある。#299 の post-merge feedback が採用候補と判定 (Effort S / Adoption Risk None)。
>
> **対処案**: `src/hooks-session-start/tests/e2e.rs` (新設) に、SessionStart 入力 JSON を stdin で渡して exe を駆動し、`systemMessage` を含む出力 JSON の形状 (systemMessage 有り/無し・additionalContext の nudge 併載) を assert する E2E を追加する。既存の exe-spawn bounded-wait convention ([ADR-049](adr/adr-049-incident-eval-regression-suite.md) `incident_eval.rs`) を踏襲。**注記**: 本 E2E は JSON contract の regression 防止に留まり、Claude Code クライアント UI 側の実描画確認 (ADR-059 削除条件2 / 判定期限 2026-08-16) は代替できないため dogfood 目視は別途必要。
>
> **参照**: `.claude/feedback-reports/299.md` Tier2 #1、[ADR-059](adr/adr-059-hook-system-message-visibility.md)、[ADR-049](adr/adr-049-incident-eval-regression-suite.md) (exe-spawn E2E 先例)、`src/hooks-session-start/src/main.rs` (`build_session_start_json`)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort S (既存 exe-spawn E2E convention を流用可能、tests/ 新設)。

#### 作業計画

- [ ] `src/hooks-session-start/tests/e2e.rs` を新設し、実 config + stdin 入力で exe を駆動して systemMessage 有り/無しの JSON 形状を assert。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 実 config パース込みの exe 駆動で systemMessage を含む JSON contract が regression テストで seal されること (UI 実描画確認は別途 dogfood)。

---

### 順位 332: `pnpm build:all` 前に git usr/bin (cp.exe) の PATH 未設定を自動検出・追加 (#301 post-merge feedback 採用)

> **動機**: `pnpm build:all` (及び per-crate `build:*`) は `cp target/release/X.exe .claude/X.exe` で Unix `cp` を使うが、pnpm は Windows で `cmd.exe` 経由で script を実行するため `cp` が解決できず copy step が失敗する (`'cp' is not recognized`)。memory `windows-build-cp-path-gotcha.md` に既記録だが、PR-N3 (#301) の実装でも**再度手動で PATH 追加が必要になった (再発 2 回目)**。ビルド阻害という Severity Medium と再発 Frequency Medium が揃う。#301 の post-merge feedback が採用候補と判定。
>
> **対処案**: `package.json` の `build:all` (または各 `build:*`) で、Windows のとき git の `usr/bin` (cp.exe 提供) を PATH に前置してから cargo/cp を実行する。Windows 限定の additive な分岐 (他 OS は非該当) とし既存の Unix 動作を変えない。あるいは `cp` を Node の cross-platform copy (`node -e` / `shx` 等) に置換する案も検討。あわせて setup ドキュメントへの明記を補助的に実施。
>
> **参照**: `.claude/feedback-reports/301.md` Tier1 #2、`package.json` (`build:all` / `build:*` scripts)、memory `windows-build-cp-path-gotcha.md` (既記録・再発)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (ビルド阻害) / Frequency Medium (再発 2 回目) / Effort S (Windows 限定 if 分岐、他 OS 非影響、Adoption Risk は OS 依存分岐のみ)。

#### 作業計画

- [ ] `package.json` の build script を Windows で cp.exe を解決できるよう修正: `git.exe` の場所を自動検出 (非標準インストールにも対応) → `usr/bin/cp.exe` の存在確認 → 既存 PATH を保持したまま前置。未検出時は cross-platform copy (`node -e` / `shx` 等) へ fallback するか明確なエラーを出す (silent 失敗にしない)。
- [ ] setup ドキュメントに前提を明記 (補助)。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- クリーンな Windows 環境で `pnpm build:all` が手動 PATH 調整なしに exe を `.claude/` へ配布できること。
- Git が非標準の場所にインストールされている / `cp.exe` が不在の環境でも、cross-platform copy への fallback か診断可能な明確なエラーで失敗すること (silent 失敗・意味不明な `'cp' is not recognized` で止まらない)。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo10.md / todo9.md 末尾を参照。)
