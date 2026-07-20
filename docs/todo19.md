# TODO (Part 19)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo10.md がファイルサイズ約 95KB (50KB 安定読み取り閾値の約 1.9 倍) に達したため、順位 220〜224 のエントリを本ファイルに分離した (2026-07-20 docs 50KB 超過解消の物理分割)。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### subprocess stress test (>64KB stdout) を ADR-031 weekly-review pipeline 経由で週次実行 (PR #217 post-merge-feedback T2-1 採用)

> **動機**: PR #217 (refactor PR-3a) の post-pr-review iter 2 で 2 module (`hooks-session-start/src/jj_helpers.rs` + `hooks-pre-tool-validate/src/todo_staleness.rs`) に同型の subprocess deadlock 脆弱性が independent 観測された。具体的な脆弱性は、`Command::new("jj")` を `.stdout(Stdio::piped())` で spawn したあと parent process が `try_wait` ループで wait しつつ child の stdout を drain せず終了後にまとめて read するため、jj log の出力が pipe buffer (Linux default 64KB / Windows 4-64KB) を超えると child が write block → 親が wait block → deadlock。
>
> CR Major fix として `spawn_stdout_drainer` + `poll_child_with_deadline` 関数を抽出して background drain に変更 (takt-fix iter 2)、さらに iter 3 で `lib_subprocess::drain_pipe_unlimited` + `wait_with_timeout_basic` の既存共通 helper への統合に refactor。本 fix で deadlock は構造的に防止されたが、**実際に >64KB を pipe させる regression test が存在しない** ため future refactor で再発する盲点が残る。
>
> **本タスクの位置づけ**: PR #217 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Medium / Effort M / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「2 モジュールで deadlock パターン確認、High severity + Medium frequency で M effort を正当化、deadlock は大 buffer 時のみ顕在化するため手動検証が困難でテスト化が最も確実な防止手段」。
>
> **ユーザー判断 (2026-06-23)**: 「毎回走るタイプ (hooks など) のテストに組み込むのは適切ではない、週に 1 回程度に頻度を落として通常の開発速度に影響が出ない形で CI に組み込みたい」。Stop hook quality gate (`cargo test`) や pre-push pipeline (`cargo test`) は毎 push 実行のため stress test のような高コスト・低頻度検証は不適切。ADR-031 weekly-review pipeline (週次 cron / 手動 `/weekly-review`) で `cargo test -- --ignored --test-threads=1` 系の追加 step として実行する方針。
>
> **参照**: `.claude/feedback-reports/217.md` Tier 2 #1、PR #217 takt-fix iter 2 / iter 3 (`lib-subprocess` 統合)、ADR-031 § Phase B (takt workflow + facets)、`#[ignore]` test 慣習 (例: cli-pr-monitor の integration test、ADR-021)、`docs/adr/adr-044-subprocess-utility-extraction-boundary.md` (lib-subprocess の extraction 境界判定)、順位 221 (ADR docs codification、bundle 推奨)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。stress fixture 作成 (~50 行 × 2 module) + ADR-031 weekly workflow への step 追加 + `cargo test -- --ignored` 経由の動作確認。

#### 設計決定 (案)

- **test 配置**: 各 module の既存 `mod tests` に `#[ignore = "stress test, requires explicit --ignored flag (PR #217 T2-1)"]` 付きで追加
- **fixture 方針**: 実 `jj log` を呼び出すと環境依存になるため、`Command::new("yes")` 系 (Linux) や `Command::new("cmd").args(["/c", "for /L %i in (1,1,N) do @echo ..."])` (Windows) で >64KB の決定論的出力を生成。あるいは `jj log` を repo 内の真の commit history で呼ぶ場合は test 前提として大型 repo を必要とせず、std::process::Command 単体で test 可能な形に
- **検証項目**:
  - `> 64KB` の stdout を吐く child を spawn し、`drain_pipe_unlimited` 経由で完全 read できる (`output.len() > 64 * 1024`)
  - timeout 内に child が exit する (`child.wait().is_ok()` 系 assert)
  - parent が wait 完了する (deadlock していたら test 自体がハングして CI timeout で fail)
