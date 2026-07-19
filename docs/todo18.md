# TODO (Part 18)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo10.md がファイルサイズ約 95KB (50KB 安定読み取り閾値の約 1.9 倍) に達したため、順位 215〜219 のエントリを本ファイルに分離した (2026-07-20 docs 50KB 超過解消の物理分割)。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### `~/.claude/rules/common/coding-style.md` に「Defensive State Reset in State Machines」section 追加 (PR #214 post-merge-feedback T3-1 採用)

> **動機**: PR #214 round 2 で CR Major #4 (`既存 state 再利用時も現在の push 情報に更新してください`) の fix として `finalize_initial_review_park` 内で `read_state()` 後に `state.pr` / `state.repo` / `state.started_at` を `ctx` 値で **無条件上書き** する pattern を land した。この pattern は同 function 内の既存 reset と同型 (CR Major #1 fix で `head_commit` 上書き、CR Major #2 fix で `review_recheck_count = 0`) で、現時点で 3 field に適用済の確立された defensive pattern。
>
> ただしこの「無条件上書き」は新規 reader / reviewer から見ると一見「冗長 (= `unwrap_or_else(|| ::new(...))` で既に同値を設定済だから不要)」に見える危険性がある。同型コードを future PR で reviewer (人間 / AI 両方) が「redundant resets は削除すべき」と誤判定して削除した場合、prior cycle の stale state (古い PR 番号 / repo / 開始時刻) が混入する silent bug を導入するリスクが顕在化する。
>
> **本タスクの位置づけ**: PR #214 post-merge-feedback Tier 3 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-20 ユーザー承認)。analyzer rationale: 「PR #214 の `review_recheck.rs` で positive pattern として land (lines 185–187)。同型コード (`review_recheck_count`, `head_commit` 上書き) との一貫性がある確立されたパターン。Frequency Medium = cli-pr-monitor には複数の state machine があり再発確実。Effort S、Adoption Risk None → ✅ 採用候補と判定」。pre-push:simplicity + pre-push:security の独立 2 ソース検出。
>
> **参照**: `.claude/feedback-reports/214.md` Tier 3 #1、[src/cli-pr-monitor/src/stages/poll/review_recheck.rs:177-193](../src/cli-pr-monitor/src/stages/poll/review_recheck.rs#L177-L193) (現行 `finalize_initial_review_park` の defensive reset block)、memory `feedback_no_unenforced_rules.md` (enforcement 要件)、memory `feedback_global_config_backup.md` (snapshot 必須)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。global rules への docs 追記 ~30 行で完結、`feedback_global_config_backup` snapshot を忘れない。

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

### `no-workstream-seq-names-in-config` lint rule 追加 — config comment 内 `PR-[0-9]+` ephemeral workstream sequence 検出 (PR #216 post-merge-feedback T1-1 採用)

> **動機**: PR #216 で `.claude/hooks-config.toml` の `weekly_review_reminder` section comment に `(2026-06-23、PR-1)` および `次 PR (PR-3) で移行予定` を書き込んだ。これは ephemeral workstream sequence names (= マルチ PR 計画のローカル連番、GitHub PR `#NNN` ではない) を permanent artifact (config file comments) に embed する違反であり、`coding-style.md` § Cross-File Reference Lifecycle の「permanent → ephemeral 禁止」原則と同根。
>
> 既存の rule⑥ `no-ephemeral-todo-reference` は `docs/todo*.md` file path 直接参照を検出するが、本ケースのような workstream sequence names (`PR-N`) は対象外。PR シリーズ完了後に「PR-3 とは何だったか」が文脈喪失し dead pointer 化するリスクが構造的に残る。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-23 ユーザー承認)。順位 217 (文書層) と同根 = 1 PR bundle 推奨。analyzer rationale: 「config file comment は permanent artifact であり dead pointer 化の直接トリガ。pattern `(?i)PR-[0-9]+` の FP リスクは軽微 (config コメントで企業コード等との混同は稀)」。Prepush T1-1 + Session T1-1 + Session T1-2 の 3 ソース独立検出。
>
> **Tier 列との不整合補足**: analyzer feedback report (`.claude/feedback-reports/216.md`) では `Tier 1: Hooks/Linter 改善` カテゴリに分類されているが、本 todo entry の Tier 列および「実行優先度」行では **🔧 Tier 2** に再分類している。memory `feedback_tier_classification` の re-classification rule (= analyzer の Tier 1/3 分類は鵜呑みにせず実体ベース ⟨mechanical enforcement = T1 / docs 修正 = T3⟩ で再分類) に従い、project tier 定義 (🚀 Tier 1 = high-impact urgent / 🔧 Tier 2 = tooling improvements) と整合させた意図的再分類。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 1 #1、PR #216 commit `65963197e6c0` の hooks-config.toml diff、既存 rule⑥ `no-ephemeral-todo-reference` (template)、rule⑫ `no-hardcoded-jj-revset-range` (TOML meta field test_coverage pattern の template)、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle、`.claude/custom-lint-rules.toml` (rule 配置先)。
>
> **実行優先度**: 🔧 **Tier 2** (project 分類、上記 re-classification 後) — Effort S。rule 追加 (~30 行 TOML + meta field) + test 追加 (~50 行 main.rs) で約 80 行、順位 217 と bundle すれば 1 PR diff < 200 行見込み。

