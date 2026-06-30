# TODO (Part 13)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo10.md がファイルサイズ約 95KB (50KB 安定読み取り閾値の約 2 倍) に到達したため、新規エントリは本ファイルに記録する (PR #224 セッション、2026-06-29 ユーザー判断)。**新規エントリの追加先は本ファイル**。todo.md / todo2.md 〜 todo12.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### auto-push gate-bypass の是正 — A1 (fix facet で `--ignored` 必須) + B1-loop (auto-push に gate + convergence ループ差し戻し) (PR #224 セッション合意)

> **動機**: PR #224 (PR-W2) で、CodeRabbit Major finding を takt が auto-fix した際、`create_fix_commit` の変更が `#[ignore]` な repush 統合テスト2件を破壊した。しかし (a) takt fix facet の coder は `cargo test` (非 `--ignored`) で検証して `convergence_verdict: fully_resolved` を宣言、(b) 監視の auto-push は `jj git push` 直 push で cli-push-runner の quality_gate (`cargo test -- --ignored` を含む唯一のゲート) を**バイパス**したため、回帰が無検証で PR に到達した。手動で `cargo test --ignored` を回して初めて発見・revert した。
>
> **本タスクの位置づけ**: PR #224 セッション ユーザー合意 (2026-06-29)。A1 (fix 時の検証強化) + B1-loop (push 時の安全網) の 2 層。
>
> **参照**: PR #224 (`1c0f345b`)、`push-runner-config.toml` `[[quality_gate.groups]]` name=`rust-lint-test` (`cargo test -- --ignored --test-threads=1` を含む、コメント「push pipeline でのみ実行」= 当ゲートが #[ignore] テストを回す唯一の自動経路)、`.takt/facets/instructions/fix.md` (coder 完了ゲート、修正対象)、`src/cli-pr-monitor/src/stages/repush.rs` (auto-push の `jj git push`、修正対象)、ADR-043 (Fail-Closed)、ADR-037 (convergence_verdict 信頼)、ADR-022 (auto-push 責務)。
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。回帰素通しの実害が PR #224 で顕在化。A1 は低コスト (facet に数行)、B1-loop は auto-push 経路の改修。

#### 設計決定 (案)

- **A1**: `fix.md` の完了ゲートに「test ファイルを変更した、または `pub(crate)` 関数の挙動・signature を変えた場合は `cargo test -- --ignored --test-threads=1` を実行し PASS を `fully_resolved` 宣言の前提とする」を必須条件として追記。
- **B1-loop**: 監視の auto-push を `jj git push` 直ではなく、push 前に quality_gate 相当 (clippy + `cargo test` + `cargo test -- --ignored`) を実行。FAIL なら (i) takt convergence ループに差し戻して再 analyze→再 fix (= `convergence_verdict` に `--ignored` 結果を反映)、(ii) N 回で収束しなければ fail-closed で `action_required` に倒し人間へ escalation。
- **人間ボトルネック回避**: B1-loop により自己修復可能な回帰 (sibling テスト忘れ等) は機械で完結、本当に詰まった時のみ人間。今日の「壊れたまま push → 後で人間が後始末」より前倒し + 良いシグナル。

#### 作業計画

- [ ] A1: `fix.md` 完了ゲートに `--ignored` 必須条件を追記
- [ ] B1: auto-push (`repush.rs`) に push 前 quality_gate (`--ignored` 含む) を挿入
- [ ] B1-loop: gate FAIL を convergence ループに差し戻す経路 + N 回上限 → `action_required` (fail-closed)
- [ ] dogfood: 意図的に #[ignore] テストを壊す fix を作り、(a) A1 で fix 時捕捉、(b) B1 で push 前捕捉、(c) loop で自動修復を確認
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- takt fix が #[ignore] テストを壊す変更をしても fix 時 (A1) または auto-push 前 (B1) に必ず検出され、自己修復可能なら機械収束・不能なら `action_required` で人間へ。`jj git push` 直 push で gate を迂回する経路が解消。