- **CI 統合**: ADR-031 weekly-review workflow に新 step `rust-stress` を追加 (`cargo test --workspace -- --ignored --test-threads=1`)。既存 rust-test group (`cargo test --workspace`) とは分離 (前者は毎回、後者は週次)
- **派生プロジェクト transferability**: `lib-subprocess` を採用する他 crate (cli-merge-pipeline / cli-pr-monitor / cli-push-runner / hooks-post-tool-linter) へも同型 stress test を transfer 可能。本 task の MVP は 2 module 限定、3+ module で同 pattern 観測時に拡張判断 (cli-push-pipeline は 2026-07-17 に crate 削除済みのため対象外)
- **memory `feedback_test_dry_antipattern.md`** 適用: 各 module の test 内に独立 helper (`spawn_large_output_child` / `assert_no_deadlock_within`) を duplicate、共有 test module は抽出しない

#### 作業計画

- [ ] hooks-session-start/src/jj_helpers.rs の `mod tests` に `stress_drain_large_stdout_does_not_deadlock` 追加 (~50 行、`#[ignore]` 付き)
- [ ] hooks-pre-tool-validate/src/todo_staleness.rs の `mod tests` に同型 test 追加 (~50 行)
- [ ] `cargo test -p hooks-session-start -- --ignored --test-threads=1` でローカル動作確認
- [ ] `cargo test -p hooks-pre-tool-validate -- --ignored --test-threads=1` でローカル動作確認
- [ ] ADR-031 weekly-review workflow (`.takt/workflows/weekly-review.yaml` 等) に rust-stress step 追加
- [ ] 次回 `/weekly-review` で実発火確認、本 task entry 削除 + todo-summary2.md 行削除

#### 完了基準

- 各 module で `>64KB stdout` を pipe する stress test が `#[ignore]` 付きで存在
- `cargo test -- --ignored --test-threads=1` で test が pass、deadlock していないこと (timeout しないこと) を確認
- ADR-031 weekly-review workflow に新 step が追加され、次回 weekly 実行で stress test が走る
- 順位 221 (ADR docs) と合わせ、test 層 (本 task) + docs 層 (221) の 2 層防御が確立

#### 詰まっている箇所

- Windows での大出力 fixture コマンド: `yes` は Windows に存在しない。`cmd /c for /L` で代替可能だが PowerShell / bash 等の環境差を test 内で吸収する設計が必要 (cross-platform test fixture)
- ADR-031 workflow への step 追加: 既存 weekly-review yaml の構造を確認、rust-stress step の独立 facet 化が必要か、aggregate-weekly facet の pre-step として組み込むかは実装時判断

---

### ADR-NNN (採番未確定、land 時に確定): Safe Subprocess Stdout Pattern を ADR-016 appendix or 新 ADR で codify (PR #217 post-merge-feedback T3-1 採用)

