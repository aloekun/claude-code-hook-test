# TODO (Part 3)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo2.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して PR #88 以降の新規エントリは本ファイルに記録した。本ファイルも PR #96 セッションで 50KB 接近のため、それ以降の新規エントリは [docs/todo4.md](todo4.md) へ。ほかの todo ファイル (現存する一覧は [docs/todo.md](todo.md) の preamble が正) の既存エントリは引き続き有効、相互に独立。新セッションでは25つすべてを確認すること (todo.md / todo3-28.md / todo-summary.md / todo-summary2.md / todo-summary3.md。todo2.md は 2026-08-12 退役)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 順位 17: `pnpm create-pr` 必須引数未指定時のヘルプ改善 (PR #88 T2-5)

> **動機**: 引数なしで `pnpm create-pr` を実行すると `gh pr create` が `must provide --title and --body (or --fill or fill-first or --fillverbose)` エラーのみ出力し、使用例が示されない。今回 PR 作成時に手動ワークアラウンド (`pnpm prepare-pr-body` で `.tmp-pr-body.md` 生成 → `pnpm create-pr -- --title "..." --body-file .tmp-pr-body.md`) が必要になった。`gh` のエラーをそのまま流す現設計だと、Claude や人間が次の手を察するのに余計な往復が発生する。
>
> **本タスクの位置づけ**: cli-pr-monitor の UX 改善。現実装は `gh pr create` への薄い wrapper だが、必須引数チェックを wrapper 側で実施することで使用例付きエラーを返せる。
>
> **参照**: `.claude/feedback-reports/88.md` の Tier 2 #5 finding
>
> **実行優先度**: **Tier 2** — 工数 Small。daily efficiency への影響中 (PR 作成は頻繁ではないが、エラー時の摩擦が高い)。

#### 背景

- 現実装: `cli-pr-monitor.exe` (PR 作成モード) は受け取った args をそのまま `gh pr create` に forwarding
- `gh` のエラーは英語かつ汎用的。プロジェクト固有の推奨 (prepare-pr-body スクリプトを使う等) は反映されない
- Claude / 人間の双方が「`pnpm prepare-pr-body` を先に呼ぶ」運用を覚える必要がある

#### 設計決定 (案)

- cli-pr-monitor の PR 作成モード入口で `--title` / `--body` / `--body-file` / `--fill*` 系のいずれかが指定されているかチェック
- 未指定なら使用例付きエラーを stderr に出力して非 0 で exit:

```text
Error: PR title and body are required.
Usage:
  pnpm create-pr -- --title "feat: ..." --body-file .tmp-pr-body.md
  pnpm create-pr -- --title "feat: ..." --fill-verbose
Hint:
  Run `pnpm prepare-pr-body` first to generate `.tmp-pr-body.md` from stdin.
```

- gh の実行は引数チェック後にのみ進む

#### 作業計画

- [ ] cli-pr-monitor の PR 作成モード入口で arg validation 追加
- [ ] エラーメッセージ作成 (上記の使用例ベース)
- [ ] dogfood: 引数なしで `pnpm create-pr` 実行 → 改善されたエラーが出ることを確認
- [ ] 既存の正常系 (--title --body-file 指定時) が変わらず動作することを確認
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- 引数なし実行でプロジェクト固有の使用例 + Hint がエラーに含まれる
- `--title` + `--body-file` または `--fill*` 指定時は従来通り PR 作成が走る

#### 詰まっている箇所

なし (Effort Small、cli-pr-monitor 入口の arg parser 拡張のみ)

---

### 順位 18: `.failed` marker への recovery 手順自己文書化 (PR #90 T2-2)

