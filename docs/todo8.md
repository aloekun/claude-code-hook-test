# TODO (Part 8)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo6.md がファイルサイズ 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して PR #143 T3-#1 採用時 = 2026-05-11 から新規エントリは本ファイルに記録していた。**本ファイルも 60KB に到達したため、PR #172 仕組み化方針切替セッション = 2026-05-25 以降の新規エントリは [docs/todo9.md](todo9.md) へ移行**。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md / todo7.md / todo9.md の既存エントリは引き続き有効、相互に独立。新セッションでは十つすべてを確認すること (todo.md / todo2-9.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

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

> **動機**: 順位 78 (旧 ADR-038 Rust timestamp arithmetic safety、PR #115 T3-1) は entry 登録時 (2026 年序盤) に新規 ADR として ADR-038 を予約のつもりで hardcode していたが、queue 滞留中に Bundle Z 系列の連続採用で `ADR-037 / 038 / 039 / 040` がすべて占有され、2026-05-16 セッションで番号 conflict が顕在化 (ADR-041 へ振り直し)。さらに 2026-05-22 に順位 139 (PR #168 follow-up) が ADR-041 を取得したため順位 78 を再 placeholder 化 = **同一 entry が 3 回 (038 → 041 → NNN) 番号変更を経た実証ベース**で、queue 深度と滞留期間の積に比例して同型 conflict が再発する構造リスクを convention で予防する必要がある。
>
> **本タスクの位置づけ**: 順位 78 振り直し対応の **再発防止 convention**。採番予約簿 (`docs/adr/RESERVED.md` 等) は管理コストが過剰なため見送り、entry 登録時は placeholder で済ませて land 時の PR で空き番号を確定する運用に統一する (作業着手時に採番するだけの軽量運用、ユーザー判断 2026-05-16)。
>
> **参照**: 順位 78 entry ([docs/todo5.md](todo5.md) § ADR-NNN Rust timestamp arithmetic safety + CLAUDE.md security 拡充)、`~/.claude/rules/common/docs-governance.md`
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

### working copy staleness 検出 hook 2 段構え + stale todo entry 既実装 grep 提示 (PR cleanup-stale-rank-39 由来 + PR #150 T3-#1 統合 2026-05-25)

> **動機**: 本セッション (PR cleanup-stale-rank-39 作業中) で「local working copy が stale parent (master と sibling) のまま docs/todo*.md を読み込み、master 上で既に削除済の entry 2 件 (順位 104 / 126) を『stale entry として削除する』と誤判定」failure mode を実証した (実 stale entry は 1 件のみだった)。memory rule `feedback_verify_task_not_already_done.md` (todo 着手前に既実装検証 → stale entry 削除に再目的化) は強制力ゼロで再発確実 = memory rule 全般の限界 (`feedback_no_unenforced_rules.md` 原則の自己事例)。Claude Code Web との並列セッション運用前提下では構造的に同 mode が発生する。
>
> **統合履歴 (2026-05-25)**: 旧 順位 122 (`development-workflow.md` Step 0 に「新 todo 着手前の既実装確認 `jj log --limit 20 <keyword>`」step 追加 = PR #150 T3-#1 採用) を本 task に統合。rule 化 (= docs 追加) では session 毎に読み込みコストがかかり別セッションで結果が一定にならない課題が PR #172 (順位 144 hook 化成功事例) で明確化、仕組み化に方針切替。stale 検出 hook が `docs/todo*.md` edit 時に発火する際、合わせて既実装の有無を grep して結果を提示する形で 順位 122 の機能を吸収する (`feedback_pipeline_over_rules.md` 適用)。
>
> **本タスクの位置づけ**: 本セッション post-merge-feedback 相当の structural defense + 旧 順位 122 機能統合。`feedback_no_unenforced_rules.md` 例外条件 = **2 つの hook で機械強制可能**。案 A (予防層 = session 起動時に状況認識) + 案 B (最終 backstop = stale 状態での編集を hard block + 既実装 grep 提示) のセット二段構え。
>
> **週次レビュー (ADR-031) 観点 ⑤ Todo 妥当性 との責務分離 (2026-05-26 ユーザー合意)**: **本 hook = 編集時 immediate guard / 週次 = 全 entry 横断 batch 棚卸し**。本 hook land 後の週次レビュー Phase B+1 (順位 154 `review-todo-whole` facet) は hook が拾えない broad な観点 (経年劣化 entry / cross-file 重複 / preamble drift) に focus する設計。順位 8 entry の「7 観点責務 mapping」表参照。
>
> **参照**: 本セッション (2026-05-18) PR cleanup-stale-rank-39 root cause 分析 (ユーザー対話)、PR #150 post-merge-feedback Tier 3 #1 (旧 順位 122 由来)、memory `feedback_verify_task_not_already_done.md`、ADR-039 (Experimental feature 標準パターン)、PR #172 (順位 144 hook 化 dogfood 事例)
>
> **実行優先度**: 🚀 **Tier 1** — Effort Medium-Large (案 A ~80 行 + 案 B ~50 行 = 既実装 grep 拡張で +~20 行)。本セッションの実観測 failure mode に対する直接対策で、並列セッション運用が常態化している現状で再発確率が高い。

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

**案 B: PreToolUse hook で stale 時の `docs/todo*.md` edit を block + 既実装 grep 提示 (旧 順位 122 統合)**

- 配置: 既存 `src/hooks-pre-tool-validate/` に統合 (~50 行追加 = 30 行 stale 検知 + ~20 行既実装 grep 拡張)
- 動作 1 (stale 検知): Edit / Write の対象が `docs/todo*.md` 系列のとき、master と @- の lineage 確認 → master が ahead なら hard block
- 動作 2 (既実装 grep 提示、旧 順位 122 機能統合): stale でない場合も `docs/todo*.md` への Edit/Write 時に対象 entry の keyword (= 直近の `### ` 見出し title から抽出) を `jj log --limit 20` で grep し、既実装らしき commit があれば warning として additional context に表示
- block / warning message:
  ```text
  [docs/todo edit context]
  @: lmzvnwlu (parent: #159, master: #161 = 2 ahead)
  stale parent detected → block
  関連既実装の可能性: <jj log --limit 20 "<keyword>" の上位 3 件>
  修正手順: `jj git fetch && jj new master -m "WIP: <description>"`
  ```
- scope 限定: `docs/todo*.md` のみ block / grep 対象 (コード / config までは過剰、false positive リスク)
- 案 A と異なり、本 hook は fail-closed (lineage 判定不能なら block) で安全側に倒す
- 既実装 grep の keyword 抽出ロジック: `### ` で始まる見出しから「順位 N」prefix を除いた title を取得、句読点 / 括弧を除外して 2-3 語の noun phrase を抽出 (NLP 不要、簡易 regex で実装可能)

#### 作業計画

- [ ] 既存 SessionStart hook の有無確認 (`src/hooks-session-start/` または settings.json の `SessionStart` entry)
- [ ] `jj git fetch` の timeout / kill-switch / network 例外処理設計
- [ ] `master..@-` の lineage 計算ロジック実装 (`jj log -r "master..@-" --no-graph -T 'description'` 等)
- [ ] additional context 出力フォーマット決定 (一行 vs 複数行、AI 読み飛ばし耐性検証)
- [ ] `hooks-pre-tool-validate.exe` に `docs/todo*.md` edit block ロジック追加
- [ ] **既実装 grep ロジック実装 (旧 順位 122 統合)**: Edit/Write の old_string or new_string から `### ` 見出し title を抽出 → keyword 抽出 (順位 prefix / 句読点除去) → `jj log --limit 20` 実行 → 上位 3 件を additional context に追記
- [ ] `~/.claude/rules/common/development-workflow.md` Step 0 (Research & Reuse) の手動 grep step 追加は **不要** (hook が自動実行するため rule 化スキップ、`feedback_pipeline_over_rules.md` 適用)
- [ ] memory rule `feedback_verify_task_not_already_done.md` の closure 検討 (hook 化で機能吸収後、memory entry を削除して責任を hook に集約)
- [ ] ADR 起案 (新 hook 設計 + ADR-039 experimental pattern 適用、land 時採番確定)
- [ ] dogfood 期間設定 (試験運用 flag で N 週間運用後採否決定)
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) deploy 検討
- [ ] 本エントリ削除 + todo-summary.md 行削除 (順位 122 行は本 entry 統合時に削除済 2026-05-25)

#### 完了基準

- session 開始時に working copy が master より遅れている場合、AI が context 出力で即座に状況を認識する
- stale parent 状態で `docs/todo*.md` を編集しようとすると hard block + 修正手順 (`jj git fetch && jj new master -m "WIP: <description>"`) 表示
- **`docs/todo*.md` への Edit/Write 時に既実装 grep が自動実行され、関連 commit が warning として提示される (旧 順位 122 機能、hook 化で session 跨ぎ品質一定化)**
- ADR-039 experimental pattern に従い kill-switch 装備 (network 異常 / feature branch 運用への退避経路)
- 派生プロジェクトでの動作確認

#### 詰まっている箇所

- `jj git fetch` の timeout が低速 network で頻発した場合の UX → 案 A は fail-open で warning なし pass-through、案 B は fail-closed (lineage 不能 = stale 扱い) で安全側に倒す trade-off
- master 判定ロジック: 現状 trunk-based 前提で master を正と扱う。feature branch 運用が始まると assumption が破綻するが、本リポジトリは当面 trunk-based のため問題なし。trunk 名 (master / main) は config 可能にしておく

---

### ADR-NNN (採番未確定、land 時に確定): ADR Numbering Strategy — Placeholder Policy for Multi-PR Race-Free Assignment (PR #169 T3-#2 採用)

> **動機**: 順位 135 で codify された「ADR 番号は entry 登録時に hardcode せず `ADR-NNN (採番未確定)` placeholder で記述し、land 時 PR で空き番号を確定する」運用が、PR #111 / PR #132 / PR #169 の **3+ PR で適用実証済**になった。特に PR #169 では同一 entry (順位 78) が `ADR-038 → 041 → NNN` の **3 段振り直し** を経た live dogfood が完了し、queue 滞留 entry と後発 PR の採番衝突を convention 層で完全予防できる状態が確立された。現在 policy は `~/.claude/rules/common/docs-governance.md` の 2-3 行追記として ephemeral todo (順位 135) 内で codify されているが、ephemeral artifact 限りでは派生プロジェクト (techbook-ledger / auto-review-fix-vc 等) への transferability に欠ける。正式 ADR に昇格して永続化する。
>
> **本タスクの位置づけ**: PR #169 post-merge-feedback Tier 3 #2 採用。`feedback_no_unenforced_rules.md` の例外 = 既存実践 (3 PR で実証済) の明文化 + multi-PR race-freedom rationale + history の codify。Severity Low / **Frequency Medium (PR #111/#132/#169 の 3+ PR で適用実証)** / Effort S / Adoption Risk None。
>
> **参照**: `.claude/feedback-reports/169.md` Tier 3 #2、順位 135 entry (`docs/todo8.md` 内、本 ADR 昇格後に retire 候補)、`~/.claude/rules/common/docs-governance.md` (現状 codify 先)、PR #111 / PR #132 / PR #169 history
>
> **実行優先度**: 💎 **Tier 3** — Effort S。新規 ADR 1 件作成 (記述のみ、コード変更なし) + CLAUDE.md ADR list 追記 + 順位 135 entry retire (= todo8.md から削除)。

#### ADR 番号

順位 135 codified policy 自身に従い、本 entry では番号を `ADR-NNN (採番未確定)` placeholder とする (= dogfood 自己適用)。**land 時 PR で空き番号を確定**する (現時点既存: ADR-041 まで確定、ADR-NNN slot は順位 78 で「Rust timestamp arithmetic safety」用に予約中)。本 entry が順位 78 より先に land する場合は次の空き番号を本件に割り当て、順位 78 の placeholder は維持。

#### 設計決定 (案)

- **ADR タイトル候補**: `ADR-NNN: ADR Numbering Strategy — Placeholder Policy for Multi-PR Race-Free Assignment` (内容を反映、派生プロジェクトでも理解可能な英文タイトル)
- **内容構成**:
  - **コンテキスト**: queue 滞留 entry の ADR 番号 hardcode が後発 PR の採番と衝突する構造リスク。PR #111/#132/#169 の history (順位 78 が `ADR-038 → 041 → NNN` の 3 段振り直しを経た live dogfood)
  - **決定**: ① entry 登録時は `ADR-NNN (採番未確定、land 時に確定)` placeholder で記述、② land 時 PR で `docs/adr/` の空き番号を確定、③ 同一 PR で当該 entry / markdown / table 内 placeholder を実番号に同時置換、④ 採番予約簿 (`RESERVED.md` 等) は導入しない (queue 滞留 entry の管理コストが回収可能性に見合わない)
  - **帰結**: queue 滞留期間と queue 深度の積に比例する番号衝突リスクが convention 層で予防される。派生プロジェクトでも同 policy を採用すれば multi-PR race-freedom が確保される。コスト: entry 著者は placeholder を維持する規律が必要、land 時 PR では multi-point sync (todo + ADR + CLAUDE.md) を同 commit で揃える必要
  - **適用範囲**: 全 ADR (試験運用 / 永続採用問わず)。既存 ADR (ADR-001〜ADR-041) には遡及適用しない
  - **既存資料との関係**: `~/.claude/rules/common/docs-governance.md` の 2-3 行追記 (順位 135 で codified 予定) を ADR で補完する layer。global rule は entry author への 1-line guidance、ADR は派生プロジェクトを含む reference layer
- **CLAUDE.md ADR list 追加**: project-local の Architecture Decisions list に link 追記
- **順位 135 entry retire**: 本 ADR で内容を完全 codify した時点で順位 135 を todo8.md から削除 (ephemeral → permanent への migration、`feedback_todo_no_history` 適用)

#### 作業計画

- [ ] `docs/adr/adr-NNN-adr-numbering-strategy.md` を新規作成 (番号は land 時 PR で確定)
- [ ] 内容構成 (上記 5 項目) を記述
- [ ] CLAUDE.md (project) Architecture Decisions リストに該当 ADR を追加 (番号確定時)
- [ ] 順位 135 entry を todo8.md から削除 (本 ADR が retire 先になる)
- [ ] PR description で `docs/adr/adr-NNN-adr-numbering-strategy.md` への link と「順位 135 内容を permanent ADR に migrate、派生プロジェクト transferability 確保」要約を明記 (PR 作成時)

#### 完了基準

- ADR ファイルが新規作成され、PR #111/#132/#169 の history + placeholder policy + multi-PR race-freedom rationale が記述される
- CLAUDE.md の ADR リストに該当 entry が追加される
- 順位 135 entry が todo8.md から削除される
- 次回 ADR 採番が必要な entry を書く際の reference として global rule (docs-governance.md) から本 ADR にリンク可能になる

#### 詰まっている箇所

なし。記述のみで実装変更不要。順位 135 と内容重複しないよう「global rule = 1-line entry author guidance / ADR = full rationale + history + transferability」で役割分離を明示する。

---


### ADR-041 補強 — "State Preservation Invariant" pattern section 追加 (PR #170 T3-#1 採用)

> **動機**: PR #170 post-merge-feedback analyzer が **PR #168/169/170 で write-once 不変式 (once-set-never-overwritten) のテストカバレッジ漏れが連続観測** されたことを Frequency Medium で識別。ADR-041 (Test Isolation Patterns for Multi-Condition Guards) の既存 section は early-return guard (sentinel pattern + 直交 precondition) のみで、`state.fix_push_time.or_else(...)` のような **write-once 不変式は別 pattern class** として未収録。順位 141 で takt-fix が自動追加した 3 件の preservation test (poll.rs `finalize_*_preserves_existing_fix_push_time` / monitor.rs `resume_returns_fix_push_time_from_state_when_set`) が、ADR-041 の延長として補強される自然な pattern であることが post-merge analyzer により独立識別された。
>
> **本タスクの位置づけ**: PR #170 post-merge-feedback Tier 3 #1 採用。`feedback_no_unenforced_rules.md` の例外 = 既存実践 (3 PR で実証) + project-specific 参照実装の明文化 + 派生プロジェクト transferability 確保。Severity Low / **Frequency Medium (PR #168/169/170 の 3 PR 横断)** / Effort S / Adoption Risk None。
>
> **参照**: `.claude/feedback-reports/170.md` Tier 3 #1、`docs/adr/adr-041-test-isolation-patterns.md` (本セッション順位 139 で land 済、本 task で補強)、`src/cli-pr-monitor/src/stages/poll.rs` (preservation test 2 件)、`src/cli-pr-monitor/src/stages/monitor.rs` (preservation test 1 件)、PR #168/169/170 history
>
> **実行優先度**: 💎 **Tier 3** — Effort S。既存 ADR への追記のみ (新規 ADR / コード変更なし)。

#### 設計決定 (案)

analyzer report の `[ADR-041 追加 section 案]` をベースに、`docs/adr/adr-041-test-isolation-patterns.md` の「## 適用範囲」セクションの前に新 section `## 補足: State Preservation Invariant パターン (once-set-never-overwritten)` を挿入する。内容構成:

- **パターン定義**: `state.fix_push_time = state.fix_push_time.or_else(|| ctx.fix_push_time.map(String::from));` 形式の write-once 不変式コード例
- **3 点セット test**:
  1. `state.fix_push_time = Some("old_time")` — 既存値あり (preservation される側)
  2. `ctx.fix_push_time = Some("new_time")` — 新値を提供 (上書きを試みる側)
  3. `assert_eq!(state.fix_push_time, Some("old_time"))` — old value が retain されたことを確認
- **Anti-pattern**: 全テスト fixture を `fix_push_time: None` で統一すると "don't overwrite" branch (preservation path) が実行されず coverage = 0
- **適用タイミング**: 新 field を追加し、その field が `or_else` / `if existing.is_none() { ... }` 等の write-once 意味論を持つ場合、**field 追加と同一 PR で** 上記 3 点セット test を追加する
- **参照実装**: PR #170 で land された 3 件 (`finalize_initial_review_park_preserves_existing_fix_push_time` / `finalize_review_recheck_park_preserves_existing_fix_push_time` / `resume_returns_fix_push_time_from_state_when_set`)
- **由来**: PR #170 simplicity-review F-2 + post-merge analyzer session で観測

#### 作業計画

- [ ] `docs/adr/adr-041-test-isolation-patterns.md` に新 section `## 補足: State Preservation Invariant パターン (once-set-never-overwritten)` を挿入 (上記 6 項目)
- [ ] `## 適用範囲` セクション内の対象記述に「write-once 不変式を持つ pure function 系 state 更新」を追記 (既存 = 2+ 条件の OR/AND 早期 return を持つ pure function 系 test、追加 = write-once 不変式パターン)
- [ ] `## 改訂履歴` に「2026-05-23: PR #170 T3-#1 採用、State Preservation Invariant section 追加」を追記
- [ ] 本 todo8.md entry を削除 (本 ADR 補強で内容が ADR に migrate されるため、`feedback_todo_no_history` 適用)

#### 完了基準

- ADR-041 に State Preservation Invariant section が追加され、3 点セット test pattern + 参照実装 + Anti-pattern + 適用タイミングが記述される
- 次回 write-once 不変式 field を追加する PR で、本 ADR section を直接 cite して 3 点セット test を実装できる
- 順位 142 entry が todo8.md から削除される

#### 詰まっている箇所

なし。記述のみで実装変更不要。順位 141 と異なり ADR 本体への追記のみで完結する。

---

### 複言語 fixture helper 標準化 (hooks-post-tool-linter-tests) (PR #171 T2-#4 採用) ★ Bundle 171

> **動機**: PR #151 (`byte_offset_to_line` char-boundary panic 発見) + PR #171 (`build_violation_json` defensive test 追加) の 2 PR 横断で multi-byte content fixture を手動で組み立てるコストが顕在化。Japanese / emoji / combining chars の各 sample を helper として標準化することで、新規 string-processing 関数追加時の boundary test 実装コストを低減し silent regression を early detection できる。
>
> **本タスクの位置づけ**: PR #171 post-merge-feedback Tier 2 #4 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。Bundle 171 のコア (順位 142 ADR-041 補強 + 順位 144 jj hook と同 PR で land 推奨)。
>
> **参照**: `.claude/feedback-reports/171.md` Tier 2 #4、`src/hooks-post-tool-linter/src/main.rs` (`run_custom_rules_line_number_correct_with_multibyte_content` を helper 化対象)、PR #151 / PR #171
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。Bundle 171 ペアタスク。

#### 設計決定 (案)

- **helper API** (3 関数):
  - `multibyte_fixture_japanese() -> &'static str` — 3 bytes/char (例: `// 日本語コメント`)
  - `multibyte_fixture_emoji() -> &'static str` — 4 bytes/char (例: `// 🦀 rust`)
  - `multibyte_fixture_combining() -> &'static str` — e + U+0301 結合文字 (例: `// caf\u{00e9}`)
- **配置先候補**: `src/hooks-post-tool-linter/src/main.rs` の test mod 内 (in-crate) vs 共有 test util crate (cross-crate 再利用)。本タスクでは前者を採用し、再利用ニーズが顕在化したタイミングで後者へ migrate
- **既存 test refactor**: PR #171 で追加した `run_custom_rules_line_number_correct_with_multibyte_content` を helper を呼ぶ形に書き換え

#### 作業計画

- [ ] helper 配置先決定 (in-crate test mod を優先採用)
- [ ] 3 helper 関数を実装 (Japanese / emoji / combining)
- [ ] PR #171 で追加した既存 test を helper を使う形に refactor
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への transferability 考慮 (in-crate なら porting 容易)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3 helper 関数が公開され、test mod 内から呼べる
- 既存 test の refactor 完了 (動作不変、`cargo test` pass)
- 新規 string-processing 関数追加時に 1 行で multi-byte boundary test を書ける状態になる

#### 詰まっている箇所

なし。Effort S、Bundle 171 内で 順位 142 + 順位 144 と並列実施可能。

---

### preset matrix test 追加 — default fallback vs config-selectable の 2 軸 classification 検証 (PR #172 T2-#1 採用)

> **動機**: PR #172 で `jj-message-required` preset 実装の Phase 3 において、当初 `is_blocked("jj new")` (default config 使用) で block を assert する test を書いたが、`jj-message-required` が `default_preset_names()` の fallback list に含まれない opt-in preset であることを前提とせず、test rewrite が必要になった。preset architecture の implicit assumption (always-enabled vs config-selectable) を test 設計レベルで codify することで、将来の新 preset 追加時の design misalignment を構造的に防止する。
>
> **本タスクの位置づけ**: PR #172 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort M / Adoption Risk None)。matrix test で preset 分類を明示する mechanical enforcement 層を追加。
>
> **参照**: `.claude/feedback-reports/172.md` Tier 2 #1、`src/hooks-pre-tool-validate/src/main.rs` の `default_preset_names()` + test module、PR #172 Phase 3 (test rewrite 経緯)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。Bundle 171 残タスク (順位 142 + 143) との並列実施可能。

#### 設計決定 (案)

- **配置先**: `src/hooks-pre-tool-validate/src/main.rs` の test module (feedback report は lib.rs と記載するが本 crate は binary crate のため main.rs を採用)
- **matrix 構成** (2 軸):
  - axis 1: `default fallback (always-enabled)` vs `config-selectable (opt-in)`
  - axis 2: 各 preset 名
- **classification 期待値** (本セッション時点):
  - always-enabled (`default_preset_names()` 内): `default` / `git` / `jj-immutable` / `jj-main-guard` / `jj-push-guard` / `electron`
  - config-selectable: `gh-pr-create-guard` / `gh-pr-merge-guard` / `polling-anti-pattern` / `exe-help-block` / `jj-message-required`
- **test 案**:
  - `preset_default_fallback_classification`: 各 always-enabled preset 名が `default_preset_names()` の return に含まれることを assert
  - `preset_config_selectable_opt_in_classification`: 各 config-selectable preset 名が `default_preset_names()` に含まれないことを assert
  - `preset_matrix_full_coverage`: 既知 preset 名の全集合が classification 表 (always-enabled ∪ config-selectable) と一致することを assert (= 新 preset 追加時に matrix 更新を強制)

#### 作業計画

- [ ] preset 分類表を const として定義 (`ALWAYS_ENABLED_PRESETS` + `CONFIG_SELECTABLE_PRESETS`)
- [ ] matrix test 関数 3 件追加 (default fallback / config-selectable / full coverage)
- [ ] 既存 test (`default_config_enables_all_presets` / `jj_message_required_not_in_default_fallback_is_opt_in` 等) との重複整理 (削除 or matrix への移行)
- [ ] `resolve_preset_or_custom` の dispatch arm 列挙との整合性確認 (matrix の preset 名 = dispatch arm 名)
- [ ] 派生プロジェクト transferability 考慮 (porting 時に preset 分類を即把握できる)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- preset の分類 (always-enabled vs config-selectable) が test レベルで codify される
- 将来の新 preset 追加時に classification 表を更新せざるを得ない構造になり、design misalignment が構造的に検出される
- 既存 test (158 件) との regression なし
- `resolve_preset_or_custom` の arm 列挙との不整合 (preset 追加忘れ等) が test で catch される

#### 詰まっている箇所

- feedback report は target を `src/hooks-pre-tool-validate/src/lib.rs` と記載するが、本 crate は binary crate (main.rs のみ) で lib.rs は存在しない → main.rs を採用 (target 是正)
- 「config-selectable preset 名が default に含まれない」test は `jj_message_required_not_in_default_fallback_is_opt_in` で 1 件既存。matrix 化で全 5 preset に拡張する

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