> **動機**: PR #217 takt-fix iter 2 で 2 module 同型の subprocess deadlock を fix した実例 (順位 220 参照) から、`Stdio::piped()` を伴う child process の安全な扱い方を ADR で永続化する必要が判明した。同 pattern は本 PR 以前にも `lib-subprocess` 内部で `drain_pipe_unlimited` + `wait_with_timeout_basic` として codify されていたが、**新規 subprocess spawn を書く著者が pipe buffer 制約を知らない場合の防御層が欠落** していた。
>
> ADR で pattern を明文化することで:
>
> - 機械検知 (T1-1 lint rule、🤔 様子見) より低 risk な代替防止層として機能
> - 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への transferability を確保 (ADR は global 参照可能)
> - reviewer (人間 / AI) が PR review 時に「Stdio::piped() を見たらこの ADR を確認」する mental check が成立
> - ADR-025 (CwdRestore Drop guard pattern) の precedent と整合: 「pattern の codify は test/lint に先行する低コスト防止層」
>
> **本タスクの位置づけ**: PR #217 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「2 モジュールで同一パターン違反、Medium frequency + S effort + None risk、pattern を明文化することで T1-1 の lint rule 化より低 risk な代替防止層として機能、ADR-025 の precedent あり」。
>
> **参照**: `.claude/feedback-reports/217.md` Tier 3 #1、PR #217 takt-fix iter 2 (`spawn_stdout_drainer` + `poll_child_with_deadline` 初版抽出) / iter 3 (`lib-subprocess` 統合)、`docs/adr/adr-016-long-running-command-strategy.md` (append 候補)、`docs/adr/adr-025-cwd-restore-drop-guard.md` (precedent: pattern codify ADR)、`docs/adr/adr-044-subprocess-utility-extraction-boundary.md` (lib-subprocess 境界判定)、`src/lib-subprocess/src/lib.rs` (`drain_pipe_unlimited` / `wait_with_timeout_basic` 実装)、順位 220 (test 層、bundle 推奨)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。ADR appendix or 新 ADR 作成 (~150 行)、3 pattern (background drain / `Command::output()` / `Stdio::null()`) の説明 + lib-subprocess utility cite + anti-pattern 例 (本 PR の deadlock fix 経緯を inline cite)。

#### 設計決定 (案)

- **配置選択**: 2 案あり、ADR 起案時にユーザー判断:
  - **Option A** (append): `docs/adr/adr-016-long-running-command-strategy.md` に新 section「Safe Subprocess Stdout Pattern」を追加。ADR-016 が既に subprocess 戦略を扱うため整合的、ADR 数を増やさない
  - **Option B** (new): `docs/adr/adr-NNN-safe-subprocess-stdout-pattern.md` として新規 ADR (採番は land 時に確定、順位 135 placeholder policy per)。pattern が ADR-016 の長時間コマンド扱いとは別関心 (= pipe buffer 制約は短時間 subprocess でも発生) のため scope 分離する根拠あり
- **本 task の MVP 推奨**: Option A (append) — ADR 数増加を抑え、ADR-016 § 長時間コマンド戦略 直後の new section として組み込む。実装時の dogfood で B 化判断
- **記述項目** (3 pattern + anti-pattern):
  1. **Background drain pattern**: `spawn(...)` + `std::thread::spawn(move \|\| out.read_to_end(...))` で stdout を別 thread で drain、parent は `try_wait` ループ。本 PR で `lib_subprocess::drain_pipe_unlimited` + `wait_with_timeout_basic` として codify 済
  2. **`Command::output()` pattern**: 短時間 subprocess で stdout/stderr を一括 capture する標準慣習。pipe buffer 問題を回避するが timeout 制御不可
  3. **`Stdio::null()` pattern**: stdout を完全に捨てる場合 (= 副作用のみ目的)。pipe buffer 問題なし、最も simple
  4. **Anti-pattern**: `Stdio::piped()` + drain なしで `try_wait` ループ。pipe buffer 枯渇で deadlock (本 PR の修正前 state、inline cite)
- **由来 cite**: PR #217 takt-fix iter 2 の deadlock 修正経緯と iter 3 の lib-subprocess 統合 refactor を inline 引用
- **派生プロジェクト波及**: ADR は global 参照可能、本 ADR を `~/.claude/rules/common/` に link することで techbook-ledger / auto-review-fix-vc 等に reference 提供
- **enforcement layer**: 機械 lint (T1-1) は false positive リスクで 様子見、本 ADR が author 教育 + reviewer 確認の文書層、順位 220 (stress test) が test 層、3 層構成 (docs / test / lint defer) で防御

#### 作業計画

- [ ] ADR 配置の Option A/B 判断 (Option A = ADR-016 append が MVP 推奨)
- [ ] ADR section / 新 ADR を作成 (~150 行、3 pattern + anti-pattern + cite)
- [ ] CLAUDE.md ADR list 追記 (Option B の場合のみ)
- [ ] ADR-025 precedent との相補性を ADR 内で明示
- [ ] `~/.claude/rules/common/coding-style.md` (or rust/patterns.md) から本 ADR への link を追加 (派生プロジェクト transferability)
- [ ] 本 task entry 削除 + todo-summary2.md 行削除