#### 詰まっている箇所

- B1-loop の「convergence ループ差し戻し」を takt 既存の analyze→fix 反復にどう接続するか (`convergence_verdict` に `--ignored` 結果を含める形が素直)、N 回上限の妥当値。

---

### fmt baseline cleanup + `cargo fmt --check` gate 導入 + rustfmt 固定 (PR #224 セッション合意)

> **動機**: PR #224 で分割 agent の `cargo fmt` が分割対象外の 5 ファイルに整形差分を混入した (revert で対処)。調査の結果、fmt enforcement がリポジトリのどこにも無く (Stop gate / push pipeline / CI / package.json いずれも `cargo fmt --check` 不在)、ワークスペース全体で **29 ファイル**が rustfmt-clean でないドリフトを蓄積していると判明。
>
> **本タスクの位置づけ**: PR #224 セッション ユーザー合意 (2026-06-29)。file_length plan と同じ「clean baseline → gate」構造。
>
> **参照**: PR #224 (`1c0f345b`)、`.claude/hooks-config.toml` `[stop_quality]` (clippy はあるが fmt 無し)、`push-runner-config.toml` `[quality_gate]` (fmt 無し)、`package.json` scripts (lint/build/test は TS 向け)、ADR-017 (バージョン固定哲学 = rustfmt 固定の根拠)。
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。29 ファイルの一括正規化 (機械) + gate 1 step + toolchain 固定。

#### 設計決定 (案)

- **(A)** 一度きりの `cargo fmt --all` クリーンアップ commit で 29 ファイルを正規化 (mechanical、behavior 不変 = Phase 1 clean state)。
- **(B)** `cargo fmt --all -- --check` を Stop gate (hooks-config.toml の clippy step と同型) and/or push-runner-config.toml の quality_gate に 1 step 追加。
- **(C)** `rust-toolchain.toml` で rustfmt バージョン固定 (未固定だとマシン/セッション間で出力が揺れ gate が flaky 化)。
- **順序必須**: (A) を飛ばして (B) を入れると 29 ファイルで即 fail。

#### 作業計画

- [ ] (A) `cargo fmt --all` 実行 → 29 ファイル正規化 commit (mechanical)
- [ ] (C) `rust-toolchain.toml` で channel + rustfmt component 固定
- [ ] (B) fmt `--check` step を Stop gate / push-runner gate に追加
- [ ] dogfood: 意図的に非整形コードを書き gate が block することを確認
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- `cargo fmt --all -- --check` が gate で実行され、非整形コードが push/Stop 前に検出される。29 ファイルのドリフトが解消され clean baseline 確立。rustfmt 固定で gate が決定論的。

#### 詰まっている箇所

- (A) のクリーンアップ commit を W3/W4 (cli-merge-pipeline / cli-push-runner の分割) と衝突させないタイミング (分割 PR と fmt 一括が同ファイルに当たると rebase 競合)。

---

### rule⑬: 非テストコードでの理由なし `#[allow(...)]` 禁止 custom lint (PR #224 セッション合意)

> **動機**: PR #224 で分割 agent が dead な再エクスポートを残すため `#[allow(unused_imports)]` を付与していた (= clippy が検知した未使用 import を抑制、削除で対処)。`#[allow]` は本質的に「lint の握り潰し」で、既存の swallowed-error 系 custom rule (rule③ 空 catch / rule④ SilentlyContinue / rule⑩ `let _ = write_*`) と同じ philosophy で決定論的に防げる。
>
> **本タスクの位置づけ**: PR #224 セッション ユーザー合意 (2026-06-29)。判断2 の仕組み化。
>
> **参照**: PR #224 (`1c0f345b`)、`.claude/custom-lint-rules.toml` (追加先、rule③/④/⑩ と同型)、`src/hooks-post-tool-linter/src/main.rs` (`CustomRule` struct + test)、ADR-007 (正規表現層)、Bundle Z #B-α philosophy。
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。custom-lint-rules.toml に 1 rule + main.rs に positive/negative test。

