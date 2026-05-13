# TODO (Part 8)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo6.md がファイルサイズ 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #143 T3-#1 採用時 = 2026-05-11)。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md / todo7.md の既存エントリは引き続き有効、相互に独立。新セッションでは九つすべてを確認すること (todo.md / todo2-8.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### `takt-workflow-persona-without-model` rule コメント拡張 + ADR-007 case study 追記 (PR #150 T1-#1 採用、実体 Tier 3)

> **動機**: PR #150 の Major fix (4 fields 追加) で「enumeration 方式は新規 field 追加時に明示的拡張が必要」という設計判断が再確認された。custom-lint-rules.toml ルール⑨ のコメントに field 拡張手順 (どの workflow を grep して enumeration に追加するかの手順) を明記すれば、次回 takt yaml schema 拡張時の rule 更新漏れリスクを低減できる。同 PR で ADR-007 にも「enumeration-based 正規表現層の好例」として case study 追記すれば、次回 lint rule 設計判断の prior assumption として再利用可能。
>
> **本タスクの位置づけ**: PR #150 post-merge-feedback で **Tier 1 #1 として採用** されたが、実体は「コメント追記 + ADR docs 修正」のみで mechanical enforcement なし。**ユーザー判断で Tier 3 に reclassify** (rule 追加 / docs 修正 は judgment-required で機械強制力がないため Tier 1 ではない)。analyzer 分類器に Tier 定義の誤解がある (`feedback_no_unenforced_rules.md` と関連)。Severity Medium / Frequency Low (1 PR) / Effort XS / Adoption Risk None。
>
> **参照**: `.claude/feedback-reports/150.md` Tier 1 #1、`docs/adr/adr-007-custom-linter-layer-boundary.md`、`.claude/custom-lint-rules.toml` ルール⑨ (line 295-)

#### 作業計画

- [ ] ルール⑨ のコメントに「field 拡張手順 (1) `.takt/workflows/*.yaml` を grep / (2) `persona:` 直後に出現する未列挙 field を pattern alternation に追加 / (3) regression test 追加」を 4-5 行追記
- [ ] `docs/adr/adr-007-custom-linter-layer-boundary.md` に「Case study: takt-workflow-persona-without-model (enumeration-based 正規表現層、Rust regex lookahead 非対応の pragmatic 対処)」section を追記
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ルール⑨ コメントに field 拡張手順が記載され、次回 takt yaml schema 拡張時の rule 更新フローが文書化される
- ADR-007 に enumeration-based pattern の case study が記録される
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 経由でも同更新が反映される

---

### `takt_workflow_persona_detects_required_permission_mode_violation` doc 修正 + 残り 3 fields 個別 fixture test 追加 (PR #150 T2-#1 採用)

> **動機**: PR #150 CR Major fix で 4 fields (`output_contracts` / `pass_previous_response` / `required_permission_mode` / `parallel`) を pattern に追加したが、regression test は `required_permission_mode` の 1 case のみ。doc comment は「4 fields regression test」と主張しているが実態と乖離 (`pass_previous_response` は非トリガー位置にあり、`output_contracts` / `parallel` は不在)。将来 regex 変更時に test 漏れに気付けない保守債が累積する。
>
> **本タスクの位置づけ**: PR #150 post-merge-feedback Tier 2 #1 採用。Severity Low / Frequency Medium (3 独立分析ソースが同一 finding) / Effort S / Adoption Risk None。
>
> **参照**: `.claude/feedback-reports/150.md` Tier 2 #1、`src/hooks-post-tool-linter/src/main.rs` L2108-2123

#### 作業計画