#### 完了基準

- ADR (appendix or new) が land、3 pattern + anti-pattern + 由来 cite を含む
- ADR-016 / ADR-025 / ADR-044 / lib-subprocess との関係性が明確
- `~/.claude/rules/common/` から本 ADR への参照が追加され、派生プロジェクトで future PR 著者が `Stdio::piped()` を書く際の reference として機能
- 順位 220 (stress test) と相補的に、docs + test の 2 層防御確立

#### 詰まっている箇所

- Option A vs B の判断: ADR-016 § 長時間コマンド戦略 が「長時間 = timeout 制御」に focus している場合、本 pattern (pipe buffer = 短時間でも発生) との scope mismatch で別 ADR が綺麗。実装着手時に既存 ADR-016 を read して判断
- `~/.claude/rules/common/` への link 追加先: `coding-style.md` か `rust/patterns.md` かは Option A/B の結論と整合させる必要あり

---

### `~/.claude/CLAUDE.md` に「複数セッション跨ぎの計画文書作成時は AI が先走らずユーザー確認後に方針報告し GO/NO-GO を得る」ルール追加 (PR #218 post-merge-feedback #5 採用)

> **動機**: PR #218 (docs PR、ファイルサイズチェックフロー改善計画 + 順位 220/221 採用) のセッション内で、Plan file (`docs/file-length-enforcement-plan.md`) 作成完了報告後、AI (Claude) が **ユーザー承認なしに PR-W0 (weekly audit step 追加) の実装着手を開始** し、ユーザーが `[Request interrupted by user]` で停止 + 「勝手に作業を進めないでください」と明示的に course correction する事案が発生した。Auto mode 下でも「計画書 / planning doc 作成のような **大きな task 完了時** は GO/NO-GO の確認待ちが必須」という規範を CLAUDE.md に明文化することで、本セッション内の事例を後続セッションで再発防止する。
>
> **本タスクの位置づけ**: PR #218 post-merge-feedback #5 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None、2026-06-23 ユーザー承認)。analyzer rationale: 「AI がユーザー確認なしに計画書作成を開始し `[Request interrupted by user]` で停止させた事例。Severity Medium (AI 暴走 = UX 劣化)・Effort XS・Adoption Risk None → ✅ 条件を満たす。Frequency Low だが Effort が極小なため採用コストが低い」。
>
> **参照**: `.claude/feedback-reports/218.md` Tier 3 #5、PR #218 session transcript (Plan file 作成完了 → AI 先走り → ユーザー停止 → "勝手に作業を進めないでください" の course correction)、memory `feedback_no_unauthorized_reorder.md` (推奨実行順序の上位タスクが blocked された時点で停止し、ユーザーに pivot 可否を確認する、の補強)、memory `feedback_global_config_backup.md` (snapshot 必須)、`~/.claude/CLAUDE.md` (編集対象 global config)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global config への 1 段落追記で完結、`feedback_global_config_backup` snapshot を忘れない。

#### 設計決定 (案)

- **追加先**: `~/.claude/CLAUDE.md` の `## Personal Preferences` section 直後 (もしくは `## Doing tasks` 配下の sub-section)
- **rule 内容**:

  ```markdown
  ### AI 先走り防止 — 計画文書作成完了時の GO/NO-GO ゲート

  複数セッション跨ぎの計画文書 (planning doc / 設計ドキュメント) の作成完了時は、
  Auto mode の最中であっても **次の実装着手を一時停止し、ユーザーに方針報告 +
  GO/NO-GO 確認を待つ**。

  対象となる "大きな task" の例:

  - 新規 planning doc (`docs/<topic>-plan.md` / `docs/<topic>-analysis.md` 等) の作成完了
  - ADR 起案
  - 複数 PR にまたがる作業計画の決定
  - 既存 planning doc への大規模追記 (Tier 1/2 構成変更等)

  対象外 (= 通常 task として継続して問題ない):

  - 単一 PR scope 内の段階的 commit
  - 既存計画通りの逐次実装 step

  GO/NO-GO 確認のフォーマット例:

  > Plan file 作成完了。次の step は PR-W0 (...) への着手です。進めて OK か?

  Auto mode の「prefer action over planning」原則の例外として、planning doc
  レベルの完了点では明示承認待ちが必須。
  ```