#### 設計決定 (案)

- **pattern**: justification マーカー (`// ALLOW-JUSTIFIED:` 等) が直前/同行に無い `#[allow(...)]` を検出。Rust regex は lookbehind 非対応のため、マーカー判定は 2 行 multiline pattern or enumeration で実装。
- **severity**: warning (一律 block は friction 大、reviewer 判断補助)。
- **scope**: extensions=["rs"]。test code (`#[cfg(test)]` 配下) は `#[allow]` が正当なケースが多いため除外したいが、regex で module スコープ判定は困難 → 着手時に paths filter (test ファイル除外) vs 近傍判定 vs 許容のいずれかを決定。
- **必須**: rule 追加時の `test_coverage` (positive: 理由なし allow 検出 / negative: justified allow skip) を main.rs に追加 (`rule_test_coverage_check` 機械強制)。

#### 作業計画

- [ ] custom-lint-rules.toml に rule⑬ 追加 (pattern + severity + why + fix + example + test_coverage)
- [ ] main.rs に positive/negative test 追加 (justification マーカー有無で discriminate)
- [ ] false positive 計測 (既存コードの `#[allow]` を grep、正当なものに justification マーカー付与 or scope 調整)
- [ ] `cargo test -p hooks-post-tool-linter` pass
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- 理由なし `#[allow(...)]` が Write 時 (PostToolUse) に warning として検出され、justification マーカー付きは skip。`rule_test_coverage_check` が positive/negative test を機械強制。

#### 詰まっている箇所

- `#[cfg(test)]` スコープ判定が regex 層で困難。false positive 計測で既存 `#[allow]` 件数を把握してから severity/scope (test 除外方式) を確定。

---

### `rate_limit_signal::cr_clean` の regression test (PR #224 post-merge-feedback T2-1 採用)

> **動機**: PR #224 で CodeRabbit Major 指摘を採用した `evaluate_rate_limit_shortcut` の `cr_clean` 判定拡張 (`unresolved_threads` のみ → `new_comments` / `actionable_comments` も検査) の回帰防止。`unresolved_threads: None` を clean と誤認する silent failure を将来の変更から保護する。
>
> **本タスクの位置づけ**: PR #224 post-merge-feedback Tier 2 #1 採用 (severity High / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/224.md` Tier 2 #1、`src/cli-pr-monitor/src/stages/poll/rate_limit_signal.rs` `evaluate_rate_limit_shortcut`、PR #224 fix commit (Fix 3)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。

#### 作業計画

- [ ] `unresolved_threads` / `actionable_comments` の None / Some(0) / Some(1)、`new_comments` (型は `usize`) の 0 / 1 を直交させ、各境界で `cr_clean` の true/false を assert する test 追加
- [ ] 既存 `evaluate_rate_limit_shortcut_blocks_when_new_comments_exist` (Fix 3 で追加) との重複排除、各 field 独立の discriminating test
- [ ] `cargo test -p cli-pr-monitor` pass
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- 3 field 各々の clean/dirty 境界が test で保護され、None ケースの silent-clean 誤認が regression として検出可能。

---

### ADR-022 拡張 — pre-create cleanup flow の具体例 + agent fmt スコープ指針 (PR #224 post-merge-feedback T3-1 採用)