> **動機**: ADR-030 で確立した soft-fail 機構 (`<pr>.md.failed` marker + L2 recovery) は PR #89 セッションで実際に発火し、UserPromptSubmit hook 経由で recovery が機能することが実証された。しかし現状の marker file は識別子のみで、recovery に必要な手順 (再実行コマンド、必要な引数、想定所要時間、よくある失敗原因) が外部 (ADR-030 / skill SKILL.md) を参照しないと分からない。marker 自体に手順を埋め込めば、将来 (ドキュメント所在を忘れた時 / ADR-030 が改訂された時 / 派生プロジェクトでの再現時) の recovery が省力化される。
>
> **本タスクの位置づけ**: ADR-030 の運用負荷削減。soft-fail 機構そのものは正しく動作しているため、UX 改善カテゴリ。marker file の content をテンプレート化し、生成側 (cli-merge-pipeline) で recovery 手順 + コマンド例 + ADR-030 への参照を含める。
>
> **参照**: `.claude/feedback-reports/90.md` の Tier 2 #2 finding
>
> **実行優先度**: **Tier 2** — 工数 S。daily efficiency への影響中 (recovery 発生頻度は低いが、発生時の摩擦を低減)。rate-limit 系 task (cli-pr-monitor ポーリング延長 PR #88 T2-4、完了済 / post-pr-review rate-limit 自動検出) ほど critical ではないが、ADR-030 の long-term 運用品質に寄与。

#### 背景

- ADR-030 の L1 (cli-merge-pipeline → takt workflow 同期実行) が失敗した場合、`.claude/feedback-reports/<pr>.md.failed` marker が残存する設計
- L2 recovery (UserPromptSubmit hook) が次セッションで marker を検出し additionalContext で再実行を促す
- PR #89 セッションで実際に soft-fail が発火し、recovery 経路が機能した実証あり
- 課題: marker file の content が空 or 識別用の最小情報のみで、再実行手順は外部ドキュメント (ADR-030 / skill SKILL.md) を参照する必要がある
- 将来リスク: ADR-030 改訂・派生プロジェクト展開・時間経過による参照先不明化により、recovery が高摩擦化する可能性

#### 設計決定 (案)

- cli-merge-pipeline (or takt workflow 失敗時の marker 書込み箇所) で marker content をテンプレート化
- テンプレート例:

~~~markdown
# Post-Merge Feedback Failed: PR #<pr>

This marker indicates the post-merge feedback workflow failed for PR #<pr>.
The L2 recovery hook (UserPromptSubmit) will detect this file on the next
prompt and prompt Claude to re-run the workflow.

## Manual Recovery (if L2 hook does not fire)

1. Check the takt run logs at `.takt/runs/<run-id>/` for the failure reason.
2. Re-run the workflow:

   ```sh
   takt run post-merge-feedback.yaml --input pr=<pr>
   ```

3. On success this marker will be replaced by `.claude/feedback-reports/<pr>.md`.

## Failure Context

- Failed at: <ISO 8601 timestamp>
- takt run id: <run-id>
- Last error (truncated to 500 chars): <stderr tail>

## Reference

- ADR-030: docs/adr/adr-030-deterministic-post-merge-feedback.md
~~~

- marker 内容は ADR 改訂耐性のため「ADR-030 への参照リンク + 当時の手順」を共存させる
- 失敗の context (timestamp / run-id / stderr tail) を含めることで、再実行前に原因切り分けがしやすくなる
- 本タスク完了後、L2 hook の additionalContext からも marker content を読ませる構成にすれば自己完結度が上がる (本タスクの拡張、必須ではない)

#### 作業計画

- [ ] cli-merge-pipeline の `.failed` marker 書込みロジックを確認 (現状 content がどう生成されているか)
- [ ] テンプレート文字列を crate 内 const として定義 or 外部 template ファイル化を判定
- [ ] timestamp / run-id / stderr tail を marker に埋め込む実装
- [ ] L2 hook (`hooks-user-prompt-feedback-recovery` 等) の additionalContext 出力で marker content を流用するか判定 (本タスクの scope 内 or 別タスク化)
- [ ] dogfood: 意図的に takt fail を inject し、marker に手順 + context が含まれることを確認
- [ ] ADR-030 を更新 (marker format の section を追記)
- [ ] 本 todo3.md エントリを削除

#### 完了基準

- `.failed` marker file に recovery 手順 + コマンド例 + ADR-030 参照 + failure context が含まれる
- ADR-030 の本文に marker format が明文化される
- 派生プロジェクトでも同じ template が機能する (ADR-030 が外部 reference として読める前提)

#### 詰まっている箇所

なし (Effort S、cli-merge-pipeline の marker 書込み箇所のテンプレート化のみ)



---

## 旧 todo17.md から移した分 (2026-09-29 統合)

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
> **参照**: [ADR-047](adr/adr-047-prepush-refute-facet.md) §却下理由の補強 (一般反証機構との構成差・本案の出自)、[ADR-056](adr/adr-056-review-policy-anomaly-shadow.md) (inline 反証 = 規律の移植元)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) (Phase 0 需要調査の根拠)、`docs/adr/adr-056-step-timings.md` (step 別実測、別コミット)、[ADR-015](adr/adr-015-push-runner-takt-migration.md) § 検討して採らなかった方向 R4。
>
> **実行優先度**: Tier 2 — Severity Low〜Medium (現行に実害はない: recall 穴は post-PR CodeRabbit が受けている。改善余地の探索) / Effort: Phase 0 = S、Phase 1 = M (条件付き)。