- [ ] `takt_workflow_persona_detects_required_permission_mode_violation` の doc comment を「`required_permission_mode` のみの代表 case (PR #150 CR Major 採用) を assert」に修正
- [ ] `pass_previous_response` 個別 fixture test 追加 (例: `persona: code-reviewer\n    pass_previous_response: false`)
- [ ] `output_contracts` 個別 fixture test 追加 (例: `persona: simplicity-reviewer\n        output_contracts:`)
- [ ] `parallel` 個別 fixture test 追加 (例: `persona: code-reviewer\n    parallel:` または該当箇所の構造に応じて)
- [ ] `cargo test` 全 pass + clean baseline test (`deployed_takt_workflows_have_clean_baseline_for_persona_model_rule`) も pass を確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 4 fields すべてに対応する individual fixture test が存在し、各 field の regex alternation 動作が機械検証される
- doc comment が test 実態と整合する
- 将来 alternation から 1 field を誤って削除した場合に test fail で検出される

---

### `development-workflow.md` Step 0 に「新 todo 着手前の既実装確認」チェックステップ追加 (PR #150 T3-#1 採用、補足: ユーザー判断採用)

> **動機**: PR #150 着手時に「順位 47 は PR #126 で既 land 済」という stale todo entry を memory rule `feedback_verify_task_not_already_done.md` 適用で発見・回避できた。memory にとどまる限り read 漏れリスクが残るため、canonical workflow doc (`~/.claude/rules/common/development-workflow.md`) Step 0 (Research & Reuse) に「新 todo 着手前に `jj log --limit 20 <keyword>` で既実装確認」step を正式追加すれば、AI エージェントの workflow 読込時の visibility が向上する。
>
> **本タスクの位置づけ**: PR #150 post-merge-feedback Tier 3 #1 採用。rule 追加は本来 `feedback_no_unenforced_rules.md` 適用で却下 zone だが、本 case は「stale entry 発見の具体的 grep コマンドが workflow 内で機械的に実行可能 (`jj log` は決定的)」+「memory rule の昇格 path 実例」としてユーザー判断で採用。Severity Medium / Frequency Medium (memory 既存 + 本 PR で再発) / Effort XS / Adoption Risk None。
>
> **参照**: `.claude/feedback-reports/150.md` Tier 3 #1、`~/.claude/rules/common/development-workflow.md` Step 0 (Research & Reuse)、memory `feedback_verify_task_not_already_done.md`

#### 作業計画

- [ ] `~/.claude/rules/common/development-workflow.md` Step 0 (Research & Reuse) 末尾または直後に「Stale task verification」サブステップ追加:
  - `jj log --limit 20 <keyword>` で既実装の有無を確認
  - 既 land を発見した場合は stale todo entry を docs/todo*.md / todo-summary.md から削除する形に re-purpose
- [ ] 既存 memory `feedback_verify_task_not_already_done.md` の content を canonical rule へ昇格させた旨を memory に追記 (or memory を削除して rule に統合)
- [ ] グローバル設定変更前に `~/.claude/` バックアップ取得 (memory `feedback_global_config_backup.md` 適用)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `development-workflow.md` Step 0 で「stale entry 確認」が canonical workflow として読まれる
- memory ファイルとの責任分離が明確 (rule = 公式手順、memory = session-specific 補足) または memory が rule に統合される
- 次回 todo 着手時に AI エージェントが自然に `jj log` 確認を行う

---

### ADR-040 `step_timeout` 説明に sublinear / KV cache locality clarification 追記 (PR #145 T3-#1 採用)

> **動機**: ADR-040 L42-48 の `step_timeout` 説明は「sublinear (3.33x)」と記述したが、本文中に「per-invoke latency が num_ctx に対して概ね線形に拡大する経験則」も併記しており、両者の関係が不明瞭。派生プロジェクトが reference table から 32K = 600s を読む際、なぜ formula `(num_ctx/8192)*180` で導出される 720s と乖離するかが直感的に分からない。clarification として「実測値 600s を正規値として採択、computed 720s は保守上限の目安、sublinear 性の根拠は KV cache locality 効果 (大規模 context で per-token efficiency 向上)」の 2-3 行追記が必要。
>
> **本タスクの位置づけ**: PR #145 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Low / Effort XS / Adoption Risk None)。永続 ADR の数値整合性確保。
>
> **参照**: `.claude/feedback-reports/145.md` Tier 3 #1、`docs/adr/adr-040-local-llm-context-size.md` L42-48

#### 作業計画

- [ ] ADR-040 § `step_timeout` 比例係数の根拠 に 2-3 行追記:
  - 実測値 600s を正規採択、computed 720s は保守上限見積もり
  - sublinear 性 (3.33x vs context 4x) の根拠 = KV cache locality 効果 (推定)
  - 派生プロジェクトでの derivation 時は実測 cargo test 経過時間の 2x margin を採用
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-040 の reference table と本文の formula が矛盾なく解釈可能になる
- 派生プロジェクトの porting 時に sublinear の根拠が永続記録から逆引きできる

---

### rule⑧ への paths filter 適用範囲検討 (順位 102 land 時に意図的保留、follow-up)

> **動機**: 順位 102 (PR #148 想定で land 中、Phase D D-3) で paths filter が lint runner に実装されたが、当初計画した rule⑧ への `paths = ["docs/**/*.md"]` migration は **意図的に保留**。理由: D-2 (PR #146、順位 101) で追加した「root-level MD (CLAUDE.md / README.md) からの `../docs/` 参照を fire = true positive で扱う」design intent が、`paths = ["docs/**/*.md"]` 適用で scope narrow されて壊れる (root-level MD の実 path が docs/ 配下ではないため rule 対象外になり、broken link 検出を失う)。本タスクで以下のいずれを採用するか検討する:
>
> 1. **保留継続** (現状維持): rule⑧ は `extensions = ["md"]` のみで run、root-level fire を保護
> 2. **broader glob**: `paths = ["**/*.md"]` で全 .md 受容 (= extensions filter と機能的同等、demonstration 用途)
> 3. **explicit list**: `paths = ["docs/**/*.md", "*.md", ".claude/**/*.md"]` で docs/ + root + .claude/ をカバー
> 4. **rule split**: rule⑧-docs (docs/ scope) + rule⑧-root (root scope) に分割
>
> **本タスクの位置づけ**: 順位 102 follow-up (Severity Low / Frequency Low = 1 観測 / Effort XS / Adoption Risk None)。実 production lint behavior に影響しない range で trade-off 評価。
>
> **参照**: PR #148 (順位 102 land) の TOML rule⑧ コメント、PR #146 (D-2、順位 101) の `md_no_docs_relative_detects_root_*` tests

#### 作業計画

- [ ] 4 案の trade-off を ADR-007 amendment (順位 104) と整合させて評価
- [ ] 採用案を `.claude/custom-lint-rules.toml` rule⑧ に適用 (案 1 保留継続なら no-op だが、本エントリ削除で結論明示)
- [ ] 既存 test (`md_no_docs_relative_*` group) との整合性確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- rule⑧ scope の設計判断が ADR-007 amendment と本タスク entry で明確化される
- 同型 trade-off (filter scope narrow vs coverage 保持) を将来 rule 追加時に逆引きできる

---

### `coding-style.md § Cross-File Reference Lifecycle` に「ephemeral → permanent 知識移管 edit order」追記 (PR #145 T3-#3 採用)

> **動機**: PR #145 で lib.rs L128-139 dogfood evolution コメントを ADR-040 に migrate した際、edit 順序が曖昧だった (ADR-040 を先に作るべきか、lib.rs 側の参照削除を先にすべきか)。同パターンが (1) lib.rs コメント → ADR-040、(2) Phase C/D empirical data → ADR-040 で 2 回観測。既存の Cross-File Reference Lifecycle ルール は「参照方向の制約」(permanent → ephemeral 禁止) に特化しており、移管作業の edit order checklist は complementary で重複なし。次回同型の永続化作業 (ephemeral 計画書 retire 時の permanent value 移管 等) で再発防止策として codify する。
>
> **本タスクの位置づけ**: PR #145 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/145.md` Tier 3 #3、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle

#### 提案する 3 ステップ原則

1. **permanent target 先行作成・validate**: 移管先の permanent artifact (ADR / stable docs) を先に作成し、内容の正確性 (cross-reference の妥当性 / 数値整合性 / markdownlint pass) を確認
2. **参照追加**: ephemeral 側 (lib.rs コメント / config コメント / scratch markdown 等) から permanent への参照 link を追加 (1-2 行)
3. **参照元削除**: ephemeral 側の冗長な内容を削除し、参照 link のみ残す。同一 commit で 3 step すべてを実施

#### 作業計画

- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle 末尾に「ephemeral → permanent 知識移管 edit order」 subsection を追加
- [ ] 3 ステップ原則を inline で記述、PR #145 (lib.rs L128-139 → ADR-040) を実例として cite
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 計画も検討
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 次回 permanent 化作業時に edit order が決定論的に決まる
- ephemeral 計画書 retire 時の permanent value 移管プロセスが checklist 化される

---