> **動機**: PR #224 で CodeRabbit が `create_fix_commit` の「空 findings でも commit 作成」を bug と誤判定した (ADR-022 の意図的な pre-create 設計を知らなかったため、却下した CR#2)。また分割 agent が無差別 `cargo fmt` を実行した事象も ADR-022 の責務分離原則で説明可能。両事象とも将来再発が見込まれる。
>
> **本タスクの位置づけ**: PR #224 post-merge-feedback Tier 3 #1 採用 (doc-only / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/224.md` Tier 3 #1、`docs/adr/adr-022-automation-responsibility-separation.md` (追記対象)、`src/cli-pr-monitor/src/fix_commit/abandon.rs` (`create_fix_commit` → takt amend → `try_abandon_empty_fix_commit` のシーケンス、cite 対象 + integration test 参照)、PR #224 CR#2 却下 reply (pull/224 discussion_r3487797338)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。doc-only。

#### 作業計画

- [ ] ADR-022 に (1) Pre-Create Cleanup Flow の具体例 (`jj new` 空 child → takt amend → 変更なければ `try_abandon_empty_fix_commit` で abandon、integration test 参照) を追記
- [ ] ADR-022 に (2) 大規模リファクタリング agent 委譲時の format スコープ指針 (分割対象ファイルのみに fmt 限定、無差別 `cargo fmt` 回避) を追記
- [ ] markdownlint clean
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- ADR-022 に pre-create cleanup の具体シーケンスが明記され、CodeRabbit / 将来の reader が「空 commit 作成は意図的設計」と理解できる。agent fmt スコープ指針が責務分離原則として codify される。

---

### post-merge-feedback / workflow agent の repo 作業ツリー書込禁止 + 検知安全網 (PR #224 セッション合意)

> **動機**: PR #224 の merge 時、post-merge-feedback workflow の analyze-session agent が transcript 解析用の Python スクリプト (`parse_transcript.py`) を repo root に生成し後始末しなかった (PR-specific throwaway、本セッションで削除済)。merge は日常工程のため、その都度 scratch / 中間ファイルが残ると repo にゴミが累積し、コーディングエージェントが全ファイルを読む性質上、不要なコンテキスト消費・意図せぬ挙動の原因になる。
>
> **本タスクの位置づけ**: PR #224 セッション ユーザー合意 (2026-06-29)。多層 (発生源を断つ + 検知安全網)。
>
> **参照**: PR #224 merge (`1c0f345b` land 後)、`.takt/facets/instructions/analyze-session.md` (+ analyze-pr / aggregate-feedback、修正対象)、`.takt/workflows/post-merge-feedback.yaml` (workflow)、`.takt/post-merge-feedback-transcript.jsonl` (gitignore 済の transcript = jq で直接読めば script 不要)、`cli-merge-pipeline` post_steps (検知ステップ追加候補)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort S-M。facet instruction 追記 (S) + 検知ステップ実装 (S-M)。

#### 設計決定 (案)

- **(1) 発生源を断つ (最優先)**: feedback facets (analyze-session / analyze-pr / aggregate-feedback 等) の instruction に「**repo 作業ツリーにファイルを作らない**。中間解析は jq/grep を in-context で使うか、agent scratch ディレクトリ / `.takt/` (gitignore 済) に限定する」を明記。`.takt/post-merge-feedback-transcript.jsonl` は jq で読めるため helper script 不要。
- **(2) 検知安全網**: merge pipeline (`cli-merge-pipeline`) の post_steps または Stop hook で「workflow 実行後に root 直下 (or tracked dir 外) へ新規 untracked ファイルが出現したら warning + 一覧表示」を追加 → commit 前にゴミを surface (block はしない、ADR-039 mechanical-lint 例外パターン)。
- **(3) gitignore**: ad-hoc 名は予測不能なので gitignore 単独では不十分。(1)+(2) が本質、gitignore は補助。

#### 作業計画

- [ ] (1) analyze-session.md + 関連 feedback facets に「repo 書込禁止 + in-context/scratch 使用」を追記
- [ ] (2) post_steps / Stop hook に root 直下新規 untracked 検知 + warning を実装
- [ ] dogfood: 次回 merge で feedback workflow が repo root に stray を残さないことを確認
- [ ] (3) 必要なら gitignore に補助パターン追加
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- post-merge-feedback workflow が repo 作業ツリーに stray ファイルを残さない。万一残った場合も検知ステップが commit 前に warning で surface。