#### 作業計画

- [ ] Phase 0: feedback-reports / CodeRabbit resolved threads / ADR 実害記録の 3 系列から「pre-push 通過後に検出された設計起因の見落とし」を集計し、需要の有無を判定する (ゼロなら見送り + negative result 永続化で本エントリ完了)。
- [ ] Phase 0: weekly-review architecture facet / post-PR CodeRabbit との役割重複を確認し、並列レビュアー固有の担当領域を定義できるか判定する。
- [ ] Phase 1 (条件付き): design-review facet 作成 + pre-push-review.yaml へ並列追加 (ADR-039 3 点セット、上記規律 (a)〜(e))。
- [ ] Phase 1 (条件付き): 受け入れ基準 ①〜③ を dogfood で計測し、採否判定を ADR 化する。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- Phase 0 の需要調査結果 (実績数と判定) が記録されていること。見送りなら negative result が [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) § 追記 (2026-09-13) の 3 点セットで永続化されていること。
- Phase 1 に進んだ場合: design-review が並列で動き、受け入れ基準 ①〜③ の計測データに基づく採否判定が ADR に記録されていること。

---

### 順位 327: 多段コミットの ADR / observability 更新チェックリストを cli-docs-lint の検査にする (#295/#296 post-merge feedback 採用)

