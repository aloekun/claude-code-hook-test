# TODO (Part 8)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo6.md がファイルサイズ 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #143 T3-#1 採用時 = 2026-05-11)。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md / todo7.md の既存エントリは引き続き有効、相互に独立。新セッションでは九つすべてを確認すること (todo.md / todo2-8.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

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

### working copy staleness 検出 hook 2 段構え: SessionStart + PreToolUse (本セッション cleanup-stale-rank-39 由来)

> **動機**: 本セッション (PR cleanup-stale-rank-39 作業中) で「local working copy が stale parent (master と sibling) のまま docs/todo*.md を読み込み、master 上で既に削除済の entry 2 件 (順位 104 / 126) を『stale entry として削除する』と誤判定」failure mode を実証した (実 stale entry は 1 件のみだった)。memory rule `feedback_verify_task_not_already_done.md` (todo 着手前に既実装検証 → stale entry 削除に再目的化) は強制力ゼロで再発確実 = memory rule 全般の限界 (`feedback_no_unenforced_rules.md` 原則の自己事例)。Claude Code Web との並列セッション運用前提下では構造的に同 mode が発生する。
>
> **本タスクの位置づけ**: 本セッション post-merge-feedback 相当の structural defense。`feedback_no_unenforced_rules.md` 例外条件 = **2 つの hook で機械強制可能**。案 A (予防層 = session 起動時に状況認識) + 案 B (最終 backstop = stale 状態での編集を hard block) のセット二段構え。
>
> **参照**: 本セッション (2026-05-18) PR cleanup-stale-rank-39 root cause 分析 (ユーザー対話)、memory `feedback_verify_task_not_already_done.md`、ADR-039 (Experimental feature 標準パターン)
>
> **実行優先度**: 🚀 **Tier 1** — Effort Medium (案 A ~80 行 + 案 B ~30 行)。本セッションの実観測 failure mode に対する直接対策で、並列セッション運用が常態化している現状で再発確率が高い。

#### 設計決定 (案 A + B)

**案 A: SessionStart hook で `jj git fetch` + lineage 報告**

- 配置: `src/hooks-session-start/` (既存があれば拡張、なければ新設)
- 動作:
  1. `jj git fetch --quiet` を timeout 付き (3 秒) で実行
  2. `master..@-` / `@-..master` の commit 数を比較
  3. additional context として AI に出力 (例):
     ```text
     [working-copy-freshness]
     @: lmzvnwlu (parent: #159)
     master: #161 (2 commits ahead of @-)
     warning: working copy is behind master; recommend `jj rebase -d master`
     ```
- kill-switch: `hooks-config.toml` の `[session_start]` section に `enabled` flag
- 最適化: `.git/FETCH_HEAD` mtime を確認して「5 分以内なら fetch skip」 (network cost 抑制)
- fail-open: fetch timeout / 失敗時は warning なしで pass-through (block しない、AI 操作は継続可能)

**案 B: PreToolUse hook で stale 時の `docs/todo*.md` edit を block**

- 配置: 既存 `src/hooks-pre-tool-validate/` に統合 (~30 行追加)
- 動作: Edit / Write の対象が `docs/todo*.md` 系列のとき、master と @- の lineage 確認 → master が ahead なら hard block
- block message:
  ```text
  ❌ working copy parent (#X) is N commits behind master (#Y).
  docs/todo*.md は state を反映する artifact のため、master と同期した状態で編集すること。
  修正手順: `jj git fetch && jj new master`
  ```
- scope 限定: `docs/todo*.md` のみ block (コード / config までは過剰、false positive リスク)
- 案 A と異なり、本 hook は fail-closed (lineage 判定不能なら block) で安全側に倒す

#### 作業計画

- [ ] 既存 SessionStart hook の有無確認 (`src/hooks-session-start/` または settings.json の `SessionStart` entry)
- [ ] `jj git fetch` の timeout / kill-switch / network 例外処理設計
- [ ] `master..@-` の lineage 計算ロジック実装 (`jj log -r "master..@-" --no-graph -T 'description'` 等)
- [ ] additional context 出力フォーマット決定 (一行 vs 複数行、AI 読み飛ばし耐性検証)
- [ ] `hooks-pre-tool-validate.exe` に `docs/todo*.md` edit block ロジック追加
- [ ] ADR 起案 (新 hook 設計 + ADR-039 experimental pattern 適用、land 時採番確定)
- [ ] dogfood 期間設定 (試験運用 flag で N 週間運用後採否決定)
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) deploy 検討
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- session 開始時に working copy が master より遅れている場合、AI が context 出力で即座に状況を認識する
- stale parent 状態で `docs/todo*.md` を編集しようとすると hard block + 修正手順 (`jj git fetch && jj new master`) 表示
- ADR-039 experimental pattern に従い kill-switch 装備 (network 異常 / feature branch 運用への退避経路)
- 派生プロジェクトでの動作確認

