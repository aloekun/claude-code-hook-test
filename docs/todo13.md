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

### tempfile mandate + PID+ms 命名 block の custom lint (PR #229 post-merge-feedback T1-1 採用)

> **動機**: PR #227 で temp file collision flaky (`body_with_literal_newline_converted`) を `tempfile` 一意化で修正したが、将来同型の PID+ms カスタム命名 (`gh-pr-body-{PID}-{ms}.md` 等) が再導入されると flaky が再発する。実際 PR #229 で本 flaky が push pipeline の `cargo test` を **3 回ブロックした実害**あり (push retry 3 回)。custom lint で `tempfile::NamedTempFile` / `tempfile::Builder` を mandate し、手動 PID+ms temp 命名を block して構造的に予防する。
>
> **本タスクの位置づけ**: PR #229 post-merge-feedback Tier 1 #1 採用 (High / Effort S / Adoption Risk None)。順位 230 (flaky 修正、#227 land) の予防層。順位 237 (regression test = 検出層) と二層防御。
>
> **参照**: `.claude/feedback-reports/229.md` Tier 1 #1、PR #227 (`3d8e2aac`、flaky fix)、`.claude/custom-lint-rules.toml` (追加先、rule①〜⑫ と同型)、`src/cli-pr-monitor/src/stages/create_pr.rs` (tempfile 移行の実例)、`src/hooks-post-tool-linter/src/main.rs` (`CustomRule` + test)、ADR-007 (正規表現層)。
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。custom-lint-rules.toml に 1 rule + main.rs に positive/negative test。

#### 設計決定 (案)

- **pattern**: temp file path を PID+ms で手動生成するパターン (例: `format!("...{}-{}...", std::process::id(), ...as_millis())` 系の temp 命名) を検出。`tempfile` crate (O_EXCL + ランダム名 + 衝突リトライ = industry standard、FP 極小) の使用を促す。
- **severity**: warning (reviewer 判断補助)。block 化も検討。
- **scope**: extensions=["rs"]。test code を含めるかは着手時判断。
- **必須**: `rule_test_coverage_check` 用の positive (PID+ms 命名検出) / negative (tempfile 使用は skip) test を main.rs に追加。

#### 作業計画

- [ ] PID+ms temp 命名パターンを検出する rule を custom-lint-rules.toml に追加
- [ ] main.rs に positive/negative test 追加
- [ ] 既存 `.rs` の PID+ms temp 命名を grep して false positive 計測
- [ ] `cargo test -p hooks-post-tool-linter` pass
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- PID+ms カスタム temp 命名が Write 時 (PostToolUse) に検出され `tempfile` crate 使用が促される。順位 237 (regression test) と二層防御を構成。

#### 詰まっている箇所

- PID+ms 命名パターンの regex 表現 (false positive を抑えつつ手動 temp 命名を捕捉)。`process::id()` + `as_millis()` の組合せを近傍で検出する multiline pattern が要検討。

---

### create_pr flaky の高並列 regression test (PR #229 post-merge-feedback T2-1 採用)

> **動機**: PR #227 で temp file collision flaky を `tempfile` + per-test `tempfile::tempdir()` 注入で修正したが、将来 collision-prone な命名が再導入された場合の**検出網がない**。`body_with_literal_newline_converted` 系を高並列 (`--test-threads` 高 / concurrent run) で回す regression test を追加し、collision を恒常的に trap する。順位 236 (lint = 予防) と本タスク (test = 検出) の二層防御。
>
> **本タスクの位置づけ**: PR #229 post-merge-feedback Tier 2 #1 採用 (Medium / Effort M / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/229.md` Tier 2 #1、PR #227 (`3d8e2aac`)、`src/cli-pr-monitor/src/stages/create_pr.rs` test module。
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。

#### 作業計画