> **動機**: R4 (ADR-047 却下 / ADR-056 延長) を「判定ドラフト → 却下理由補強 → plan2.md 反映 → 却下確定・撤去 → 観測ツール」と複数コミット・複数 PR に分割して進めた際、齟齬が複数回発生した — (a) timing doc が ADR-047 を「却下」と断定したが該当ブランチの ADR status header は未確定だった (PR #295 の pre-push review が REJECT → fix step が訂正)、(b) timing doc の `docs/takt-step-timings.md` への参照を markdown link にすると中間コミットで cross-ref が壊れるため plain-text に統一する必要があった、(c) ADR status 行と「採否判定」セクションの同期。ADR 58 件超・活発な多段階判定運用の本 repo では同型の反復が見込まれる。#295 と #296 の post-merge feedback がいずれも採用候補と判定。
>
> **対処案** (2026-09-08 に出口を再設計。文書チェックリストではなく検査として実装する。convention 集は 2026-09-13 に廃止、順位 445): 下の 3 項目を `cli-docs-lint` の検査にする。項目案: ① doc が外部 ADR の status (試験運用/却下等) に言及する場合は、参照先 ADR の**現行 status header と同期**しているか (未確定を「確定」と書かない)、② 別コミット/別 PR にまたがるファイルへの参照は **markdown link ではなく plain-text パス**にして中間コミットの cross-ref 破壊を避ける (docs-lint cross-ref は markdown link のみ検査)、③ ADR の status 行と「採否判定」セクションの記述を同時更新する。dev-conventions には WP-06/07/08 由来の同種 checklist 先例が複数あり同形式で追加可能。
>
> **参照**: `.claude/feedback-reports/295.md` Tier3 #2 / `.claude/feedback-reports/296.md` Tier3 #2、`src/cli-docs-lint/`、[ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md) (plain-text 参照統一の先例は本 R4 で ADR-047/056 に適用済)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)。
>
> **実行優先度**: Tier 3 — Severity Low / Frequency Medium / Effort S (cli-docs-lint の検査 3 本)。実害は未観測 (齟齬は各 PR の review / feedback で捕捉できている) のため、より重い自動化 (custom lint / pre-push facet checklist) は再発観測後にエスカレーション。

#### 作業計画

- [ ] 上記 3 項目を **cli-docs-lint の検査として実装**する — ② (中間コミットで壊れる参照) は既存 cross_ref の隣、①③ (ADR status の同期) は新規 validator (2026-09-08 に出口を再設計。dev-conventions.md へは書かない)。
- [ ] 本エントリ削除 + todo-summary2.md 行削除。

#### 完了基準

- 多段コミットで ADR/observability doc を更新したとき、status 同期・中間コミットで壊れる参照・セクション同期の 3 点のずれが `pnpm lint:docs` で検出されること。

---

### 順位 331: hooks-session-start に systemMessage を含む JSON 出力の exe-spawn E2E テストを追加 (#299 post-merge feedback 採用)

> **動機**: PR-N1 (#299) で systemMessage 可視化 (ADR-059) を追加したが、テストは `build_session_start_json` の pure function レベルに留まり、**実 config パースを含む exe 実駆動レベルの検証がない** (`src/hooks-session-start/tests/` 自体が未作成)。ADR-059 の第2弾展開で同型の 2 チャネル JSON contract が複製される見込みで、JSON contract の regression を exe レベルで seal する価値がある。#299 の post-merge feedback が採用候補と判定 (Effort S / Adoption Risk None)。
>
> **対処案**: `src/hooks-session-start/tests/e2e.rs` (新設) に、SessionStart 入力 JSON を stdin で渡して exe を駆動し、`systemMessage` を含む出力 JSON の形状 (systemMessage 有り/無し・additionalContext の nudge 併載) を assert する E2E を追加する。既存の exe-spawn bounded-wait convention ([ADR-049](adr/adr-049-incident-eval-regression-suite.md) `incident_eval.rs`) を踏襲。**注記**: 本 E2E は JSON contract の regression 防止に留まり、Claude Code クライアント UI 側の実描画確認 (ADR-059 削除条件2 / 判定期限 2026-08-16) は代替できないため dogfood 目視は別途必要。
>
> **参照**: `.claude/feedback-reports/299.md` Tier2 #1、[ADR-059](adr/adr-059-hook-system-message-visibility.md)、[ADR-049](adr/adr-049-incident-eval-regression-suite.md) (exe-spawn E2E 先例)、`src/hooks-session-start/src/main.rs` (`build_session_start_json`)。
>
> **実行優先度**: Tier 2 — Severity Medium / Frequency Medium / Effort S (既存 exe-spawn E2E convention を流用可能、tests/ 新設)。

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
> **実行優先度**: Tier 2 — Severity Medium (ビルド阻害) / Frequency Medium (再発 2 回目) / Effort S (Windows 限定 if 分岐、他 OS 非影響、Adoption Risk は OS 依存分岐のみ)。

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

---

## 旧 todo18.md から移した分 (2026-09-29 統合)

### 順位 215: `~/.claude/rules/common/coding-style.md` に「Defensive State Reset in State Machines」section 追加 (PR #214 post-merge-feedback T3-1 採用)

> **動機**: PR #214 round 2 で CR Major #4 (`既存 state 再利用時も現在の push 情報に更新してください`) の fix として `finalize_initial_review_park` 内で `read_state()` 後に `state.pr` / `state.repo` / `state.started_at` を `ctx` 値で **無条件上書き** する pattern を land した。この pattern は同 function 内の既存 reset と同型 (CR Major #1 fix で `head_commit` 上書き、CR Major #2 fix で `review_recheck_count = 0`) で、現時点で 3 field に適用済の確立された defensive pattern。
>
> ただしこの「無条件上書き」は新規 reader / reviewer から見ると一見「冗長 (= `unwrap_or_else(|| ::new(...))` で既に同値を設定済だから不要)」に見える危険性がある。同型コードを future PR で reviewer (人間 / AI 両方) が「redundant resets は削除すべき」と誤判定して削除した場合、prior cycle の stale state (古い PR 番号 / repo / 開始時刻) が混入する silent bug を導入するリスクが顕在化する。
>
> **本タスクの位置づけ**: PR #214 post-merge-feedback Tier 3 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-20 ユーザー承認)。analyzer rationale: 「PR #214 の `review_recheck.rs` で positive pattern として land (lines 185–187)。同型コード (`review_recheck_count`, `head_commit` 上書き) との一貫性がある確立されたパターン。Frequency Medium = cli-pr-monitor には複数の state machine があり再発確実。Effort S、Adoption Risk None → ✅ 採用候補と判定」。pre-push:simplicity + pre-push:security の独立 2 ソース検出。
>
> **参照**: `.claude/feedback-reports/214.md` Tier 3 #1、旧 `src/cli-pr-monitor/src/stages/poll/review_recheck.rs` の `finalize_initial_review_park` defensive reset block (**WP-17 PR 3 の park モデル廃止でファイルごと削除済み** — pattern の実例は PR #214 の diff を参照。rule 化する価値は削除後も変わらない: 同型の state machine は `finalize_pending_review` / iteration の state 継承等に現存)、memory `feedback_no_unenforced_rules.md` (enforcement 要件)、memory `feedback_global_config_backup.md` (snapshot 必須)。
>
> **実行優先度**: **Tier 3** — Effort S。global rules への docs 追記 ~30 行で完結、`feedback_global_config_backup` snapshot を忘れない。
>
> **Status update (2026-08-12)**: 旧環境 rules snapshot (syncthing/.claude_old、2026-06-17 凍結) の実査で本タスクは旧環境でも未実施と確認。~/.claude/rules の再配置 (採否) が保留中のため、配置先確定 (docs/todo22.md の「旧 rules 採否判断」エントリ) 後に着手する。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/coding-style.md` の末尾 (`## Code Quality Checklist` の直前) または `## Error Handling` 直後に新 section「Defensive State Reset in State Machines」を追加
- **rule 内容**: 「State machine 内で `read_state()` / `load_state()` 等の persisted state を再利用する場合、`new()` で設定される identity field と同等の **無条件上書き reset** を `read_state()` 後に明示的に書く。これは『冗長』に見えるが、prior cycle の stale state (古い PR 番号 / repo / session ID 等) が再利用 path で混入する silent bug を防ぐ defensive pattern。reviewer (人間 / AI) は redundant 削除を提案しないこと」
- **anti-pattern 警告**: `let state = read_state().unwrap_or_else(|| State::new(id, repo, time));` だけで identity field を `ctx` で上書きしないと、prior cycle の値が残留する
- **good pattern 例**: PR #214 `review_recheck.rs:177-193` を inline cite (`state.pr` / `state.repo` / `state.started_at` / `state.review_recheck_count` / `state.head_commit` の 5 field reset)
- **由来 cite**: PR #214 の CR Major #4 が「既存 state 再利用時も現在の push 情報に更新してください」として独立検出した実証
- **enforcement layer**: 機械 lint は困難 (`read_state` pattern の構文認識 + identity field 列挙が必要) だが、simplicity-review LLM が `coding-style.md` を読むため "enforced via review" として機能、memory `feedback_no_unenforced_rules` 例外を満たす
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/coding-style.md` に新 section「Defensive State Reset in State Machines」追記 (anti-pattern + good pattern + PR #214 由来 cite、約 30 行)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/coding-style.md` に「Defensive State Reset in State Machines」section が追加される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 由来 cite (PR #214 CR Major #4 + `review_recheck.rs:177-193`) で reviewer / Claude が rule 背景を理解可能
- simplicity-review LLM が future PR で同型 `read_state()` を含む state machine 編集を review する際、本 section の anti-pattern 警告を参照可能

#### 詰まっている箇所

なし。Effort S、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### 順位 217: `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に config file comments の permanent artifact 扱い明記 + workstream sequence 禁止例追加 (PR #216 post-merge-feedback T3-1 採用)

> **動機**: 既存 `coding-style.md` § Cross-File Reference Lifecycle は markdown 内 cross-reference (docs/ADR/README 等) を主に想定して書かれており、**config file comments (`.toml`/`.json`/`.yaml`) も permanent artifact** であることが暗黙的にしか扱われていない。PR #216 で `hooks-config.toml` comment に "PR-1" / "PR-3" ephemeral workstream sequence を embed した違反は、author が「config の comment は注釈であって rule の対象外」と暗黙的に判断していた可能性が高い。
>
> 順位 216 の lint rule が機械的に防止するが、author の理解を促す **文書層** として補完することで「なぜ config comment にも reference lifecycle が適用されるか」を理解可能にする。機械層 (216) + 文書層 (本 task) の 2 層防御は順位 200/202/205 と同 pattern。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。順位 216 (機械層) と 1 PR bundle 推奨。analyzer rationale: 「既存ルールは markdown document 内の cross-reference を主に想定しており config file comments の permanent artifact としての扱いが暗黙的。Tier 1-1 の custom lint rule が機械的に防止するが、author の理解を促す文書層として補完。Frequency Medium (cross-file reference violations の systemic pattern と同根)」。Session T3-1 + PR-analysis T3-1 の独立 2 ソース収束。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #1、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (現行 section、編集対象)、順位 216 (機械層 = lint rule、本 task の機械強制対応)、memory `feedback_global_config_backup` (snapshot 必須)、PR #216 hooks-config.toml diff (違反実例として inline cite)。
>
> **実行優先度**: **Tier 3** — Effort XS。global rules への docs 追記 ~15 行で完結、`feedback_global_config_backup` snapshot を忘れない。
>
> **Status update (2026-08-12)**: 旧環境 rules snapshot (syncthing/.claude_old、2026-06-17 凍結) の実査で本タスクは旧環境でも未実施と確認。~/.claude/rules の再配置 (採否) が保留中のため、配置先確定 (docs/todo22.md の「旧 rules 採否判断」エントリ) 後に着手する。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle の anti-pattern examples block (現状 Rust raw string / TOML コメント / JSONC ヘッダーコメント の 3 種を含む) の TOML コメント sub-section に「workstream sequence names も禁止」と明記
- **追加内容案**:

  ```markdown
  - **TOML コメント / config** (拡張):
    - BAD: `# 由来: docs/todo.md "<task name>" 参照のため`
    - BAD: `# 詳細: docs/local-llm-offload-analysis.md §A-2 を参照` (`*-analysis.md` は ephemeral 計画書、retire 時に dead pointer 化)
    - BAD (workstream sequence): `# PR-3 で移行予定` / `# 次 PR (PR-1) で実装` (ephemeral workstream sequence、PR シリーズ完了後に文脈喪失で dead pointer 化)
    - GOOD: `# 由来: PR #94 (docs lifecycle 整理)` または ADR 参照
    - GOOD: `# 詳細: docs/adr/adr-NNN-feature.md を参照` または config 設計意図を inline で 1-2 行記述
    - GOOD (workstream cite): `# 由来: PR #216` (GitHub PR number は永続 identifier)
  ```

- **由来 cite**: PR #216 で `hooks-config.toml` の `weekly_review_reminder` comment に `(2026-06-23、PR-1)` / `次 PR (PR-3) で移行予定` を embed した実例を inline cite
- **enforcement layer**: 機械層は順位 216 lint rule で強制、本 task は author の理解促進と「なぜ workstream sequence も dead pointer になるか」の rationale 提供
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle の TOML コメント anti-pattern block に workstream sequence 禁止例を追加 (~5 行)
- [ ] 同 section 末尾近くの GOOD examples block に GitHub PR number 形式 (`# 由来: PR #NNN`) を明示 (~2 行)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に「workstream sequence names (`PR-1`/`PR-3` 等) も config comment 内で禁止」が明文化される
- GOOD example として GitHub PR number 形式 (`# 由来: PR #NNN`) が提示される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- 順位 216 (機械層) と同 PR で land した場合、機械強制 + 文書理解の 2 層防御が確立される

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### 順位 218: ADR-039 § Bounded Lifetime + `~/.claude/rules/common/patterns.md` に provisional `enabled` 変更時の todo entry 必須化を追加 (PR #216 post-merge-feedback T3-2 採用)

> **動機**: PR #216 で `weekly_review_reminder.enabled = false → true` を「次 PR-3 (`[features].enabled` allow-list 移行) で真の opt-in 切り替えになるまでの暫定」として config comment に rationale を残したが、対応する `docs/todo*.md` の **移行 tracking entry を作成していなかった**。
>
> このため:
>
> - 「いつ PR-3 で移行する予定か」が config comment にしか残らず、commit を辿らないと判らない
> - PR-3 が遅延または忘れられた場合、provisional state が silent に永続化する (= silent aging)
> - ADR-039 § Bounded Lifetime の「採否判定タイミングの明示」原則が config comment では弱く、todo entry で明示すべき
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「provisional state を config comment のみで追跡する pattern が silent aging を招く。ADR-039 の bounded-lifetime checklist に『provisional enabled 変更 → todo entry 追加』を明示することで future PR での遵守を促進。Frequency Low (初観測) だが Adoption Risk None で早期 codify の費用対効果は高い」。Session T3-2 + PR-analysis T3-2 + Prepush T2-1 (ADR 側アプローチ統合) の 3 ソース独立収束。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #2、`docs/adr/adr-039-experimental-feature-standard-pattern.md` § Bounded Lifetime (編集対象、6-point design checklist 拡張)、`~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 (補助編集対象、同旨 note 追加)、PR #216 hooks-config.toml comment (違反実例)、memory `feedback_global_config_backup` (snapshot 必須)。
>
> **実行優先度**: **Tier 3** — Effort XS。ADR + global rules への docs 追記 ~10 行で完結、`feedback_global_config_backup` snapshot を忘れない。
>
> **Status update (2026-08-12)**: 旧環境 rules snapshot (syncthing/.claude_old、2026-06-17 凍結) の実査で、旧環境の patterns.md に土台 section (Experimental Feature 設計時の参照必須、PR #194 由来) は存在するが、本タスクの増分 (provisional enabled 変更時の todo tracking note) は旧環境でも未実施。~/.claude/rules の再配置 (採否) が保留中のため、配置先確定 (docs/todo22.md の「旧 rules 採否判断」エントリ) 後に着手する。

#### 設計決定 (案)

- **ADR-039 編集**: § Bounded Lifetime の 6-point design checklist に新 checklist item 追加 (本 ADR は project-local のため snapshot 対象外):

  ```markdown
  - [ ] **provisional state の todo tracking**: 試験運用中に config 値を一時的に変更する場合 (例: `enabled = false → true` を本採用判定前に試験的に有効化)、対応する `docs/todo*.md` entry を作成し、移行/採否判定タイミングを明示する。config comment のみで追跡すると silent aging を招く
  ```

- **`~/.claude/rules/common/patterns.md` 編集** (global、波及対象): § Experimental Feature 設計時の参照必須 の末尾に同旨 note を追加 (~3 行):

  ```markdown
  > **provisional state の追跡**: 試験運用中に config 値を一時的に変更する場合 (例: 試験運用元での明示 enable)、必ず `docs/todo*.md` に移行 tracking entry を作成し、採否判定タイミングを明示する。config comment のみの追跡は silent aging を招く (PR #216 で実観測、ADR-039 § Bounded Lifetime 参照)。
  ```

- **由来 cite**: PR #216 で `weekly_review_reminder.enabled` の provisional change を config comment のみで tracking した実例を inline cite
- **派生プロジェクト波及**: `~/.claude/rules/common/patterns.md` への追加で techbook-ledger / auto-review-fix-vc に自動波及

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per、patterns.md 編集のため)
- [ ] `docs/adr/adr-039-experimental-feature-standard-pattern.md` § Bounded Lifetime 6-point checklist に provisional todo tracking item 追加 (~5 行)
- [ ] `~/.claude/rules/common/patterns.md` § Experimental Feature 設計時の参照必須 に同旨 note 追加 (~3 行)
- [ ] markdownlint clean (両 file)
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- ADR-039 § Bounded Lifetime に provisional state todo tracking checklist item が追加される
- `~/.claude/rules/common/patterns.md` に同旨 note が追加される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及
- future PR で provisional state を導入する際、config comment のみで tracking せず todo entry も作成する慣行が確立される

#### 詰まっている箇所

なし。Effort XS、ADR + global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。

---

### 順位 219: `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に「commit description 言及 ≠ 実装完了」明文化 (PR #216 post-merge-feedback T3-3 採用)

> **動機**: PR #216 cleanup 作業中、analyzer (Claude) が「PR #215 commit description で 順位 215 を言及している = 実装完了」と naïve に判断し、当初 6 entries (147/151/212/213/214/215) 削除計画を立てた。実際にはユーザーの修正 + grep `"Defensive State Reset" ~/.claude/rules/common/coding-style.md` による実体確認の結果、順位 215 は **todo entry が PR #215 で追加されただけ** で実装は未着手だった (5 entries 削除が正解)。
>
> この naïve assumption は今後も analyzer / Claude が再発する可能性が高く、誤った削除を実施すると未実装タスクが docs から消える silent loss につながる。development-workflow.md に明文化することで、future Claude session 内で同 anti-pattern を構造的に防止する。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #3 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「本 PR で『commit description に順位 N 言及 = 実装完了』の naïve assumption から analyzer が誤った 6 entry 削除計画を立てた実観測。ユーザー修正で 5 entry に訂正。Effort XS、Severity Medium (analyzer の誤判定リスクが今後も継続)、Adoption Risk None」。Session T3-3 の単一ソースだが Severity Medium で採用条件成立。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #3、`~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック (編集対象)、PR #216 cleanup session log (誤判定 → grep 救出の経緯)、memory `feedback_verify_task_not_already_done` (関連 memory、再確認 verb-noun rule の前提となる「verify」step)、memory `feedback_global_config_backup` (snapshot 必須)。
>
> **実行優先度**: **Tier 3** — Effort XS。global rules への docs 追記 ~8 行で完結、`feedback_global_config_backup` snapshot を忘れない。
>
> **Status update (2026-08-12)**: 旧環境 rules snapshot (syncthing/.claude_old、2026-06-17 凍結) の実査で本タスクは旧環境でも未実施と確認。~/.claude/rules の再配置 (採否) が保留中のため、配置先確定 (docs/todo22.md の「旧 rules 採否判断」エントリ) 後に着手する。

#### 設計決定 (案)

- **追加先**: `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック の末尾近く (関連 guideline と隣接配置)
- **追加内容案**:

  ```markdown
  ### Commit description 言及は実装完了の証拠ではない

  PR commit description で「順位 N」「feature X 実装」「Y を追加」等を言及していても、**実際のファイル変更を `jj diff` / `grep` で確認するまで completion 判定してはならない**。

  特に多 commit PR (= 1 PR で複数の論理 unit を扱う場合):
  - 「順位 N 削除」commit と「順位 N 実装」commit が分かれていることがある
  - todo entry の追加 / docs 更新だけで実装本体が未着手な commit も存在する

  検証手順:

  1. commit description で言及されている feature / 順位 N を特定
  2. `jj diff -r <commit_id>` で実際のファイル変更を確認
  3. 実装対象 file を `grep` で確認 (例: 「Defensive State Reset」section が `~/.claude/rules/common/coding-style.md` に実在するか)
  4. 実体確認後に「完了」判定

  由来: PR #216 で analyzer が「PR #215 commit description で 順位 215 を言及 = 実装完了」と naïve 判定し誤った削除計画を立てた実観測 (ユーザー修正 + grep 救出で訂正)。memory `feedback_verify_task_not_already_done` と相補的 (前者は task 着手前の verify、本 rule は task 完了判定前の verify)。
  ```

- **enforcement layer**: 機械 lint は困難 (commit description の意味解析 + ファイル diff の cross-check が必要) だが、Claude が development-workflow.md を読む文脈で「明示的に書かれた rule」として機能、memory `feedback_no_unenforced_rules` 例外 (= 既存実践の明文化) を満たす
- **派生プロジェクト波及**: `~/.claude/rules/common/development-workflow.md` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に新 sub-section「Commit description 言及は実装完了の証拠ではない」を追加 (~25 行、設計決定の追加内容案 per)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に「commit description 言及 ≠ 実装完了」guideline が追加される
- 検証手順 (4 step) が明示される
- PR #216 事例が inline cite として記録される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及

#### 詰まっている箇所

なし。Effort XS、global rules への docs 追記のみ、`feedback_global_config_backup` snapshot を忘れない。