#### 設計決定 (案)

- **rule id**: `no-workstream-seq-names-in-config`
- **pattern**: `(?i)\bPR-[0-9]+\b`
- **extensions**: `["toml", "yaml", "yml", "jsonc"]` (config formats、plain `json` は comment 構文を持たず rule 対象外なので除外)
- **検出範囲**: comment 行のみ (TOML `#`、YAML `#`、JSONC `//` 等)。**実装注**: 多くの project-local lint rule は file 全体に regex match している。本 rule は comment 行検出が本質だが、初期実装は file 全体マッチで MVP として開始し、false positive 観測後に comment 行限定への絞り込みを判断する (= 同 pattern の rule⑥/⑫ と整合的な段階導入)
- **exception**: GitHub PR number `#[0-9]+` 形式は対象外。実装上は positive pattern `(?i)\bPR-[0-9]+\b` が `#NNN` を match しない (`#` prefix 形式は別) ため exception 不要。ただし test で「`#216`」「`PR #216`」のようなケースが fire しないことを negative test で固定
- **severity**: `warning` (block しない、author への hint 機能優先、rule⑫ と同 pattern)
- **block message**: 「Ephemeral workstream sequence name (`PR-N`) detected in config comment. Permanent artifacts (config files) must not reference ephemeral workstream sequences. Use GitHub PR `#NNN` for stable cite, or inline rationale instead of "PR-3 で移行予定". See coding-style.md § Cross-File Reference Lifecycle.」
- **TOML meta field** (`test_coverage` schema、rule⑫ と同 pattern):
  ```toml
  [rules.test_coverage]
  other_ext_tests = ["no_workstream_seq_detects_pr_dash_n_in_jsonc_comment"]

  [rules.test_coverage.main_ext_tests]
  toml = ["no_workstream_seq_detects_pr_dash_n_in_toml_comment", "no_workstream_seq_skips_github_pr_number"]
  yaml = ["no_workstream_seq_detects_pr_dash_n_in_yaml_comment"]
  yml = ["no_workstream_seq_detects_pr_dash_n_in_yml_comment"]
  ```

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に `[[rules]]` entry 追加 (id / pattern / extensions / severity / message / test_coverage)
- [ ] `src/hooks-post-tool-linter/src/main.rs` の `mod tests` に positive test 4 件 (toml / yaml / yml / jsonc 各 1) + negative test 1 件 (`#216` / `PR #216` が fire しない) を追加
- [ ] `cargo test -p hooks-post-tool-linter` で rule_test_coverage_check が pass することを確認
- [ ] dogfood: 本 PR で `hooks-config.toml` から `PR-1` / `PR-3` 表記が削除 or `#216`/`#NNN` 表記に置換されることを確認
- [ ] 本エントリ削除 + docs/todo-summary.md 行削除

#### 完了基準

- `no-workstream-seq-names-in-config` rule が `.toml` / `.yaml` / `.yml` / `.jsonc` / `.json` comment 内の `PR-[0-9]+` を warning として検出
- `#216` / `PR #216` のような GitHub PR number は fire しない (negative test pass)
- rule_test_coverage_check が main_ext_tests / other_ext_tests 整合性を強制
- 順位 217 (文書層) と同 PR で land した場合、`coding-style.md` への具体例追加と機械強制の 2 層防御が確立される

#### 詰まっている箇所