- **由来 cite**: PR #218 session transcript で実観測した「Plan file 完了報告 → AI が PR-W0 着手 → ユーザー停止 + 'AI 先走り' 指摘」の流れを inline cite
- **memory `feedback_no_unauthorized_reorder` との関係**: 既存 memory は「task が blocked された時点で停止」を扱うが、本 rule は「task 完了時 (= 自然な区切り) で停止」を扱う = lifecycle の異なる stage を扱う相補的 rule
- **派生プロジェクト波及**: `~/.claude/CLAUDE.md` 編集のため全 project に自動波及、planning doc の頻度が高い大型 refactor PR で効果を発揮
- **Auto mode との関係**: Auto mode 仕様の「prefer action over planning」と本 rule の「planning 完了時は停止」は scope 分離 (前者は通常作業の AI 自律性、後者は planning doc レベルの mile stone 確認) で衝突しない

#### 作業計画

- [ ] `~/.claude/` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/CLAUDE.md` に新 sub-section「AI 先走り防止 — 計画文書作成完了時の GO/NO-GO ゲート」を追加 (上記設計決定の rule 内容、~30 行)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + docs/todo-summary2.md 行削除

#### 完了基準

- `~/.claude/CLAUDE.md` に新 sub-section が追加される (対象 task 例 / 対象外 / フォーマット例 含む)
- PR #218 事例が inline cite として記録される
- 全プロジェクト (techbook-ledger / auto-review-fix-vc 含む) に global rule として自動波及
- Auto mode 下でも planning doc 完了時点で AI が明示承認待ちに転じることが、次回以降の planning task で確認可能

#### 詰まっている箇所

- 対象範囲の境界定義: 「計画文書」「設計ドキュメント」「大きな task」の判定基準が author に依存する余地あり。MVP は上記「対象 task の例」「対象外」リストで運用、3+ 回の dogfood で境界明確化を判断 (順位 207 mechanical lint scope 外 boundary case 追加の pattern と同様)
- Auto mode 仕様との関係明示: `~/.claude/CLAUDE.md` の Auto mode セクションが追加 or 改訂されている場合、本 rule の例外条項 (「prefer action over planning」との関係) を Auto mode セクション側にも cross-reference するか判断

---

### `ACTIVE_RUN_FRESH_THRESHOLD_SECS` と `ORPHAN_THRESHOLD_SECS` の compile-time 同期 (PR #222 post-merge-feedback T1-1 採用)

> **動機**: PR #222 で hooks-stop-quality に追加した `ACTIVE_RUN_FRESH_THRESHOLD_SECS = 1500` は、hooks-session-start reaper の `ORPHAN_THRESHOLD_SECS = 1500` と **同値である必要がある** (Stop hook が「active」と判定する window と、reaper が「orphan」と判定する threshold が非対称になると、その隙間に挟まった run が両方の防御層から漏れる)。現状は `hooks-stop-quality/src/main.rs:67` のコメント (「reaper の `ORPHAN_THRESHOLD_SECS` (= 1500s) と同値」) で同期を「人間が読んで覚える」契約に留まり、片方を変更したときに他方を追従する mechanical enforcement が欠落している。
>
> 同型の precedent として `cli-merge-pipeline/src/feedback.rs:60` の `pub const ORPHAN_THRESHOLD_SECS: u64 = TAKT_TIMEOUT_SECS + 300;` が存在し、derived value として上流定数 (`TAKT_TIMEOUT_SECS`) との関係を compile-time で保証している。本 task は同じ pattern を hooks-stop-quality / hooks-session-start の magic number 同期にも適用する。
>
> **本タスクの位置づけ**: PR #222 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Medium / Effort M / Adoption Risk None、2026-06-27 ユーザー承認)。analyzer rationale: 「3 reports 全てで threshold alignment を言及。`cli-merge-pipeline` の precedent (const + assert 方式) が参照実装として存在、実装コスト低。drift 発生時に orphan detection window と quality gate skip window が非対称になるリスクが明確。Effort M かつ Frequency Medium で採用候補」。`feedback_tier_classification` per analyzer Tier 1 (= mechanical enforcement) → project Tier 2 (🔧) に再分類。
>
> **参照**: `.claude/feedback-reports/222.md` Tier 1 #1、`src/hooks-stop-quality/src/main.rs:67` (現状のコメント契約 + 定数定義)、`src/hooks-session-start/src/reaper.rs:29` (source of truth = `pub(crate) const ORPHAN_THRESHOLD_SECS: u64 = 1500`)、`src/cli-merge-pipeline/src/feedback.rs:60` (precedent: derived const + 上流定数 reference)、ADR-043 (Security/Quality Gate Fail-Closed) — fail-closed threshold の同期が崩れた場合のリスクを ADR で論じる経路、ADR-030 (決定論的 Post-Merge Feedback) § L2 reaper — `ORPHAN_THRESHOLD_SECS` の出処、順位 224 (ADR-043 amendment、bundle 推奨)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。lib 抽出 (shared crate / lib module) vs cross-crate const re-export + compile-time assert の選択 + test 追加。

#### 設計決定 (案、land 時に判断)

3 案を比較し、本 task 着手時に Option 選択:

- **Option A** (shared lib crate 新設): `src/lib-takt-constants/` 等の新 crate を作成し `ORPHAN_THRESHOLD_SECS` / `ACTIVE_RUN_FRESH_THRESHOLD_SECS` を pub const として公開。reaper と stop-hook 双方が dep として import。**Pros**: 単一の source of truth、将来 takt 関連の magic number 追加に scale。**Cons**: 新 crate のため Cargo workspace への追加 + build graph 拡張、ADR-026 (Cargo workspace) との整合確認、過剰一般化のリスク (Effort M の上限)
- **Option B** (cross-crate const re-export): hooks-session-start の `reaper::ORPHAN_THRESHOLD_SECS` を `pub` に昇格、hooks-stop-quality が `[dependencies] hooks-session-start = { path = "..." }` で依存し `use hooks_session_start::reaper::ORPHAN_THRESHOLD_SECS;` で参照。**Pros**: 新 crate 不要、変更最小。**Cons**: hooks crate 間の依存方向が逆 (本来は SessionStart hook と Stop hook は独立だが、これで一方向 dependency が発生)、cargo workspace の循環依存リスク (現状は問題なくとも将来制約に)
- **Option C** (compile-time assert via `const _: () = assert!(...)`): 各 hook crate で独立に const を定義しつつ、片方の crate 内で `const _: () = assert!(REAPER_ORPHAN_THRESHOLD_SECS == ACTIVE_RUN_FRESH_THRESHOLD_SECS);` 系の compile-time 検証を入れる。**Pros**: dep 依存 0、各 hook crate の autonomy 維持。**Cons**: 検証側 crate に「他 crate の値を埋め込む」必要があり、結局参照経路が必要 (= Option B と同等の依存になる)
- **MVP 推奨**: **Option B と C の hybrid** で、stop-hook 内に `const _: () = assert!(...)` を追加して const 同期を保証 (C 側のメリット) しつつ、`hooks-session-start::reaper::ORPHAN_THRESHOLD_SECS` を pub 昇格して hooks-stop-quality が dep として参照 (B 側のメリット)。Option A (新 crate) は将来同型 magic number が 3+ 出てきた時に再評価
- **`cli-merge-pipeline/src/feedback.rs:60` の precedent を inline cite**: 上流定数 (`TAKT_TIMEOUT_SECS`) + derived const + 同 crate 内 const equality assert の構成例。本 task は cross-crate 版に拡張
- **test 追加**: compile-time assert は失敗時 compile error なので runtime test は不要、ただし「片方の const を変更したら compile error になる」ことを確認する手順を README / コメントに記録 (PR description で dogfood)
- **派生プロジェクト transferability**: 本 pattern (cross-crate const sync) は派生プロジェクト (techbook-ledger / auto-review-fix-vc) でも同型ニーズが発生しうるが、現状は本 repo 固有のため transferability は ADR-016 / ADR-043 等への documentation 経由

#### 作業計画

- [ ] Option A / B / C 判断 (MVP 推奨 = Option B+C hybrid: assert で同期保証 + pub 昇格して dep 参照)
- [ ] `hooks-session-start::reaper::ORPHAN_THRESHOLD_SECS` を `pub` 昇格 (現状 `pub(crate)`)、必要なら module path の整理
- [ ] hooks-stop-quality の `Cargo.toml` に `hooks-session-start` dep を追加 (Option B 側のアプローチ)
- [ ] hooks-stop-quality `main.rs` に `const _: () = assert!(ACTIVE_RUN_FRESH_THRESHOLD_SECS == hooks_session_start::reaper::ORPHAN_THRESHOLD_SECS);` を追加
- [ ] `cargo build --workspace` で compile-time assert が通ることを確認、片方を変更して compile error を観測 (PR description に貼付)
- [ ] hooks-stop-quality の既存コメント (line 67) を「compile-time assert で同期保証」 に更新
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- 片方の const を変更すると `cargo build` が compile error で失敗する
- hooks-stop-quality / hooks-session-start の magic number 同期が compile-time 契約として codified
- `cli-merge-pipeline/src/feedback.rs:60` precedent と同 pattern が確立、将来同型 magic number 追加時の参照実装になる
- ADR-030 §L2 と本 task の関係性が ADR 内で明示される (順位 224 と相補 = T1 mechanical + T3 docs)

#### 詰まっている箇所

- Option A/B/C 判断: 新 crate 追加 vs 既存 dep 追加 vs assert macro のいずれも trade-off あり、ADR-026 (Cargo workspace) との整合性を実装時に確認
- hooks crate 間の依存方向: SessionStart と Stop は本来独立な lifecycle stage だが、const 共有のため一方向 dep が必要 (Option B/C)。将来 hooks 共通 lib (= 順位 224 関連で hook_stop_quality の `meta_is_fresh` 抽出が浮上した場合) が出てきた際に再整理判断

---

### ADR-043 (Security/Quality Gate Fail-Closed) に hooks-stop-quality の error handling を具体例として追記 (PR #222 post-merge-feedback T3-1 採用)

> **動機**: PR #222 で hooks-stop-quality に追加した `meta_is_fresh()` / `meta_is_active_run()` / `takt_subsession_active()` は、すべての error path で `false` を返却することで「gate が effective (= skip しない)」状態に倒す **fail-closed pattern** を踏襲している。具体的には mtime 取得失敗 / system clock skew (future timestamp) / malformed JSON / file read error すべてが「active subsession ではないと判定」→「quality gate を skip しない」= 安全側に倒れる。
>
> ADR-043 (Security/Quality Gate での Fail-Closed 原則) は **試験運用 ADR** として既に存在するが、現状は abstract な原則記述に留まる。PR #222 の `meta_is_fresh` 実装は ADR-043 の **concrete instantiation** として価値があり、ADR 内の具体例 list に追記することで:
>
> - ADR が「概念定義 + 具体例 list」型に進化、将来同型 fail-closed pattern を実装する author の参照実装になる
> - 派生プロジェクト (techbook-ledger / auto-review-fix-vc) で hook / lint / pipeline を fail-closed で書く際の transferability 向上 (ADR は global 参照可能)
> - 順位 223 (mechanical enforcement) と相補的に、docs 層で fail-closed pattern を author 教育として codify
>
> **本タスクの位置づけ**: PR #222 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-27 ユーザー承認)。analyzer rationale: 「ADR-043 は永続 artifact で既存。追記は XS Effort で ADR の具体例を充実。fail-closed pattern は本 PR 以外でも発生する (Frequency Medium)。Effort XS + None risk で採用候補」。
>
> **参照**: `.claude/feedback-reports/222.md` Tier 3 #1、`docs/adr/adr-043-security-gates-fail-closed.md` (追記対象、試験運用 ADR)、`src/hooks-stop-quality/src/main.rs` の `meta_is_fresh()` / `meta_is_active_run()` (具体例として cite)、PR #222 (`b0b91978`) (由来 cite)、順位 223 (T1 mechanical、bundle 推奨)、`feedback_global_config_backup` 適用は本リポジトリ docs/ のため不要 (グローバル CLAUDE.md / rules は触らない)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。ADR-043 の既存 § 「concrete examples」 or 末尾に新 sub-section を追加 (~30 行)、mtime 取得失敗 / clock skew / malformed JSON / read error の 4 error path を列挙し、各 path で `false` 返却 → gate effective 維持を inline 説明。

#### 設計決定 (案)

- **追記位置**: ADR-043 に既存「concrete examples」section があればそこへ追加。無ければ末尾 (References 直前) に新 sub-section 「Concrete instantiation: hooks-stop-quality の subsession freshness check (PR #222)」を追加
- **記述項目**:
  - 4 error path 各々で何が起き `false` 返却に倒れるか
    - **mtime 取得失敗** (`std::fs::metadata()` error / `metadata.modified()` error): `Err(_) => return false`
    - **system clock skew (future timestamp)**: `mtime.elapsed()` が `Err` を返した場合 `Err(_) => false` で skip 不可、orphan 扱いになる
    - **malformed JSON**: `serde_json::from_str::<TaktMetaPartial>` が `Err` → `Ok(_)` 以外は `false`
    - **file read error**: 同上、`fs::read_to_string` が `Err` → `false`
  - 全ての error path が「subsession active ではない」= 「Stop hook の quality gate を skip しない」に倒れる構造 = fail-closed
  - 反対 pattern (fail-open = error path で `true` 返却) の anti-example: orphan 1 件が残るだけで全 session の quality gate が永続 skip → ADR-004 §「freshness check の必要性」で詳細解説
- **由来 cite**: PR #222 で CR Major 指摘 (orphan 永続 skip リスク) に対応した freshness check 実装、ADR-004 amendment と本 ADR-043 追記の 2 ADR を同時更新した経緯を inline 引用
- **派生プロジェクト transferability**: ADR は global 参照可能、本 ADR を `~/.claude/rules/common/` から link することで techbook-ledger / auto-review-fix-vc 等に reference 提供
- **順位 223 との bundle**: 1 PR でまとめて land 推奨 (T1 mechanical layer + T3 docs layer の 2 層防御を 1 PR で確立)

#### 作業計画

- [ ] ADR-043 現状を read、追記位置 (既存 concrete examples or 新 sub-section) を判断
- [ ] 「Concrete instantiation: hooks-stop-quality の subsession freshness check (PR #222)」sub-section を追加 (~30 行)
- [ ] 4 error path の inline 説明 + anti-example (fail-open) の論理対比
- [ ] PR #222 + ADR-004 § freshness check との cross-reference 追加
- [ ] markdownlint clean
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- ADR-043 に hooks-stop-quality の error handling が concrete example として追記される
- 4 error path 各々の `false` 返却 logic と「gate effective」状態への倒し方が明示される
- PR #222 / ADR-004 / ADR-043 の 3 文書間の cross-reference が成立、reader が 1 example から原則 → 適用 → 反対例を辿れる
- 順位 223 と合わせ、mechanical (T2) + docs (T3) の 2 層防御確立

#### 詰まっている箇所

- ADR-043 が現時点で試験運用 (= ephemeral 性質を残す) のため、concrete example 追記が本採用昇格のトリガーになりうるかは現状 ADR 内容を read してから判断
- ADR-043 と ADR-004 の責務境界: 「fail-closed 原則の汎用論」(043) vs 「freshness check の必要性」(004) を ambiguous にせず、043 が「why fail-closed」、004 が「how freshness check works」と明確に分業する記述

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo9.md 末尾を参照。)
