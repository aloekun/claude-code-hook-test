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

### `no-ephemeral-todo-reference` rule の TOML positive test 追加 (PR #151 T1-#1 採用、**PR #152 で再観測**)

> **動機**: PR #151 の CodeRabbit nitpick (および本 PR で発見されなかった latent gap) で、`no-ephemeral-todo-reference` rule が TOML ファイルを extensions に持つ場合の positive test (= 実際に violation を検出することの assertion) が不在と判明。既存テスト `no_ephemeral_todo_self_exclusion_invariant_holds_on_deployed_toml` は self-exclusion 確認のみで、検出力の test ではない。
>
> **本タスクの位置づけ**: PR #151 post-merge-feedback Tier 1 #1 採用 → PR #152 post-merge-feedback で再確認 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。extensions 拡張が複数 PR にわたって反復する pattern (yaml/yml = PR #110、toml = PR #129?) があり、test gap が累積するリスクを構造的に防ぐ。PR #152 post-merge-feedback でも「yaml/yml test gap (PR #110) + TOML test gap (PR #151) の 2 PR 連続観測」と同根の指摘あり。
>
> **参照**: `.claude/feedback-reports/151.md` Tier 1 #1、`src/hooks-post-tool-linter/src/main.rs` test module

#### 作業計画

- [ ] test fixture: `.toml` 拡張子ファイルに `docs/todo3.md` 等の ephemeral 参照を含む 2-3 行 fixture を作成
- [ ] test ケース: `run_custom_rules` が 1 件以上の violation を返し、`type == "NO_EPHEMERAL_TODO_REFERENCE"` を確認
- [ ] negative test: 同じ TOML fixture で `docs/adr/adr-007.md` 等の permanent 参照は violation 0 件であることを確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- TOML 拡張子で rule が機能することが explicit test で seal される
- 将来 extensions から TOML を誤削除した場合に test fail で検出される

---

### UTF-8 マルチバイト boundary test を他の string-processing hooks に横展開 (PR #151 T2-#1 採用)

> **動機**: PR #151 で `byte_offset_to_line` の char-boundary panic bug を test 拡充 (UTF-8 漢字単独 needle) で発見した。同型関数 (byte offset から行番号変換 / needle 検索 + slice 操作) は他の string-processing hooks にも存在する可能性が高く、横展開 test で systemic 防御を確保すべき。
>
> **本タスクの位置づけ**: PR #151 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Medium / Effort M / Adoption Risk None)。test 拡充は単なるカバレッジ追加ではなく fault detection に直結することが実証済 (本 PR で副産物として 1 production bug 修正)。
>
> **参照**: `.claude/feedback-reports/151.md` Tier 2 #1、`src/hooks-post-tool-comment-lint-rust/src/main.rs:byte_offset_to_line` (PR #151 で修正済)、対象は `src/hooks-*` で string offset 操作を行う関数

#### 作業計画