- [ ] `convert_body_to_file` を per-test tempdir 注入 + 高並列 concurrent で実行し temp file collision が起きないことを assert する regression test 追加
- [ ] 意図的に PID+ms 命名へ戻すと test が落ちることを確認 (検出網の有効性検証)
- [ ] `cargo test -p cli-pr-monitor` pass
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- collision-prone な temp 命名の再導入が regression test で検出される。順位 236 (予防層) と二層防御を構成。

---

### `Command::new("gh")` 直叩き禁止 + timeout wrapper 必須の custom lint (PR #230 post-merge-feedback T1-#1 採用)

> **動機**: PR-W3 (cli-merge-pipeline 分割) で移動した `fetch_pr_time_range` / `fetch_pr_diff_summary` (pr_metadata.rs) と `run_gh_logged` / `delete_remote_branch` (github.rs) の計 4 箇所が `Command::new("gh").output()` を timeout なしで同期実行しており、ネットワーク不調や gh 側停止時に merge pipeline を無期限にハングさせる (CodeRabbit Major #2/#3、ADR-016 long-running command strategy 違反)。同 crate の pipeline.rs は既に `run_cmd_shell_capped_reporting` (timeout ラッパー) を使用しているため、直叩きを custom lint で検出して timeout 経路へ寄せる。
>
> **本タスクの位置づけ**: PR #230 post-merge-feedback Tier 1 #1 採用 (High / Frequency High / Effort M / Adoption Risk = false positive リスク、`.rs` 限定で軽減)。PR-W3 で deferred した CodeRabbit findings #2/#3 の恒久対策層。
>
> **参照**: `.claude/feedback-reports/230.md` Tier 1 #1、PR #230 (`3e7fdf9e`)、`src/cli-merge-pipeline/src/feedback/pr_metadata.rs` / `src/cli-merge-pipeline/src/github.rs` (対象)、`src/lib-subprocess/` `run_cmd_shell_capped_reporting` (推奨 wrapper)、`.claude/custom-lint-rules.toml` (追加先、rule①〜⑫ と同型)、`src/hooks-post-tool-linter/src/main.rs` (`CustomRule` + test)、ADR-016。
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。custom-lint-rules.toml に 1 rule + main.rs に positive/negative test。順位 240 と同 crate、1 PR bundle 検討可。

#### 設計決定 (案)

- **pattern**: `Command::new("gh")` の直叩き (特に `.output()` / `.spawn()` を timeout 制御なしで呼ぶ経路) を検出。`run_cmd_shell_capped_reporting` 相当の timeout wrapper 使用を促す。
- **severity**: warning (reviewer 判断補助)。block 化は着手時判断。
- **scope**: extensions=["rs"]。false positive 軽減のため直叩き pattern を絞る (test code の扱いは着手時判断)。
- **必須**: `rule_test_coverage_check` 用の positive (`Command::new("gh")` 直叩き検出) / negative (wrapper 経由は skip) test を main.rs に追加。

#### 作業計画

- [ ] `Command::new("gh")` 直叩きを検出する rule を custom-lint-rules.toml に追加
- [ ] main.rs に positive/negative test 追加
- [ ] 既存 `.rs` の直叩き箇所を grep して false positive 計測
- [ ] `cargo test -p hooks-post-tool-linter` pass
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- `gh` の timeout なし直叩きが Write 時 (PostToolUse) に検出され timeout wrapper 使用が促される。将来同型の無期限ハング混入を構造的に予防。

---

### `filter_transcripts` の複数 jsonl 走査を timestamp ソートで deterministic 化 + regression test (PR #230 post-merge-feedback T2-#1 採用)

> **動機**: `filter_transcripts` (transcript.rs) が `fs::read_dir` の非決定的走査順で複数 `.jsonl` を処理しており、複数 Claude セッションが並存する場合にファイル間の時系列順が保証されない。downstream の takt workflow (analyze-session) が受け取る context の順序品質が低下し、ADR-030 の determinism 目標と乖離する (CodeRabbit findings)。走査結果を timestamp ソートして決定論化し、regression test で保護する。
>
> **本タスクの位置づけ**: PR #230 post-merge-feedback Tier 2 #1 採用 (Medium / Frequency Low / Effort M / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/230.md` Tier 2 #1、PR #230 (`3e7fdf9e`)、`src/cli-merge-pipeline/src/feedback/transcript.rs` (対象)、ADR-030 (determinism 目標)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。

#### 作業計画

- [ ] `filter_transcripts` の `fs::read_dir` 結果を timestamp (または名前) で sort してから処理するよう変更
- [ ] 複数 jsonl の順序が入力順に依らず決定論になることを assert する regression test 追加
- [ ] `cargo test -p cli-merge-pipeline` pass
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- 複数 `.jsonl` 入力時の filter 出力が決定論的順序になり regression test で保護される。

---

### `takt.rs` の spawn/try_wait `Err(_)` 分岐に eprintln 追加 — 原因握り潰し解消 (PR #230 post-merge-feedback T3-#1 採用)

> **動機**: `takt.rs` の `spawn()` / `try_wait()` の `Err(_) =>` 分岐がエラー詳細を握り潰しており、失敗時に `.failed` marker へ実際の原因 (`pnpm` 未検出 / 権限エラー等) が残らず L2 recovery の debugging が困難 (CodeRabbit findings)。同 crate に確立済の `write_pending_marker_logged` 等の `eprintln!` パターンを踏襲して原因を記録する。
>
> **本タスクの位置づけ**: PR #230 post-merge-feedback Tier 3 #1 採用 (Medium / Frequency Low / Effort XS / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/230.md` Tier 3 #1、PR #230 (`3e7fdf9e`)、`src/cli-merge-pipeline/src/feedback/takt.rs` (対象)、同 crate `write_pending_marker_logged` (踏襲する eprintln パターン)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。順位 238 と同 crate、1 PR bundle 検討可。

#### 作業計画

- [ ] `takt.rs` の `spawn()` / `try_wait()` の `Err(e)` を `eprintln!` で記録するよう変更 (握り潰しを解消)
- [ ] `cargo test -p cli-merge-pipeline` pass + `cargo clippy` clean
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- takt spawn/try_wait 失敗時に原因が stderr に記録され `.failed` marker からの debug が可能になる。

---

### binary crate の module symbol を `pub(crate)` 限定 + CLAUDE.md 明文化 (PR #230 post-merge-feedback T3-#2 採用)

> **動機**: PR-W3 の feedback module 分割で `write_failed_marker` / `fetch_pr_diff_summary` / `FeedbackInput` / `run` 等、external consumer が存在しない binary crate 内シンボルが `pub` export されており、`pub(crate)` 方針と乖離している (CodeRabbit findings)。file split refactor PR ごとに繰り返す systemic pattern (Frequency Medium) のため、CLAUDE.md に方針を明文化し、既存 `pub` を `pub(crate)` に揃える。
>
> **本タスクの位置づけ**: PR #230 post-merge-feedback Tier 3 #2 採用 (Low / Frequency Medium / Effort S / Adoption Risk None)。file-length-enforcement-plan.md の分割制約「Cross-module visibility は pub(crate)」の恒久 codify に相当。
>
> **参照**: `.claude/feedback-reports/230.md` Tier 3 #2、PR #230 (`3e7fdf9e`)、`src/cli-merge-pipeline/src/feedback/*.rs` (pub → pub(crate) 揃え対象)、`CLAUDE.md` (方針明文化先)、docs/file-length-enforcement-plan.md § 制約条件 (既存の pub(crate) ガイド)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。

#### 作業計画

- [ ] binary crate (cli-merge-pipeline) 内で external consumer 不在の `pub` シンボルを `pub(crate)` に変更
- [ ] `cargo build` / `cargo clippy --workspace -- -D warnings` clean を確認 (未使用 pub 警告含む)
- [ ] CLAUDE.md に「binary crate では cross-module 共有シンボルは pub(crate)、pub は使わない」方針を明文化
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- cli-merge-pipeline の module 間シンボルが `pub(crate)` に統一され、CLAUDE.md に方針が明文化される。将来の file split refactor で同型指摘が再発しない。

---

### `invoke_classifier` の stdin write → drain 順序修正で pipe deadlock 解消 (PR #231 CodeRabbit Major、pre-existing、別 PR 対応合意)

> **動機**: PR #231 (PR-W4) の CodeRabbit が [`src/cli-push-runner/src/stages/lint_screen/classifier.rs`](../src/cli-push-runner/src/stages/lint_screen/classifier.rs) の `invoke_classifier` で `stdin.write_all(diff)` を完了してから stdout/stderr の `drain_pipe_capped` thread を spawn する順序を Major 指摘。diff が大きく子プロセス (cli-finding-classifier.exe) が stdin 読込中に大量の stdout/stderr を出力すると、パイプバッファ (~64KB) が満杯になり子は書込ブロック・親は stdin 書込ブロックで相互デッドロック → push pipeline hang。
>
> **本タスクの位置づけ**: PR #231 CodeRabbit finding (Major) をユーザー合意で別 PR に切り出したもの。順序は分割前の単一 `lint_screen.rs` 時代と同一 = **pre-existing** であり、PR-W4 (mechanical refactor / behavior 不変) では ordering を変更せず、CR thread は「妥当だが pre-existing、別 PR 対応」で resolve 済。順位 220 (subprocess stress test) / 221 (Safe Subprocess Stdout Pattern ADR) と同型の subprocess lifecycle 問題。
>
> **参照**: PR #231 (`bf6977c8`)、CR thread `PRRT_kwDORGBRx86NgG1o`、`classifier.rs` の `invoke_classifier` / `spawn_classifier`、`lib_subprocess::drain_pipe_capped` / `wait_with_timeout_basic`、順位 220/221 (同型 pattern)、`.claude/feedback-reports/231.md` T1-1 (lint 化は 🤔 様子見で別枠)。
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。実 deadlock リスク (Severity High)。

#### 作業計画

- [ ] `invoke_classifier` を drain-first に変更 (`spawn_classifier` 後、stdin write より前に `stdout_handle` / `stderr_handle` を spawn)
- [ ] `cargo test -p cli-push-runner` pass (168 baseline) + `cargo clippy` clean
- [ ] 大 stdout を出す子プロセス相当の regression test を検討 (順位 220 の `--ignored` stress test 方式を踏襲可)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- stdin write 中に子プロセスが大量出力しても deadlock せず、classifier 呼び出しが完了する。順位 221 の Safe Subprocess Stdout Pattern に準拠。

---

### `pub(crate)` vs `pub` 可視性チェックリストを module split 手順に追加 (PR #231 post-merge-feedback T3-1 採用)

> **動機**: W-series (file-length enforcement Phase 1) の module split で cross-module visibility の判断が都度必要になる。crate 内で他 module から参照する共有シンボルは `pub(crate)`、`pub` は同一 crate 内の他 module からは有効だが (binary crate では `pub(crate)` と実質同等の可視性)、library target がある場合にのみ公開 API surface になる — この違いを具体例付きのチェックリストとして明示する。
>
> **本タスクの位置づけ**: PR #231 post-merge-feedback Tier 3 #1 採用 (Low / Frequency Medium / Effort XS / Adoption Risk None)。file-length 強制が継続する限り split は今後も発生。順位 241 (binary crate の pub(crate) 方針 + CLAUDE.md 明文化) と相補。
>
> **参照**: `.claude/feedback-reports/231.md` Tier 3 #1、PR #231、`docs/file-length-enforcement-plan.md` § 制約条件 (既存の「Cross-module visibility は pub(crate)」)、順位 241。**注意**: 追記先候補の `file-length-enforcement-plan.md` は PR-W5 land 後に削除予定のため、`~/.claude/rules/common/coding-style.md` または `CLAUDE.md` への恒久配置を着手時に判断する。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。

#### 作業計画

- [ ] `pub(crate)` (cross-module 共有) / module-private / `pub` (library API のみ) の判断チェックリストを具体例付きで作成
- [ ] 恒久配置先を決定 (coding-style.md / CLAUDE.md、file-length-enforcement-plan.md は暫定)
- [ ] 順位 241 との重複を統合 (bundle 検討)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- module split 時に visibility scoping を迷わず判断できるチェックリストが恒久 doc に存在する。

---

### per-module test helper 複製方針を coding-style.md に明文化 (PR #231 post-merge-feedback T3-2 採用)

> **動機**: `unique_temp_root` / `write_meta` / `parked_state` 等の test helper は各 test module に独立複製し、共有 util module を抽出しない方針 (memory `feedback_test_dry_antipattern`) が前提知識化しておらず、module split の度に混乱が再発する。coupling vs isolation のトレードオフ根拠と split レビュー時の確認項目を coding-style に追記する。
>
> **本タスクの位置づけ**: PR #231 post-merge-feedback Tier 3 #2 採用 (Low / Frequency Medium / Effort XS / Adoption Risk None)。memory `feedback_test_dry_antipattern` の恒久 codify。
>
> **参照**: `.claude/feedback-reports/231.md` Tier 3 #2、memory `feedback_test_dry_antipattern`、`~/.claude/rules/common/coding-style.md` (追記先)、`docs/file-length-enforcement-plan.md` § test helper は per-module duplicate。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。

#### 作業計画

- [ ] coding-style.md に「test helper は各 module 複製、shared util module は anti-pattern」を根拠 (coupling < isolation) 付きで追記
- [ ] split レビュー時の確認項目 (helper が複製されているか) を明示
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- test helper 複製方針が coding-style.md に明文化され、split の度の混乱が解消される。

---

### `PR_SIZE_CHECK_OVERRIDE=1` 適用ポリシーを push-runner-config.toml に明文化 (PR #231 post-merge-feedback T3-3 採用)

> **動機**: `PR_SIZE_CHECK_OVERRIDE=1` の使い方が「知っている人だけが知る」暗黙知になっており、機械的 refactor のたびに手探りが再発する。mechanical refactor (削除≒追加の line-neutral) の定義と override 判断基準を push-runner-config.toml の `[pr_size_check]` コメントまたは docs に明記する。
>
> **本タスクの位置づけ**: PR #231 post-merge-feedback Tier 3 #3 採用 (Low / Frequency Medium / Effort XS / Adoption Risk None)。file-length 強制が続く限り機械 refactor の override 判断は今後も発生。
>
> **参照**: `.claude/feedback-reports/231.md` Tier 3 #3、順位 151 (`pr_size_check` stage)、`push-runner-config.toml` `[pr_size_check]` section (追記先)、`docs/file-length-enforcement-plan.md` § push 手順 (override use case)。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。

#### 作業計画

- [ ] push-runner-config.toml `[pr_size_check]` コメントに override 適用基準 (mechanical refactor 定義 + PR description 明記事項) を追記
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- override の適用基準が config コメントに明文化され、機械 refactor 時の判断が暗黙知でなくなる。

---

### monitor の CI 完了判定を短絡 — CodeRabbit review-complete + mergeability CLEAN で CI 待機を skip (PR #232 post-merge-feedback T2-1 採用)

> **動機**: 本リポジトリは check が CodeRabbit のみで GitHub Actions 等の実 CI が存在しない構成。この構成で cli-pr-monitor の poll が「CI: pending」を完了と判定できず recheck を上限まで繰り返す。PR #231 / #232 の両方で、GitHub API を直接確認 (`gh pr view --json mergeStateStatus,mergeable` → `CLEAN` / `MERGEABLE`) して merge 可能を人手で確認する必要が生じた (= 幻の CI pending)。
>
> **本タスクの位置づけ**: PR #232 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。docs-only PR で共通に再現する pattern。
>
> **参照**: `.claude/feedback-reports/232.md` Tier 2 #1、`src/cli-pr-monitor/src/stages/poll/` (CI 完了判定 + poll ループ)、PR #231 / #232 (幻の CI pending を手動 GitHub API 確認で回避した実例)、ADR-018 (park モデル)。
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。既存 poll ループへの条件分岐追加のみ (parse logic 改修不要)。

#### 設計決定 (案)

- CI 状態が「実 check 不在 or CodeRabbit のみ」かつ CodeRabbit review が完了 (unresolved 0 / actionable 0) かつ mergeability が `CLEAN` / `MERGEABLE` の場合、CI 待機 (pending) を短絡して merge-ready 判定に倒す。
- 誤短絡防止: 実 CI check が 1 件でも存在し pending なら従来通り待機 (CodeRabbit-only 構成に限定)。

#### 作業計画

- [ ] poll の CI 完了判定に「review-complete + mergeability CLEAN」短絡条件を追加
- [ ] CodeRabbit-only 構成の判定 (実 CI check の有無) を実装
- [ ] `cargo test -p cli-pr-monitor` pass + regression test (短絡が誤発火しないこと)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- CodeRabbit-only 構成の PR で review 完了 + CLEAN なら monitor が recheck を無駄に繰り返さず merge-ready と判定する。実 CI がある場合は従来の pending 待機を維持。

---

### review-jj-robustness-whole facet (観点⑧) の dogfood + bounded-lifetime 評価 (ADR-031 拡張、順位247)

> **動機**: PR-2 で ADR-031 週次レビューに観点⑧ (jj-workspace robustness) の facet を新規追加した。非 colocated / 並列 jj workspace (ADR-045) 特有の silent bug 4 class (mtime staleness / `CARGO_MANIFEST_DIR` 実行時読み / `--repo` 無し gh / colocated `.git` 前提) を whole-tree で検出する。新規実験 facet のため ADR-039 § Bounded Lifetime に従い有効性を dogfood で観測して採否を判定する。
>
> **本タスクの位置づけ**: ADR-031 拡張 (観点⑧)、ADR-039 experimental pattern の bounded-lifetime 評価枠。2-3 週 dogfood で「既知 4 bug class を実検出できるか」「false positive 率」を観測し、有用なら定着、低品質なら facet を retire (ADR-031 § 採用判定の閾値 を参照)。
>
> **参照**: ADR-031 § 将来の展望 (観点⑧)、`.takt/facets/instructions/review-jj-robustness-whole.md`、ADR-045 (jj workspace 並列運用)、ADR-039 (bounded lifetime)、2026-07 セッションで実観測した 4 bug class (weekly-review staleness / stale `CARGO_MANIFEST_DIR` / untracked state 消失 / 非 colocated gh 失敗)

#### 作業計画

- [ ] 次回 `/weekly-review` で observability 観測 (既知 4 bug class 相当を実検出できるか、context 圧迫の有無)
- [ ] 2-3 週 dogfood で採用率 / false positive を ADR-031 § 採用判定の閾値 で評価
- [ ] 有用 → 定着 (本 entry 削除 + todo-summary 行削除) / 低品質 → facet retire (weekly-review.yaml + aggregate から除去)

#### 完了基準

- 観点⑧ facet が既知 bug class を再現検出でき、false positive ≤ 5% で定着判定。または retire 判定で facet を除去し軸の空白を記録。

---

### Gate Function Design Checklist を新規 guide として追加 (fail-closed パターン集) (PR #234 post-merge-feedback T3-1 採用)

> **動機**: fail-closed 実装の失敗パターンと推奨パターンが複数の ADR / memory に分散しており、新規 gate 実装者が再発させるリスクが高い。PR #234 で `collect_oversize_files` の初版が `.ok()?` で読み取り失敗を握り潰す fail-open bug を含み CodeRabbit Major #234-1 で指摘された。gate 実装の失敗/推奨パターンを 1 箇所に集約する。
>
> **本タスクの位置づけ**: PR #234 post-merge-feedback Tier 3 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。同 feedback の T1-1 (`filter_map + .ok()?` の linter 化) / T1-2 (TOCTOU linter 化) は false positive 多発リスクで却下推奨となったため、その補完としてドキュメント化が必須。
>
> **参照**: `.claude/feedback-reports/234.md` Tier 3 #1、`docs/adr/adr-043-security-gates-fail-closed.md` (fail-closed 原則)、順位 249 (ADR-043 コード例追記、相補)、custom lint ⑫ `no-hardcoded-jj-revset-range`。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。

#### 作業計画

- [ ] Gate Function Design Checklist を `CLAUDE.md` patterns section または `docs/guides/gate-functions.md` に新設: (1) 判定不能状態は fail-closed、(2) gate 関数内で `filter_map + .ok()?` 禁止、(3) single-pass file access で TOCTOU 回避、(4) iterator chain + `Result::?` idiom で nesting depth 抑制、(5) エラーパスを明示的にテスト
- [ ] ADR-043 (順位 249) との相互リンク
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- fail-closed gate の失敗/推奨パターンが 1 箇所に集約され、新規 gate 実装者が参照して再発を防げる。

---

### ADR-043 に fail-open vs fail-closed の具体コード例を追記 (PR #234 post-merge-feedback T3-2 採用)

> **動機**: ADR-043 は security-critical だが具体的なコード例が未記載で、解釈の分散が PR #234 の `.ok()?` fail-open bug を生んだ。`.ok()?` anti-pattern / single-read + `ErrorKind` inspection idiom / multi-step vs 単一操作の比較を ADR 本文に追記し、レビュー時の一貫した判断基準を提供する。
>
> **本タスクの位置づけ**: PR #234 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。順位 248 (運用チェックリスト) と相補的な決定記録。
>
> **参照**: `.claude/feedback-reports/234.md` Tier 3 #2、`docs/adr/adr-043-security-gates-fail-closed.md` (追記先)、順位 248 (Gate Function Design Checklist)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。

#### 作業計画

- [ ] ADR-043 に具体コード例 section を追加 (`.ok()?` anti-pattern / single-read + `ErrorKind` idiom / TOCTOU 回避の単一操作)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- レビュー時に fail-open / fail-closed の判断基準が具体コードで参照でき、解釈の分散が解消される。

---

### ADR-021 に「jj revset の base branch は config/arg 化 (hardcode 禁止)」を明文化 (PR #234 post-merge-feedback T3-3 採用)

> **動機**: PR #234 で `[file_length_gate] base` を config 引数化する ADR-021 準拠パターンを実装した (default `master`、`format!("{}..@", base)`)。custom lint ⑫ `no-hardcoded-jj-revset-range` は `.rs` の `master..@` literal を捕捉するが、TOML config / docs / 他ツールへの原則適用は明文化されていない。base branch hardcode 禁止の原則を明文化する。
>
> **本タスクの位置づけ**: PR #234 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。jj change detection は複数ツールで多用されるため原則の明文化価値がある。
>
> **参照**: `.claude/feedback-reports/234.md` Tier 3 #3、`docs/adr/adr-021-jj-change-detection-principles.md` (追記先)、custom lint ⑫ `no-hardcoded-jj-revset-range`。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。

#### 作業計画

- [ ] ADR-021 (または `CLAUDE.md`) に「jj revset の base branch は config / arg 化し hardcode 禁止」の原則を明文化 (`.rs` / TOML config / docs / 他ツール横断)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- base branch hardcode 禁止の原則が明文化され、新規 jj 変更検出実装で参照できる。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo10.md / todo9.md 末尾を参照。)