- comment 行限定 vs file 全体 match: MVP は file 全体 match で開始、false positive 観測後に絞り込み判断 (rule⑥/⑫ と同段階導入)。順位 217 の docs 追加で「config comment」の意図を明示化することで、author が non-comment context での意図的使用を回避できれば file 全体 match でも実用性高い
- `#NNN` vs `PR-NNN` 境界: regex `\bPR-[0-9]+\b` は `#` prefix を含まないため除外可能、ただし将来 `PR-#216` のような mixed 表記が登場した場合は pattern 拡張が必要

---

### `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に config file comments の permanent artifact 扱い明記 + workstream sequence 禁止例追加 (PR #216 post-merge-feedback T3-1 採用)

> **動機**: 既存 `coding-style.md` § Cross-File Reference Lifecycle は markdown 内 cross-reference (docs/ADR/README 等) を主に想定して書かれており、**config file comments (`.toml`/`.json`/`.yaml`) も permanent artifact** であることが暗黙的にしか扱われていない。PR #216 で `hooks-config.toml` comment に "PR-1" / "PR-3" ephemeral workstream sequence を embed した違反は、author が「config の comment は注釈であって rule の対象外」と暗黙的に判断していた可能性が高い。
>
> 順位 216 の lint rule が機械的に防止するが、author の理解を促す **文書層** として補完することで「なぜ config comment にも reference lifecycle が適用されるか」を理解可能にする。機械層 (216) + 文書層 (本 task) の 2 層防御は順位 200/202/205 と同 pattern。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。順位 216 (機械層) と 1 PR bundle 推奨。analyzer rationale: 「既存ルールは markdown document 内の cross-reference を主に想定しており config file comments の permanent artifact としての扱いが暗黙的。Tier 1-1 の custom lint rule が機械的に防止するが、author の理解を促す文書層として補完。Frequency Medium (cross-file reference violations の systemic pattern と同根)」。Session T3-1 + PR-analysis T3-1 の独立 2 ソース収束。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #1、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (現行 section、編集対象)、順位 216 (機械層 = lint rule、本 task の機械強制対応)、memory `feedback_global_config_backup` (snapshot 必須)、PR #216 hooks-config.toml diff (違反実例として inline cite)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rules への docs 追記 ~15 行で完結、`feedback_global_config_backup` snapshot を忘れない。

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

### ADR-039 § Bounded Lifetime + `~/.claude/rules/common/patterns.md` に provisional `enabled` 変更時の todo entry 必須化を追加 (PR #216 post-merge-feedback T3-2 採用)

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
> **実行優先度**: 💎 **Tier 3** — Effort XS。ADR + global rules への docs 追記 ~10 行で完結、`feedback_global_config_backup` snapshot を忘れない。

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

### `~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック に「commit description 言及 ≠ 実装完了」明文化 (PR #216 post-merge-feedback T3-3 採用)

> **動機**: PR #216 cleanup 作業中、analyzer (Claude) が「PR #215 commit description で 順位 215 を言及している = 実装完了」と naïve に判断し、当初 6 entries (147/151/212/213/214/215) 削除計画を立てた。実際にはユーザーの修正 + grep `"Defensive State Reset" ~/.claude/rules/common/coding-style.md` による実体確認の結果、順位 215 は **todo entry が PR #215 で追加されただけ** で実装は未着手だった (5 entries 削除が正解)。
>
> この naïve assumption は今後も analyzer / Claude が再発する可能性が高く、誤った削除を実施すると未実装タスクが docs から消える silent loss につながる。development-workflow.md に明文化することで、future Claude session 内で同 anti-pattern を構造的に防止する。
>
> **本タスクの位置づけ**: PR #216 post-merge-feedback Tier 3 #3 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「本 PR で『commit description に順位 N 言及 = 実装完了』の naïve assumption から analyzer が誤った 6 entry 削除計画を立てた実観測。ユーザー修正で 5 entry に訂正。Effort XS、Severity Medium (analyzer の誤判定リスクが今後も継続)、Adoption Risk None」。Session T3-3 の単一ソースだが Severity Medium で採用条件成立。
>
> **参照**: `.claude/feedback-reports/216.md` Tier 3 #3、`~/.claude/rules/common/development-workflow.md` § 設計 doc/実装の同期チェック (編集対象)、PR #216 cleanup session log (誤判定 → grep 救出の経緯)、memory `feedback_verify_task_not_already_done` (関連 memory、再確認 verb-noun rule の前提となる「verify」step)、memory `feedback_global_config_backup` (snapshot 必須)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rules への docs 追記 ~8 行で完結、`feedback_global_config_backup` snapshot を忘れない。

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