- [ ] `grep -rn "as_bytes\|byte\|offset" src/hooks-*/src/` で類似処理を持つ hooks を列挙
- [ ] 各 hook で multi-byte boundary に晒される operation を識別 (byte slice / needle search / offset → line 変換 等)
- [ ] 対象 hook 毎に test fixture 追加: 漢字単独 / emoji / 結合文字 / BMP 外文字 のうち最低 1 パターン
- [ ] 検出された production bug は 1 行 fix で resolve (PR #151 と同じ pattern)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 全 string-processing hook が multi-byte boundary の panic に対して test で防御されている
- 横展開 test 実施過程で発見された production bug が修正される

---

### extensions 拡張時の test 追加 pattern をコード comment で明文化 (PR #151 T3-#2 採用、順位 124 と同 PR 推奨、**PR #152 で再観測**)

> **動機**: 順位 124 (TOML positive test) の根因である「extensions 配列を変更しても対応する test が追加されない」pattern を、`custom-lint-rules.toml` または `no_ephemeral_todo_reference_rule()` 関数の近傍コメントに明記。「extensions を変更した際は対応する positive/negative test を追加すること」のリマインダを次回 rule 変更時に目に入る位置に置く。
>
> **本タスクの位置づけ**: PR #151 post-merge-feedback Tier 3 #2 採用 → PR #152 post-merge-feedback で再確認 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。memory rule `feedback_no_unenforced_rules.md` に抵触するように見えるが、本 case は **既存実践の明文化 + 機械強制ではなく guide 効果** のため例外採用 (順位 122 と同じロジック)。PR #152 post-merge-feedback でも「point-of-edit reminder は enforcement ゼロでも omission 抑止効果あり」と同様の判断で再採用された。
>
> **参照**: `.claude/feedback-reports/151.md` Tier 3 #2、`.claude/custom-lint-rules.toml`、`src/hooks-post-tool-linter/src/main.rs`

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` の `no-ephemeral-todo-reference` rule entry の上に 2-3 行 comment 追加: 「⚠️ extensions を変更する場合: 同 PR で positive + negative test を `src/hooks-post-tool-linter/src/main.rs` に追加すること」
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 要否を検討 (`.claude/custom-lint-rules.toml` は project 個別なので deploy 不要)
- [ ] 順位 124 (TOML test 追加) の作業中に test の location を確認して、comment 内の path 参照を正確に書く
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 次回 extensions 変更時に rule 編集者が test 追加を忘れにくくなる
- comment が機械強制ではなく guide として機能する (PR review 時の checklist としても再利用可)

---

### CLAUDE.md § Cross-File Reference Lifecycle に多ファイル同時削除 retirement condition checklist を追加 (PR #153 T3-#2 採用)

> **動機**: PR #153 で旧 `docs/local-llm-offload-analysis.md` を `phase-d-outcomes.md` に分割した際 (3 ファイルは Phase E 採用昇格 = 2026-05-15 に retire 済)、retirement clause を **3 ファイル (analysis.md / history.md / phase-d-outcomes.md) 同時削除** に統一する作業が developer/AI の手動 review でしか担保されていなかった。advisor 指摘で明示的に「3 ファイルすべてに同じ retirement clause を書く」ステップを踏んだが、これは structural pattern として再利用可能 (今後の docs/* 50KB 分割でも同じ checklist が必要)。同パターンが drift すると ephemeral artifact の lifecycle 整合が崩れ、stale pointer が増殖するリスクあり。
>
> **本タスクの位置づけ**: PR #153 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。**既存実践 (PR #133 todo.md 分割 + PR #153 analysis.md 分割) の明文化 + 機械強制ではなく guide 効果** のため、`feedback_no_unenforced_rules.md` の例外条件 (順位 122 / 127 と同じロジック) を満たす。
>
> **参照**: `.claude/feedback-reports/153.md` Tier 3 #2、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle、`~/.claude/rules/common/docs-governance.md` § Retirement Workflow

#### 作業計画

- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に「多ファイル同時削除時の retirement condition consistency checklist」section を追加 (3-5 項目程度の bullet list)
  - 「N ファイルを同時削除する設計の場合、全 N ファイルの header に同一の retirement clause が記載されているか」
  - 「retirement workflow の Step 3 (参照更新) で `grep -rn '<filename>'` を全ファイル分実施したか」
  - 「新ファイル追加時に既存ファイルの retirement clause にも追記したか」
  - 「参照先 (ADR / docs-governance.md) が permanent artifact であることを確認」
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) は `~/.claude/` global 配下なので自動波及
- [ ] グローバル設定変更前に `~/.claude/` snapshot 取得 (memory rule `feedback_global_config_backup.md` 適用)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 次回多ファイル分割 (例: history.md 50KB 接近時) で同 checklist を踏むことで drift が構造的に防止される
- PR #133 (todo.md 分割) / PR #153 (analysis.md 分割) の successful pattern が明文化され、3 例目以降の reproducibility が確保される

---

### `let _ = write_*` swallowed error 検出 custom lint rule (PR #155 T1-#1 採用) ★ Bundle l

> **動機**: PR #155 simplicity-review が BLOCKING 指摘した `write_skip_report` の Result を `let _ =` で silent drop していた問題の **構造的防止層**。同 PR の `write_report` 経路 (line 108-117) では `if let Err(e) { log_stage(...) }` pattern が確立されていたにもかかわらず、新規追加された I/O write 関数で reuse されず再発した。今後の I/O 関数追加で同 anti-pattern が混入することを機械的に防ぐ。
>
> **本タスクの位置づけ**: PR #155 post-merge-feedback Tier 1 #1 採用 (Severity High / Frequency Medium / Effort S / Adoption Risk None)。Bundle l (PR #155 由来の再発防止策バンドル) のコア。
>
> **参照**: `.claude/feedback-reports/155.md` Tier 1 #1、`src/cli-push-runner/src/stages/lint_screen.rs:381-385` (`write_skip_report_logged` 抽出例)、`.claude/custom-lint-rules.toml`

#### 設計決定

- **regex pattern**: `let\s+_\s*=\s+write_\w+\(`
- **extensions**: `["rs"]`
- **severity**: `error` (BLOCKING の再発を機械的に防止)
- **scope 限定**: `write_` prefix で I/O 書込関数のみを対象 → 他の `let _ = expression` (e.g. `let _ = stream.flush()`) は false positive にしない設計
- **fix 指示**: 「`if let Err(e) = write_*(...) { log_stage(STAGE, &format!(...)); }` または既存の `*_logged` ヘルパー抽出パターンを使用」

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に rule⑩ として entry 追加 (pattern / extensions / severity / fix message)
- [ ] `src/hooks-post-tool-linter/src/main.rs` に positive / negative unit test を追加:
  - positive: `let _ = write_foo(...);` で violation 検出
  - positive: `let _ = write_skip_report(path);` で violation 検出
  - negative: `if let Err(e) = write_foo(...) { ... }` で violation 検出なし
  - negative: `let _ = stream.flush();` (write_ 以外) で violation 検出なし
- [ ] deployed self-exclusion test: 派生プロジェクトの `.claude/custom-lint-rules.toml` 自身が rule⑩ に違反しないことを assert
- [ ] dogfood: 既存 codebase で本 rule を実行し new violations が現状ゼロであることを確認 (PR #155 land 後の baseline)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `let _ = write_<anything>(...);` が機械検出される
- 既存 codebase (Bundle k-1 land 後の master) で本 rule の violation 数がゼロ (= silent failure 経路が現状クリーン)
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) deploy 経由でも同更新が反映される

---

### lint-screen LLM への git diff format 文字列 magic-number 除外 (PR #155 T2-#1 採用) ★ Bundle l

> **動機**: PR #155 self-dogfood で simplicity-review が `similarity index 100%\n` の `100%` を mistral:7b が **magic-number として false positive 検出**した事例を観測。`similarity index` / `index ` (hex hash) / `@@ -1,1 +1,1 @@` 等の git diff format 文字列は **push ごとに出現**し、mistral:7b が無効な signal として混入させる。lint-screen の signal-to-noise 比を下げ、reviewer cross-check の負荷も増やす。
>
> **本タスクの位置づけ**: PR #155 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。Bundle l の review 精度改善層。Phase E 採否判定の前提となる FP rate 低減に直接寄与。
>
> **参照**: `.claude/feedback-reports/155.md` Tier 2 #1、PR #155 lint-screen-report.md 行 360 / 369 (FP 観測)、`src/cli-push-runner` lint-screen 前処理 or LLM prompt

#### 設計決定の余地

- **修正箇所候補**:
  - (a) `cli-push-runner` の lint-screen diff 送信前処理 — Rust 側で diff hunk parsing 時に `similarity index` / `index ` / `@@` 行を strip して mistral:7b に渡す
  - (b) `cli-finding-classifier` の `prompts/lint-screen.txt` — LLM プロンプトに「git diff format strings (similarity index, index, @@) の数値は magic-number ではない」と明示
- **推奨は (a)** — 決定論的 (regex で strip)、LLM 信頼に依存しない。(b) は agreement rate の variance を増やすリスク
- **副次効果**: filter で `.md` ハンクを drop した Bundle k-1 順位 123 と同型の前処理層拡張、設計の一貫性

#### 作業計画

- [ ] `src/cli-push-runner/src/stages/lint_screen.rs` の `filter_excluded_hunks` の隣に、または別関数として diff format meta-line stripping を実装
- [ ] strip 対象 (regex): `^similarity index \d+%$` / `^index [0-9a-f]+\.\.[0-9a-f]+( \d+)?$` / `^@@ .* @@.*$` (@ 行は hunk header だが LLM 解釈で magic-number 化されやすい)
- [ ] unit test: 各 meta-line が strip されること / 通常の `+`/`-` 行は保持されること
- [ ] Phase D dogfood で再走 (LINT_SCREEN_ENABLED=true で 1-2 PR) して FP rate が下がることを観測
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `similarity index 100%` 由来の magic-number FP が消滅
- 累積 dogfood data points で signal-to-noise 比が改善 (mistral:7b の findings 数 / FP 数の比率で測定)
- reviewer cross-check の負荷低減 (review iteration 数の減少傾向で間接観測)

#### 詰まっている箇所

- 修正箇所 (a) vs (b) の判断は dogfood 観測の蓄積が必要かもしれない。先に (a) を実装して残存 FP を見てから (b) 判断する 2 段階アプローチも valid

---

### `write_skip_report_logged` error path regression test (PR #155 T2-#2 採用)

> **動機**: PR #155 simplicity-review BLOCKING 修正後の `write_skip_report_logged()` 関数に error path の regression test が存在しない。`fs::write` 失敗時に `log_stage` が呼ばれることを assert する明示的 test がないと、将来の refactor で再度 silent failure に逆戻りするリスクがある。Bundle k-1 self-dogfood で実証された「log_stage 経路を test で seal する」pattern を正式化。
>
> **本タスクの位置づけ**: PR #155 post-merge-feedback Tier 2 #2 採用 (Severity Medium / Frequency Low / Effort M / Adoption Risk None)。Severity Medium 単独で rubric ✅ 条件を満たす。
>
> **参照**: `.claude/feedback-reports/155.md` Tier 2 #2、`src/cli-push-runner/src/stages/lint_screen.rs:381-385` (`write_skip_report_logged`)

#### 設計決定の余地

- **error injection 方法**:
  - (a) tempdir を作成し `chmod 000` (Unix) または read-only attribute (Windows) で書込不可化
  - (b) 既存ファイルを output_path として渡し、それを別プロセスで握って lock を取る (cross-platform に難しい)
  - (c) `output_path` を存在しない深い path (例: `/nonexistent/dir/that/cannot/be/created/report.md`) にして `create_dir_all` 失敗を誘発
  - 推奨: **(c)** — cross-platform、追加 dep なし、Windows でも reliable
- **assert 対象**: `log_stage` が「skip: skip-report 書き出し失敗」を含む message で呼ばれること。`log_stage` は副作用 (stdout への line) のため、test harness で stdout capture が必要 or `log_stage` 自体を test mock 化する

#### 作業計画

- [ ] `log_stage` の test mock 化を検討 — `STAGE` 引数 + format string をキャプチャできる test helper の有無を確認
- [ ] error injection 方式 (c) で test を実装: `write_skip_report_logged("/nonexistent/.../report.md")` 呼出が panic せず log_stage 経由で報告すること
- [ ] integration 経路の test も検討: full `run_lint_screen` flow で skip-report 書込失敗時のログ確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `write_skip_report_logged()` の error path で `log_stage` 呼出が機械検証される
- 将来の refactor で silent drop に戻った場合に test fail で検出
- Bundle k-1 self-dogfood で得た「log_stage 経路を test で seal する」pattern が cli-push-runner 全体に展開可能な reference 化

#### 詰まっている箇所

- `log_stage` の現行 API が test friendly でない可能性 (現在は直接 stdout への println / eprintln 系の可能性)。test 友好化のための小 refactor が effort M に含まれる可能性あり

---

### lint_screen.rs magic-number 検出ルールで `similarity index NN%` を FP 除外 (PR #156 T1 #1 採用)

> **動機**: PR #155 (Bundle k-1) self-dogfood + PR #156 (Phase E) で `lint_screen` の magic-number 検出ロジックが git diff format の `similarity index 100%` / `similarity index 75%` 等の数値を magic-number FP として報告する事象を 2 PR 連続観測。git diff ヘッダー (`similarity index`, `@@ ...`, `index ...`) は file rename / move を含む PR で必ず出現するため Frequency Medium の構造的 FP。Effort S 程度の除外ルール追加で signal/noise 比を改善できる。
>
> **本タスクの位置づけ**: PR #156 post-merge-feedback Tier 1 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。既存の **順位 130 (lint-screen LLM への git diff format 文字列 magic-number 除外)** と root cause / target が同一だが、本エントリは「**実装側の除外設定追加**」という具体策にフォーカス。順位 130 が「prompt 改修 or 前処理 filter のいずれか」を選択肢として持つのに対し、本エントリは **前処理 filter** に確定して着手する位置づけ。順位 130 を本エントリで supersede する形で land 可能。
>
> **参照**: `.claude/feedback-reports/156.md` Tier 1 #1、`src/cli-push-runner/src/stages/lint_screen.rs`、`src/cli-finding-classifier/prompts/lint-screen.txt`、順位 130 (関連 entry)

#### 設計決定 (案)

- **配置**: `cli-push-runner` lint-screen stage の前処理 layer (LLM 呼出前に diff を sanitize)
- **除外対象 (Phase 1)**: `similarity index NN%` 行を `cli-push-runner/src/stages/lint_screen.rs` の diff 整形段階で削除
- **除外対象 (Phase 2 候補)**: `@@ -N,M +N,M @@`、`index abc..def NNNNNN`、`new file mode NNNNNN`、`rename from ...` / `rename to ...` 等の git diff metadata 行も同様に除外
- **テスト**: diff fixture を追加 (file rename を含む synthetic diff) → lint_screen 経由で magic-number FP が 0 件であることを検証
- **派生プロジェクト deploy**: `cli-push-runner` exe は本リポジトリ専用、deploy 不要

#### 作業計画

- [ ] `src/cli-push-runner/src/stages/lint_screen.rs` の diff 取得部分で `similarity index NN%` 行を正規表現で除外する filter を追加
- [ ] eval fixture に rename 含む 1 件追加 (例: `eval16-file-rename.diff`)、`lint-screen-evals.json` に baseline 登録
- [ ] integration test: 該当 fixture で magic-number finding 0 件を assert
- [ ] 順位 130 entry に「順位 132 で superseded」note を追加 (または 順位 130 を本エントリに統合する形で削除)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `similarity index NN%` を含む diff で magic-number FP が報告されない
- file rename を含む現実 PR で lint_screen-report.md に該当 FP が出ない
- eval fixture が CI / cargo test で常時検証される

#### 詰まっている箇所

- 順位 130 との重複処理方針 (supersede vs merge) は実装着手時に判断。両 entry が登録されている状態で先に本エントリを着手すると 順位 130 を削除する流れが自然

---

### docs-governance §Retirement Workflow に「diff context 由来 false alarm 防止 = 必ず grep で実ファイル確認」を明記 (PR #156 T3 #1 採用)

> **動機**: PR #156 で ephemeral 4 ファイル retire を実施した際、`grep` 結果に含まれる **diff context 行が実ファイルの最新内容ではなく PR 直前の状態を反映する** ため、削除対象ファイルへの参照が「残存」と誤検出される false alarm が 5 件以上発生。fact-check の grep 実行に時間を要した。XS の文言追加で将来セッションの reviewer / Claude が同一の確認コストを繰り返すことを防止できる。ephemeral 退役ワークフローは今後も繰り返されるため Frequency Medium。
>
> **本タスクの位置づけ**: PR #156 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。`feedback_no_unenforced_rules.md` 例外条件 = 既存実践の明文化 + guide 効果のため採用 (順位 122 / 127 と同じロジック)。
>
> **参照**: `.claude/feedback-reports/156.md` Tier 3 #1、`~/.claude/rules/common/docs-governance.md` §Retirement Workflow

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` §Retirement Workflow の Step 3 (参照更新) に「diff context 由来 false alarm 防止」note 追加 (2-3 行)
  - 「`grep -rn '<filename>'` で hit した参照は **必ず該当ファイルを Read で開き、最新内容に対象参照が実在することを確認** する。diff context は PR 直前の旧状態を反映するため、retire 対象ファイルへの参照が context として残存しているように見えても、現行 working copy では既に削除されている場合がある」
  - 具体例: PR #156 (4 ファイル同時 retire) で 5 件以上の false alarm が発生
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) は `~/.claude/` global 配下なので自動波及
- [ ] グローバル設定変更前に `~/.claude/` snapshot 取得 (memory rule `feedback_global_config_backup.md` 適用)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 次回 ephemeral 退役 workflow で同 false alarm が発生しても、明文化された手順により fact-check の認知コストが低減する
- guide として PR review / Claude session 双方で参照可能

#### 詰まっている箇所

なし。Effort XS、global rule への追記のみで副作用最小。

---

### ADR-035 に docs-only PR 評価の明示的な適用外基準リストを追加 (PR #156 T3 #2 採用)

> **動機**: ADR-035 は docs-only PR の **分類基準** (どの PR が docs-only か) は定義しているが、**除外される評価観点** (docs-only PR で適用すべきでない code-logic 系評価項目) が明示されていない。PR #156 (Phase E、docs-only) で reviewer が mutation / error handling / test coverage 等の code-logic criteria を docs-only PR に適用しかけて unnecessary review overhead が発生する潜在リスクが観測された。明示することで将来セッションでの reviewer による criteria 誤適用を防止できる。
>
> **本タスクの位置づけ**: PR #156 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Low / Effort S / Adoption Risk None)。Severity Medium の根拠 = 誤適用による unnecessary review overhead / 開発体験劣化。`feedback_no_unenforced_rules.md` 例外条件 = ADR (= 設計判断 doc) への追加で機械強制ではなく reviewer / Claude の judgment 補助。
>
> **参照**: `.claude/feedback-reports/156.md` Tier 3 #2、`docs/adr/adr-035-doc-evaluation-policy.md`

#### 設計決定 (案)

- **配置先**: `docs/adr/adr-035-doc-evaluation-policy.md` 内に新 section 「docs-only PR で適用しない評価観点」を追加
- **適用外基準リスト (案)**:
  - **Mutation / immutability**: docs に code mutation は存在しないため適用しない
  - **Error handling**: docs に error path は存在しないため適用しない
  - **Test coverage**: docs に test は不要なため適用しない (test 文言の追加自体は除く)
  - **Function length / complexity**: docs に関数は存在しないため適用しない
  - **DRY / YAGNI**: docs では intentional な重複・冗長な記述が reader にとって有益な場合があるため適用しない (例: 同じ概念を複数 section で説明する)
  - **Magic number / hardcoded value**: docs 中の数値は説明的記述で magic ではないため適用しない
- **適用される評価観点** (既存 ADR-035 で定義済みのものを再確認):
  - Cross-reference lifecycle (permanent → ephemeral 禁止)
  - Markdown syntax / lint
  - Anchor link validity
  - Retirement workflow 整合
  - 内容の正確性 / typo

#### 作業計画

- [ ] `docs/adr/adr-035-doc-evaluation-policy.md` の構造確認 (既存 section header の慣習)
- [ ] 「適用外基準リスト」section を追加
- [ ] 既存 ADR の評価観点 section との整合性確認 (重複説明の有無、cross-reference の追加)
- [ ] markdownlint clean 確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- docs-only PR の reviewer / Claude が「mutation / DRY 等は適用しない」を ADR から逆引きできる
- 将来の docs-only PR 評価で criteria 誤適用が systemic に発生しなくなる
- markdownlint clean

#### 詰まっている箇所

なし。Effort S、ADR への追記のみで副作用最小。

---

### todo entry の ADR 番号 hardcode 撤廃 — 「ADR-NNN (採番未確定、land 時に確定)」placeholder 採用 (順位 78 番号 conflict 2026-05-16 観測由来)

> **動機**: 順位 78 (旧 ADR-038 Rust timestamp arithmetic safety、PR #115 T3-1) は entry 登録時 (2026 年序盤) に新規 ADR として ADR-038 を予約のつもりで hardcode していたが、queue 滞留中に Bundle Z 系列の連続採用で `ADR-037 / 038 / 039 / 040` がすべて占有され、2026-05-16 セッションで番号 conflict が顕在化。順位 78 は ADR-041 への振り直しで個別対応済だが、queue 深度と滞留期間の積に比例して同型 conflict が再発する構造リスクが残る。
>
> **本タスクの位置づけ**: 順位 78 振り直し対応の **再発防止 convention**。採番予約簿 (`docs/adr/RESERVED.md` 等) は管理コストが過剰なため見送り、entry 登録時は placeholder で済ませて land 時の PR で空き番号を確定する運用に統一する (作業着手時に採番するだけの軽量運用、ユーザー判断 2026-05-16)。
>
> **参照**: 順位 78 entry ([docs/todo5.md](todo5.md) § ADR-041 Rust timestamp arithmetic safety + CLAUDE.md security 拡充)、`~/.claude/rules/common/docs-governance.md`
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule に 2-3 行追記。

#### 設計決定 (案)

- **配置先**: `~/.claude/rules/common/docs-governance.md` の `## Document Lifecycle Classification` 周辺、もしくは新規 `## ADR 採番の運用` section
- **追記内容案** (2-3 行):
  - todo entry / planning markdown で新規 ADR を予告する際は、番号を hardcode せず **`ADR-NNN (採番未確定、land 時に確定)`** placeholder で記述する
  - land 時の PR で `docs/adr/` を確認し空き番号を確定。同時に当該 entry / markdown / table 内の placeholder を実番号に置換
  - 採番予約簿の運用は行わない (queue 滞留 entry の管理コストが回収可能性に見合わない)
- **本タスクの効果**: queue 滞留 entry が後発 PR の採番と衝突する構造リスクを convention で予防、作業着手時の軽量採番で十分運用可能

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` に上記 placeholder 採用方針を 2-3 行追記
- [ ] 既存 todo entries 内に他の hardcode された ADR 予告番号が残っていないか `grep -rn 'ADR-[0-9]\+ (新規)' docs/` 等で確認 (順位 78 振り直し後の漏れ検出)
- [ ] 派生プロジェクト deploy には影響なし (global rule のみ)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `docs-governance.md` に ADR 番号 hardcode 撤廃方針が明記される
- 将来 todo entry で新規 ADR を予告する際に placeholder 形式が convention として参照可能
- 既存 todo に他の hardcode 予告番号が残っていないことが grep で確認される

#### 詰まっている箇所

- ルール追加自体は機械検知不可だが、`feedback_no_unenforced_rules.md` 例外 = 既存実践の明文化 + 簡素な代替手順を提示。grep ベースの後付け検証も容易 (`grep -nE 'ADR-[0-9]+ \(新規\)' docs/`)

---

## 既知課題 (記録のみ、本セッションで未対応)

### post-merge-feedback workflow が長時間 stale marker を残す問題 (PR #119 marker observed 2026-05-15)

> **観測**: 2026-05-15 セッション開始時、`.claude/feedback-reports/119.md.failed` marker が **606,269 秒 (約 7 日)** 経過した状態で UserPromptSubmit hook により検出。PR #119 (ADR-038 Phase 5: cli-finding-classifier 統合) のマージ後に起動した post-merge-feedback workflow (run id `20260506-141736-post-merge-feedback-for-119`) が abrupt 終了 (kill -9 / SIGKILL / power loss / OOM 等) で中断され、Drop guard 経路を経由せず orphan reaper の 1500 秒閾値も大幅に超過した state で marker が残存。
>
> **解釈**: 単発事象として記録のみ留め、即時手動 recovery (`pnpm exec takt -w post-merge-feedback -t 'post-merge-feedback for #119'`) は実施しない (PR #119 は 7 日前 land 済で、対応するレビュー知見は後続 PR で既に消化済の可能性が高い)。次回 stale marker の自然 cleanup 機構 (ADR-030 §L2 orphan reaper / D-7 / 順位 64) の dogfood で本 marker も同時に reap されるかを観察する材料として残す。
>
> **本タスクの位置づけ**: **既知課題のみ、todo 着手は予定なし**。merge pipeline の長期化 / abrupt 終了が原因と推定されるが、systemic な再発 (Frequency Medium 以上) を確認するまで実装側の改修は scope 外。Bundle c-1 (PR #154、L1 Drop guard + L2 reaper) で recovery 機構自体は実装済のため、本 marker は単に「reaper 投入前に取り残された artifact」として扱う。
>
> **参照**: `.claude/feedback-reports/119.md.failed`、ADR-030 §L1/L2 spec、Bundle c-1 (PR #154、L2 orphan reaper の本セッションでの初回完全 dogfood)

#### 想定される追加観察項目 (Frequency が上がった場合に着手)

- abrupt 終了 (Drop guard 不発) の root cause: takt subprocess 階層のどこで SIGKILL が起きたか (cli-merge-pipeline / takt 本体 / Claude Code session 終了 etc.) の事後 forensic
- L2 orphan reaper が古い marker をどう扱うか (immediate cleanup vs warn-only vs leave alone) の policy 評価
- 7 日経過 marker を Claude Code セッション開始時に毎回提示するべきか (UserPromptSubmit hook の signal-to-noise) の検討

---