#### 詰まっている箇所

- (2) の「新規 untracked 検知」の実装場所 = merge pipeline post_step (merge 文脈限定) vs Stop hook (全 workflow 横断) のどちらが適切か。merge 以外の workflow (pre-push-review / weekly-review 等) も同型リスクがあるなら Stop hook 側が広くカバーするが、誤検知 (正当な新規ファイル) との切り分けが要る。

---

### post-pr-review (takt) の diff scope を PR 全体に修正 — `@` コミット限定による docs-only 誤判定の解消 (PR #227 観測)

> **動機**: PR #227 (cli-pr-monitor flaky 修正 2 件 + docs 整理、3 commit) の post-pr monitor で、takt `post-pr-review` の analyze が PR を **docs-only と誤判定**した。実際は spvtqwor (create_pr.rs の tempfile 化) / qzpwsyzr (state path DI) の Rust 変更を含むが、analyze が見た diff は `@` コミット (`docs/todo*.md` のみ) だった。その結果、CodeRabbit が PR 全体 (`create_pr.rs:208`) を見て出した finding を ADR-035 docs-only filter で「適用外」と**誤フィルタ**した。今回は finding 自体も false positive (composition root のため DI 不要) だったため実害はなかったが、**有効な finding を見逃すリスク**がある。
>
> **本タスクの位置づけ**: PR #227 セッション観測 (2026-06-30)、ユーザー判断で todo 登録。CodeRabbit が PR 全体 (base..head) を見るのに対し takt の判定 diff が `@` 限定で、findings と local diff scope が構造的に不整合になる点が核心。
>
> **参照**: PR #227、`.takt/review-comments.json` (findings = `create_pr.rs:208`)、push runner ログ `[diff] 実行: jj diff -r @` / `review-diff.txt (68 行)`、ADR-027 (push-time review は `@` の simplicity 限定、architectural review は post-PR CodeRabbit に委ねる)、ADR-035 (docs-only 評価ポリシー — classify の入力 diff scope を誤ると誤適用)、cli-pr-monitor の `post-pr-review` 起動箇所 (`stages/takt.rs` 周辺)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。診断 (diff scope の生成箇所特定) + 修正。実害は CodeRabbit がフル PR を見るため現状限定的だが、自動フィルタの信頼性に関わる。

#### 設計決定 (案)

- **(A)** post-pr-review の analyze に渡す diff を PR 全体 (`master..@` または PR base..head) に変更する。`@` 限定の pre-push-review (ADR-027) とは射程が異なる (post-PR は PR 全体を評価すべき) ことを明示。
- **(B)** または docs-only 分類を local diff でなく **CodeRabbit findings の file path 基準** に切り替える (findings が code file を指すなら docs-only にしない)。
- pre-push-review (ADR-027 = `@` 限定 simplicity) と diff 生成を共有しているなら、post-pr-review 専用に分離する。

#### 作業計画

- [ ] post-pr-review が docs-only 判定に使う diff の生成箇所を特定 (`review-diff.txt` 流用 or 独自生成)
- [ ] diff scope を PR 全体に修正、or 分類基準を findings file path に変更
- [ ] dogfood: code + docs 混在 PR で docs-only 誤判定しないことを確認
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- code 変更を含む PR が post-pr-review で docs-only と誤判定されず、code file を指す CodeRabbit finding が ADR-035 filter で誤って適用外にされない。

#### 詰まっている箇所

- diff scope を PR 全体にする際、pre-push-review (ADR-027 = `@` 限定 simplicity) との設定/生成共有部分に影響しないか。post-pr-review 専用に diff 生成を分離する必要があるか。

---

### memory `feedback-di-over-ambient-global-tests` に serialization primitive 例外境界 + PR #227 具体例を追記 (PR #227 post-merge-feedback T3-1 採用)