#### 詰まっている箇所

- `jj git fetch` の timeout が低速 network で頻発した場合の UX → 案 A は fail-open で warning なし pass-through、案 B は fail-closed (lineage 不能 = stale 扱い) で安全側に倒す trade-off
- master 判定ロジック: 現状 trunk-based 前提で master を正と扱う。feature branch 運用が始まると assumption が破綻するが、本リポジトリは当面 trunk-based のため問題なし。trunk 名 (master / main) は config 可能にしておく

---

### ADR-NNN (採番未確定、land 時に確定): Test Isolation Patterns for Multi-Condition Guards (PR #168 T3-#2 採用)

> **動機**: PR #120 W-001 で `enrich_with_classifier_skips_when_disabled` テストが OR-guard `if !config.enabled || state.findings.is_empty() { return; }` の責務混在 (vacuous assertion: 空 `classified_findings` → 空 `classified_findings` で早期 return 由来か他経路由来か判別不能) で書かれていた問題、および PR #168 で sentinel pattern + 直交 precondition setup により構造的解決した実装を、project-level ADR として永続化する。`~/.claude/rules/common/code-review.md` (global rule、順位 84 で追加済) の checklist entry を補完する形で、project ADR には rationale・具体実装例 (poll.rs)・PR #120 W-001 history を codify し、将来の複合 guard テスト実装者が独立して参照できるようにする。
>
> **本タスクの位置づけ**: PR #168 post-merge-feedback Tier 3 #2 採用。`feedback_no_unenforced_rules.md` の例外 = 既存実践 (PR #168 で実装済) の明文化 + project-specific context の補完。Severity Low / **Frequency Medium (PR #120 W-001 初発見 + PR #168 sentinel pattern 実装の 2 PR 横断)** / Effort M / Adoption Risk None。
>
> **参照**: `.claude/feedback-reports/168.md` Tier 3 #2、`src/cli-pr-monitor/src/stages/poll.rs` (`enrich_with_classifier_skips_when_disabled` / `enrich_with_classifier_skips_when_findings_empty`)、`~/.claude/rules/common/code-review.md` (順位 84 land 済 checklist entry)、PR #120 W-001 / PR #168 history
>
> **実行優先度**: 💎 **Tier 3** — Effort M。新規 ADR 1 件作成 (記述のみ、コード変更なし)。

#### ADR 番号

順位 135 codified policy (`~/.claude/rules/common/docs-governance.md`) に従い、本 entry では番号を `ADR-NNN (採番未確定)` placeholder とする。**land 時 PR で空き番号を確定**する (現時点既存: ADR-040 まで確定、ADR-041 は順位 78 で「Rust timestamp arithmetic safety」用に予約中)。本 entry が順位 78 より先に land する場合は ADR-041 を本件に割り当て、順位 78 を ADR-042 candidate に振り直す。逆に順位 78 が先に land する場合は本件を ADR-042 候補とする。

#### 作業計画

- [ ] `docs/adr/adr-NNN-test-isolation-patterns.md` を新規作成 (番号は land 時確定)
- [ ] 内容構成:
  - **問題**: PR #120 W-001 の vacuous assertion (検証対象 field が空のまま → 早期 return 由来か他経路由来か判別不能) で OR-guard test の責務混在が顕在化した経緯
  - **設計原則**: sentinel pattern (検証対象 field を pre-populate → survival assert で mutation 不発を明示) + OR-guard precondition assertion (短絡発火条件を test 内で明示し直交性を保証)
  - **実装例**: `enrich_with_classifier_skips_when_disabled` (左 arm = `!enabled` 単独) / `enrich_with_classifier_skips_when_findings_empty` (右 arm = `findings.is_empty()` 単独) の 2 variant 抜粋コード
  - **適用範囲**: 2+ 条件の OR/AND 早期 return を持つ pure function 系 test (副作用検証は別パターン、本 ADR の scope 外)
  - **既存資料との関係**: `~/.claude/rules/common/code-review.md` checklist entry (順位 84 land 済) を project-level rationale + 具体実装例で補完する layer
- [ ] `CLAUDE.md` の ADR リストに 1 行追加 (番号確定時)
- [ ] PR description で `docs/adr/adr-NNN-test-isolation-patterns.md` への link と「sentinel pattern + OR-guard test orthogonality を project codify」要約を明記

#### 完了基準

- ADR ファイルが新規作成され、PR #120 W-001 history + sentinel pattern + 2 variant 実装例が記述される
- CLAUDE.md の ADR リストに該当 entry が追加される
- 次回複合 guard test を含む PR を書く際の reference として poll.rs の doc comment などから ADR へリンク可能になる

#### 詰まっている箇所

なし。記述のみで実装変更不要。

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