> **動機**: PR #227 (cli-pr-monitor 並列テスト flaky 修正) の post-merge-feedback で採用候補 (T3-1) として浮上。既存 memory `feedback-di-over-ambient-global-tests` の「DI over ambient global」原則が、直感的には「通常 test helper は複製推奨」原則と矛盾するように見える問題を解消する。PR #227 は本原則の 2 例目 (PR #224 の env_override_lock 関連も同根)。
>
> **本タスクの位置づけ**: PR #227 post-merge-feedback Tier 3 #1 採用候補 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。ユーザー承認で todo 登録 (2026-06-30)。
>
> **参照**: `.claude/feedback-reports/227.md` Tier 3 #1、memory `~/.claude/projects/C--Users-owner-work-ccht-improve/memory/feedback-di-over-ambient-global-tests.md`、PR #227 (state path DI)、PR #224 (env_override_lock)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。memory への数行追記。

#### 設計決定 (案)

- (a) `PR_MONITOR_STATE_FILE_OVERRIDE` race → `state_path: &Path` DI での解消を具体例として列挙。
- (b) 「serialization primitive (`static LOCK: OnceLock<Mutex<()>>`) は複製禁止、通常 test helper は複製推奨」という例外境界を明示。

#### 作業計画

- [ ] memory `feedback-di-over-ambient-global-tests.md` に (a) 具体例 + (b) 例外境界を追記
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- memory に PR #227 の DI 具体例と serialization primitive 例外境界が明記され、「DI over ambient global」と「test helper 複製推奨」の見かけの矛盾が解消される。

#### 詰まっている箇所

- 特になし (memory 編集のみ)。順位 235 (ADR-022 Appendix) と内容が相補的なので同時着手が効率的。

---

### ADR-022 に Serialization Primitive Single-Instance Rule の Appendix 追加 (PR #227 post-merge-feedback T3-2 採用)

> **動機**: PR #227 と PR #224 T2-2 (共有 env_override_lock helper 抽出) で同根の serialization primitive 単一化問題が 2 PR 観測 (Frequency Medium)。`OnceLock<Mutex<()>>` 等の serialization primitive をプロセス内で複製すると各々が独立した Mutex になり競合排除機能が破壊される。通常の helper function 複製推奨 (DRY) との例外境界が ADR-022 に未明文化。
>
> **本タスクの位置づけ**: PR #227 post-merge-feedback Tier 3 #2 採用候補 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。ユーザー承認で todo 登録 (2026-06-30)。
>
> **参照**: `.claude/feedback-reports/227.md` Tier 3 #2、`docs/adr/adr-022-automation-responsibility-separation.md`、PR #224 T2-2 (env_override_lock)、PR #227 (state path DI)、順位 234 (memory 拡張、相補)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。ADR-022 への Appendix 追加。

#### 設計決定 (案)

- ADR-022 に Appendix「Serialization Primitive Single-Instance Rule」を追加:
  - `OnceLock<Mutex<()>>` 等の serialization primitive はプロセス内で単一化必須。
  - 複製すると各々が独立した Mutex になり競合排除機能が破壊される特殊ケース。
  - 通常の helper function 複製推奨 (DRY) との例外境界を明文化。

#### 作業計画

- [ ] ADR-022 に Serialization Primitive Single-Instance Rule の Appendix 追加
- [ ] 順位 234 (memory) と cross-reference
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- ADR-022 に serialization primitive 単一化原則が明文化され、PR #224/#227 で観測された複製による競合排除破壊が構造的に予防される。

#### 詰まっている箇所

- ADR-022 (自動化コンポーネントの責務分離) の主題と serialization primitive (test isolation) がやや別軸。Appendix として追加するか、ADR-046 (feedback-reports T3-3 様子見) として独立させるかは着手時判断。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo10.md / todo9.md 末尾を参照。)
