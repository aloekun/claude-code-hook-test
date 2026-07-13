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

#### 作業計画 (Status update 2026-07-03: 2 PR 構成に再編、PR-1 = A1+B1 実装済)

- [x] A1: `fix.md` 完了ゲートに `--ignored` 必須条件を追記 (PR-1、条件 = test ファイル変更 or `pub`/`pub(crate)` 関数の挙動・signature 変更時)
- [x] B1: auto-push (`repush.rs`) に push 前 quality_gate (`--ignored` 含む) を挿入 (PR-1、`stages/gate.rs` 新設。push-runner-config.toml の quality_gate group を単一ソース参照、docs-only fix diff は ADR-035 path 基準で gate skip、FAIL は `action_required` 即 escalation)
- [ ] dogfood: [docs/auto-push-gate-dogfood.md](auto-push-gate-dogfood.md) の観測ログ + GO/NO-GO 判断基準に従い B1-loop 要否を判定 (期限: PR-1 merge + 6 週間 / gate FAIL 2 件 / auto-push 発火 10 回 のいずれか先)
- [ ] (GO 判定時のみ) B1-loop: gate FAIL を convergence ループに差し戻す経路 + N 回上限 → `action_required` (fail-closed) — 設計案・不採用案は dogfood doc §5 に保存済み
- [ ] 本 entry 削除 + todo-summary.md 行削除 + dogfood doc 削除 (同一 commit、NO-GO の場合は ADR-043 amendment に知見移管後)

#### 完了基準

- takt fix が #[ignore] テストを壊す変更をしても fix 時 (A1) または auto-push 前 (B1) に必ず検出され、`jj git push` 直 push で gate を迂回する経路が解消 (PR-1 で達成)。B1-loop の要否が dogfood 観測で判定され、GO なら機械収束・NO-GO なら即 escalation 恒久化として決着していること。

#### 詰まっている箇所

- (解消済 2026-07-03) B1-loop の接続方式と N 値は dogfood doc §5 に設計案として確定 (専用 `gate-fix.yaml` + N=2)。残る不確定要素は dogfood 観測結果のみ。

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

### ADR-NNN (採番未確定、land 時に確定): 部分効果 env var anti-pattern の文書化 (PR #239 post-merge-feedback T3-1 採用)

> **動機**: サブコマンドによって効果が異なる env var は「一部のコマンドが成功する」ことで全体が動いていると誤認させ、silent 部分故障を招く。実例 = `GH_REPO` は `gh pr create/merge` には効くが引数なし `gh repo view` には効かず、PR #238 で「マージ成功 / post-merge feedback silent 消失」の部分故障が発生した。`gh-repo-env-guard` preset (PR #239) が GH_REPO 個別には機械防御するが、「なぜ partial coverage が危険か」の原則が未文書化で、同型のショートカット提案 (例: `GH_HOST` 系 env var) を reviewer / implementer が即認識できない。
>
> **参照**: PR #238 (実害) / PR #239 (preset 実装 + feedback 提案 #1)、ADR-045 § PR 運用時の追加設定、`.claude/hooks-config.toml` gh-repo-env-guard preset。
>
> **実行優先度**: 💎 Tier 3 — Effort S。Severity Medium + Frequency Medium + Adoption Risk None (PR #239 post-merge-feedback T3-1、ユーザー採用 2026-07-03)。

#### 作業計画

- [ ] 新 ADR (順位 135 placeholder policy 適用) に「部分効果 env var」anti-pattern を codify: 定義 / PR #238 実例 / 判定基準 (env var による回避策採用時は対象コマンド全系統でのカバレッジ確認を必須化) / 推奨代替 (全系統に効く機構 = GIT_DIR 自動注入型、または明示フラグ)
- [ ] CLAUDE.md の ADR 一覧にリンク追加 (ADR-022 の「CLAUDE.md はリンクに留める」方針に従い本文は ADR 側へ)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- 将来の env var ベースの回避策提案に対し、reviewer / implementer がカバレッジ確認を求める根拠文書が ADR として存在し、CLAUDE.md から辿れること。

#### 詰まっている箇所

- なし。

---

### ADR-030 に PR #239 の feedback silent skip 実装記録を追記 (PR #239 post-merge-feedback T3-2 採用)

> **動機**: 非 colocated jj workspace での owner_repo 検出失敗 → `.failed` marker 未書込 → L2 recovery 未発動という silent skip シナリオ (PR #238 実観測、feedback が recovery 不能なまま消失) と、その対処 (`AiStepContext` enum 化 + `SkipWithMarker` variant + `skip_with_failed_marker()`) を ADR-030 に実装記録として残し、次回同類問題の参照点にする。
>
> **参照**: PR #238 (実害) / PR #239 (`src/cli-merge-pipeline/src/pipeline.rs` の `AiStepContext::SkipWithMarker`)、ADR-030 (失敗マーカーによる recovery)。
>
> **実行優先度**: 💎 Tier 3 — Effort XS。Severity Low (既修正) + Frequency Low (PR #239 post-merge-feedback T3-2、ユーザー採用 2026-07-03)。次回 ADR-030 を参照・編集する PR への同乗で消化可。

#### 作業計画

- [ ] ADR-030 に「owner_repo 検出失敗などの実行前 skip も marker 付き skip とし L2 recovery 対象にする (`AiStepContext::SkipWithMarker`)」の実装記録 sub-section を数行追記 (PR #238 シナリオを inline cite)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- ADR-030 を読んだ実装者が「feedback の skip 経路はすべて marker を残す」規約を実装記録から把握できること。

#### 詰まっている箇所

- なし。

---

### pr_size_check の base を remote tracking ref に変更 — 並列 workspace のローカル master 遅延による誤計測解消 (順位242 push で実観測)

> **動機**: `push-runner-config.toml` `[pr_size_check]` の `default_branch = "master"` が revset `master..@` のローカル bookmark 基準のため、ADR-045 並列 workspace 運用でローカル `master` (workspace 間共有) が誰にも advance されず遅延していると、過去の merge 済み PR 分を合算して誤計測する。順位 242 の push で実害: 実 diff +123/-35 (~160 行) が「1604 行 > block_threshold 1500」と誤 block され、直前の PR #239/#240 push でも warning 閾値 (800) を静かに誤超過していた。ADR-013 では `sync_local` が「remote tracking ref (`master@origin`) を使い bare local bookmark を使わない」を test で固定済みで、同じ原則を pr_size_check にも適用すべき。
>
> **参照**: `push-runner-config.toml` `[pr_size_check]`、`src/cli-push-runner` の pr_size_check stage、ADR-013 (sync_local の master@origin 原則 + 固定 test)、ADR-021 / 順位 250 (base branch config/arg 化の明文化、相補)、ADR-045 調整ポイント 2 (ローカル master 共有と遅延の前提)。
>
> **実行優先度**: 🔧 Tier 2 — Effort XS-S。並列 workspace 運用が続く限り再発する (今回は手動 `jj bookmark set master -r master@origin` で復旧)。

#### 作業計画

- [ ] `[pr_size_check] default_branch` を `master@origin` に変更 (config 1 行) または pr_size_check 側で remote tracking ref を優先解決する fallback を実装 (着手時に判断、`[file_length_gate] base` も同点検)
- [ ] ローカル master 遅延状態を模した test (revset 解決の単体レベル) を検討
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- ローカル `master` が遅延していても pr_size_check が「master@origin 以降の実 diff」だけを計測すること。

#### 詰まっている箇所

- なし (根因・復旧手順・実測値あり)。

---

### ADR-040 の実測値を新 GPU (RTX PRO 5000 48GB) で再 calibration (ADR-046 WP-01 スパイクで陳腐化を観測)

> **動機**: ADR-038 / ADR-040 は Local LLM の実行環境を **RTX 3070 8GB** として実測値を固定しているが、実機は **NVIDIA RTX PRO 5000 Blackwell 48GB** に更新済み (2026-07-04 に `nvidia-smi` で確認)。この結果、(1) ADR-040 の VRAM/latency trade-off 表 (例: mistral:7b ~2GB at 32K ctx) と (2)「VRAM scarcity → 同時起動不可 / model swap 制約 / KV cache budgeting」という framing が陳腐化した。27-31B Q4 モデルが 100% GPU で動き (qwen3-coder:30b ~21.8GB / gemma4:31b ~20.9GB / gemma4:26b ~17.6GB at num_ctx 32768)、VRAM ではなく latency が実効制約になった。
>
> **参照**: ADR-040 (Local LLM Context Size、実測値元)、ADR-046 (WP-01 スパイク、4 モデルの VRAM・latency 実測を保持)、ADR-038 (現行 classifier、RTX 3070 前提の記述)、memory `gpu-upgrade-rtx-pro-5000`。
>
> **実行優先度**: 💎 Tier 3 — Effort S。実装変更を伴わず ADR amendment 中心。分類層 (ADR-038) の運用に直接の不具合はないが、num_ctx 再選定や派生プロジェクト porting 時に誤った RTX 3070 前提を引き継ぐリスクを解消する。

#### 作業計画

- [ ] ADR-040 に amendment: RTX 3070 8GB の実測表は「旧環境 (historical)」と明示し、新 GPU での再測定値 (ADR-046 の VRAM 実測 + 代表 diff の latency) を追記
- [ ] 「Context 選定の判断 flow」の memory 軸 (同時起動可否 / swap) を latency 軸へ再重み付け
- [ ] ADR-038 の RTX 3070 前提記述 (§コンテキスト / §帰結の VRAM 8GB 制約) に更新環境への参照を付す
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- ADR-040 を読んだ実装者が、現行 GPU では VRAM が制約でなく latency が実効制約であることを把握でき、RTX 3070 8GB の数値を現行前提と誤認しないこと。

#### 詰まっている箇所

- なし (GPU 更新の事実・ADR-046 の実測値あり)。

---

### classifier FP 検出強化プロンプトで格上げ候補を再評価 (WP-04 見送りの follow-up、ADR-038 amendment 由来)

> **動機**: WP-04 (classifier モデル格上げ) の実測で、mistral:7b からの格上げ候補 (gemma4:12b/26b/31b, qwen3-coder:30b) は **いずれも `false_positive_likely` 検出を改善しなかった** (gold FP 6 件中、正検出は最良 qwen3-coder でも 1 件、全モデルが 3〜4 件を有害な auto_fix に誤分類)。ただし eval で使った `classify.txt` は mistral:7b 向けに tune 済みのため、「FP 検出が能力限界なのか、プロンプト不適合なのか」が未分離。FP 検出を明示的に強化したプロンプト版で候補を再測し、切り分ける。
>
> **参照**: ADR-038 § classify モデル格上げの評価と見送り (2026-07-05 追記、WP-04)、`src/cli-finding-classifier/prompts/classify.txt`、`src/cli-finding-classifier/src/main.rs` (`--prompt-file` で差し替え可)、WP-04 scratchpad の eval セット (Opus gold 35 件) + ハーネス。ADR-019 § 既知 CodeRabbit FP パターン (キュレート FP 例の出典)。
>
> **実行優先度**: ⏳ Tier 5 — Effort M。現行 mistral:7b は安全軸完璧・最軽量で運用に支障なく、優先度は低い。materially better な新 local モデル出現時も再評価トリガー。

#### 作業計画

- [ ] FP 検出強化版 `classify.txt` を作成 (false_positive_likely の positive signal をより明示、Windows 専用/test mock/合成 fixture 等の既知 FP パターンを few-shot 化)
- [ ] WP-04 の Opus gold eval セット (35 件) で qwen3-coder:30b 等を再測、FP recall と human_review 安全軸を確認
- [ ] 能力限界と確認できれば恒久見送りとして本 entry 削除。プロンプト不適合なら該当モデル + 専用プロンプトで格上げ (ADR-038 の model default 変更 + amendment)
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- FP 検出が「モデル能力限界」か「プロンプト不適合」かが実測で切り分けられ、格上げ採否が結論付けられていること。

#### 詰まっている箇所

- なし (WP-04 の eval 資産・gold セットあり、プロンプト改訂のみ)。

---

### push pipeline の `cargo test` を cargo-nextest 化 (WP-05 で Stop hook には無効と判明、push 側 follow-up)

> **動機**: WP-05 (Stop hook 高速化) の実測で、当初計画の nextest 案は **Stop hook には無効**と判明した (Stop hook は cargo test を実行せず、真因は 7 ステップの逐次実行 → 並列化で解決済、ADR-004 amendment)。一方、**push pipeline (cli-push-runner の quality_gate) は `cargo test -- --ignored --test-threads=1` を実行**しており、実測で ~80s を要する (WP-03 push で観測)。ここは nextest による並列テスト実行で高速化の余地がある。push は Stop hook より低頻度だが、fix→push サイクルの待ち時間に直結する。
>
> **参照**: `push-runner-config.toml` の `[[quality_gate.groups]]` name=`rust-lint-test`、`src/cli-push-runner` の quality_gate stage、ADR-004 § ステップ並列実行による高速化 (2026-07-05 追記) の scope 外 note、ADR-017 (takt バージョン固定哲学 = nextest 固定の根拠)。
>
> **実行優先度**: ⏳ Tier 5 — Effort S-M。現行 push は機能上支障なく、優先度は低い。ツール依存追加の費用対効果を要評価。

#### 作業計画

- [ ] cargo-nextest の導入判断: ツール依存追加 (ADR-017 pinning + `pnpm deploy:hooks` 派生プロジェクト配布) のコスト vs push 高速化の便益を評価
- [ ] 採用時: `push-runner-config.toml` の `cargo test` step を `cargo nextest run` に置換。**nextest は doctest を実行しないため `cargo test --doc` を併走**させる (doctest 有無を確認: `///` の ` ``` ` を持つ crate)
- [ ] `--ignored` 統合テスト (repush 等) が nextest で正しく実行されることを確認 (nextest の `--run-ignored` フラグ)
- [ ] before/after 実測で push pipeline 時間短縮を確認
- [ ] 本 entry 削除 + todo-summary.md 行削除

#### 完了基準

- push pipeline の test 実行時間が短縮され、doctest / `--ignored` 統合テストの網羅性が維持されていること。または費用対効果が見合わないと判断し見送りが記録されていること。

#### 詰まっている箇所

- なし (WP-05 で Stop 側は完了、push 側の nextest 適用余地とコスト構造は明確)。

---

### pre-push review-diff.txt の生成形式を jj diff --git に切替 — LLM レビュアーの add/delete 誤読解消 (PR #256 post-merge-feedback Tier1 #1 採用)

> **動機**: `push-runner-config.toml:113` の `[diff] command = "jj diff -r @"`（jj デフォルト形式）で生成される `.takt/review-diff.txt` は、追加/削除を色 + 行番号2列（`NNN     :` = 削除 / `     NNN:` = 追加）で表現する。ファイル化で色が落ちると `-`/`+` マーカーが無くなり、削除が「左列のみ行番号」でしか区別できず、pre-push の LLM レビュアー（simplicity-review 等）が削除ブロックを「追加」と誤読しうる。`--git`（標準 unified diff）は色非依存で `+`/`-` を明示するため誤読しない。PR #256（ADR-051 起票 PR）で todo エントリ25行の**削除**を simplicity-review が「追加」と誤読し stale-tracking-entry として false positive REJECT を出し、レビュー約19分を浪費した実害が発生した。
>
> **本タスクの位置づけ**: PR #256 post-merge-feedback Tier1 #1 で採用（他6提案は over-engineering として却下）。fix ステップの「hunk-polarity bug」という診断は不正確で、真因は色を落とした平文 diff の LLM 可読性問題。
>
> **参照**: `push-runner-config.toml:113`（`command = "jj diff -r @"` → `"jj diff --git -r @"`、修正対象）、`templates/push-runner-config.toml:52`（同様の変更、`pnpm deploy:hooks` で派生プロジェクトに配布されるため**両方修正必須**）、memory `prepush-review-diff-plain-format-misread.md`、PR #256 feedback report (`.claude/feedback-reports/256.md`) Tier1 #1
>
> **実行優先度**: 🔧 Tier 2 — Effort S。false positive で約19分浪費した実害が既に発生しており、config + template 各1箇所の軽微な修正で再発を防止できる。

#### 設計決定 (案)

- `[diff] command` を `jj diff --git -r @` に変更。本番 config と template の2箇所を同一 PR で修正（template 未修正だと派生プロジェクトに同じ false positive が横展開）。
- review-diff.txt を format-sensitive に parse する `.rs` 箇所は存在せず（LLM facet が読むのみ）、Adoption Risk None。

#### 作業計画

- [ ] `push-runner-config.toml:113` を `command = "jj diff --git -r @"` に変更
- [ ] `templates/push-runner-config.toml:52` も同様に変更
- [ ] review-diff.txt を参照する箇所（facet instruction / `.rs`）が `--git` 形式で問題ないか確認
- [ ] dogfood: 削除を含む diff で pre-push review が正しく削除を認識することを確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- pre-push review が削除ブロックを「追加」と誤読しなくなり、config + template 両方が `--git` 形式、派生プロジェクトへの横展開も解消。

#### 詰まっている箇所

- なし（変更箇所・影響範囲とも確定済み、PR #256 feedback report で cross-validation 済み）。

---

### cli-docs-lint に ADR 重複採番 + CLAUDE.md 索引整合チェック追加 (PR #261 post-merge-feedback T1-#2 採用)

> **動機**: PR #261 で当方が ADR-052 として起草した ADR が、並行 land した PR #260 の ADR-052 (自律実行境界) と採番衝突し、rebase 時にファイル名 + 本文タイトル + ソース内参照 10+ 箇所の置換が発生した実例。ADR は既に 53 件、並行 PR 開発が常態化しており再発頻度 Medium。現状この衝突を機械検知する層が存在しない (発見は rebase 時の CLAUDE.md conflict 頼み)。
>
> **チェック内容 (案)**: (a) `docs/adr/adr-NNN-*.md` の同一 NNN 重複検出、(b) CLAUDE.md 索引 ⇔ 実ファイルの対応検証 (索引にあるファイルの存在 / 実ファイルの索引掲載)、(c) ファイル名の NNN ⇔ 本文 H1 タイトル番号の一致。
>
> **参照**: `.claude/feedback-reports/261.md` Tier 1 #2、`src/cli-docs-lint/src/main.rs` (CheckMode 拡張、preamble / cross-ref / priority-inversion の既存 check-mode dispatch と kill-switch 骨格を流用)、ADR-007 (層の線引き)、ADR-039。
>
> **関連 (重複ではない)**: 順位 135 (todo8.md、ADR-NNN placeholder policy) は todo entry 側の採番 hardcode を防ぐ「ルール」であり、本 entry は land 済みファイル群の衝突を検知する「仕組み」(ADR-042 の役割分担で相補)。feedback report Tier 2 #2 (ADR sanity テスト新設) は本 entry と目的重複のため却下済み。
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。既存 cli-docs-lint 骨格の流用で新規 module 1 つ + fixture テスト。

#### 作業計画

- [ ] `src/cli-docs-lint/src/` に adr_consistency validator module を新設 (check 内容 a/b/c)
- [ ] 既存 CheckMode dispatch / kill-switch 設定に統合 (ADR-039 パターン)
- [ ] fixture テスト: 重複採番 / 索引欠落 / 番号不一致の bad fixture + clean fixture
- [ ] push-runner quality_gate (`pnpm lint:docs`) 経由で発火することを確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR 採番衝突・索引不整合・ファイル名/タイトル番号不一致が push 前に決定論的に検出され、PR #261 型の rebase 時大量置換が再発しない構造になっていること。

---

### 層別テストテンプレート (StubOllama パターン・integration 独立性) の共有化 (PR #265 post-merge-feedback T2-1 採用)

> **動機**: WP-11 (PR #265、ADR-054) の多層防御実装で、層別テスト戦略の設計に時間を要した。具体的には (a) 空 responses の `StubOllama` で「LLM が呼ばれていないこと」を証明する短絡検証パターン、(b) tempdir + `jj git init` + CwdRestore で実 jj repo を立てる integration テストの独立性パターン、の 2 つを都度設計した。WP-17 (自律化) で classifier / scope guard 層を拡張する際に同種の設計判断が再発する見込み。
>
> **参照**: `.claude/feedback-reports/265.md` Tier 2 #1、`src/cli-finding-classifier/src/lib.rs` (StubOllama)、`src/cli-pr-monitor/src/stages/scope_guard.rs` (integration パターン)、ADR-041 (test isolation patterns)、ADR-025 (CwdRestore)、ADR-044 (共通化と分離の線引き — shared crate 化の境界判定に適用)
>
> **実行優先度**: 🔧 Tier 2 — Effort M。WP-17 着手前の実施が効果的。

#### 作業計画

- [ ] 対象パターンの棚卸し (StubOllama / tempdir+jj init+CwdRestore / 層別テストの構成方針)
- [ ] ADR-044 の境界判定で shared test crate 化 or fixture + doc 化を判断
- [ ] 切り出し + 既存呼び出し側 (cli-finding-classifier / cli-pr-monitor) の移行
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 新しい LLM 系 / jj 統合系のテストが共有テンプレートを参照して層別テストを組める状態になっていること。

---

### ADR-007 に「コメント配置の意思決定フロー」を追加 (PR #265 post-merge-feedback T3-2 採用)

> **動機**: PR #265 実装中に `classify_one` (cli-finding-classifier) と `config.rs` (cli-pr-monitor) の 2 箇所で非 doc コメントを書き、Bundle Z comment-lint (#B-α) に block された (同種ミス 2 回 = パターン化の価値あり)。「この説明は doc コメント (`///`) に書くべきか、識別子名 / 関数分割で表現して削除すべきか」の判断フローが未文書化。linter 自動化 (feedback Tier 1 #2) は意味論的判定 = NLP が必要なため却下済みで、本エントリは人間 / AI の判断補助ドキュメントとしての補完。
>
> **参照**: `.claude/feedback-reports/265.md` Tier 3 #2、`docs/adr/adr-007-custom-linter-layer-boundary.md` (既存 Q1-Q3 判断フロー形式で拡張)、`src/hooks-post-tool-comment-lint-rust` (Bundle Z #B-α)
>
> **実行優先度**: 💎 Tier 3 — Effort S。doc のみ、バッチ PR で消化可。

#### 作業計画

- [ ] ADR-007 に Q 形式の「コメントを書きたくなったときの配置判断フロー」を追記 (doc コメント / 識別子名 / マーカー付き Why コメントの 3 分岐)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- コメント配置の判断が ADR-007 の判断フローで一意に決まり、Bundle Z block の手戻りが減ること。

---

### PR body 配置タイミング規約を dev-conventions に明記 (PR #265 post-merge-feedback T3-3 採用)

> **動機**: PR #265 の push パイプライン実行中に working copy へ `__pr-body.md` を作成し、jj snapshot 直前の退避で commit 混入をかろうじて回避したヒヤリハットが実発生。混入すると repo 履歴に残る。「PR body は push 完了後に scratchpad で準備し、`pnpm create-pr -- --body-file` に絶対パスで渡す (push 実行中の working copy に置かない)」というタイミング規約が未文書化。
>
> **参照**: `.claude/feedback-reports/265.md` Tier 3 #3、`docs/dev-conventions.md` (追記先)、ADR-028 (external-output 実行フロー)、`src/cli-pr-monitor/src/stages/create_pr.rs` (--body-file パススルー実装)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。doc のみ、バッチ PR で消化可 (並列安全化 PR の docs への相乗りも可)。

#### 作業計画

- [ ] dev-conventions.md に PR body 配置タイミング規約を追記 (scratchpad + 絶対パス推奨 / repo 直下 `__` ファイルは push 完了後のみ)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- PR body ファイルが push パイプラインの snapshot に混入しない手順が規約として参照可能になっていること。

---

### pipeline lock + Stop hook 品質ゲート skip 機構 (PR #267 マージ事故の根本解決)

> **動機**: PR #267 のマージで「background の merge pipeline がローカル同期の checkout を実行中に、ターン終了で発火した Stop hook 品質ゲート (cargo/jj) が同じ working copy 上で競合」し、jj が「Concurrent checkout」で中断・working copy が旧状態に取り残される事故が実発生 (jj 側の保護と自動解決で損失ゼロ、復旧 3 コマンド)。実施済みの `--feedback-only` (PR #268) は結果の一つ (feedback 未実行) の回復手段にすぎず、競合自体は防げていない。post-merge feedback の transcript window (first_commit〜merged_at) はマージ処理中の事故を構造的に見えないため、feedback からの提案も期待できない。
>
> **設計案**: (1) cli-merge-pipeline / cli-push-runner が実行中に lock ファイル (PID + timestamp、`cli-pr-monitor/src/lock.rs` の stale takeover パターンを lib 化して流用) を保持、(2) hooks-stop-quality が開始時に lock を確認し、生きている pipeline 保持中は品質ゲートを skip (ログ + fail-open。Stop 時点のゲートは助言層で、本物のゲートは push 側)、(3) ADR-045 運用ルールに「lock 機構実装までは merge-pr を foreground 実行」の暫定ルールを追記済み → 実装後に撤去。
>
> **参照**: ADR-045 § Known operational risks、ADR-030 (feedback recovery)、ADR-043 (skip は助言層ゲートの fail-open で整合)、`src/cli-pr-monitor/src/lock.rs` (流用元)、PR #268 (`--feedback-only` = 対症療法側)
>
> **実行優先度**: 🚀 Tier 1 — Effort S-M。**PR #268 の次の PR で対応予定 (ユーザー合意済み 2026-07-13)**。

#### 作業計画

- [ ] lock を lib (lib-jj-helpers or 新 lib) に一般化 (PID + timestamp + stale takeover)
- [ ] cli-merge-pipeline / cli-push-runner の実行区間で lock 保持
- [ ] hooks-stop-quality に lock 検知 → skip (fail-open) を追加 + off/stale ケースのテスト
- [ ] ADR-045 の「merge は foreground」暫定ルールを撤去し本機構に置換
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- background の merge/push pipeline 実行中にターンを終了しても Stop hook 品質ゲートが working copy に触れず、Concurrent checkout 事故が構造的に再発しないこと。

---

### config-reading hook の `current_dir()` 解決を検出する lint rule (PR #267 post-merge-feedback T1-1 採用)

> **動機**: PR #267 で新規 hook (jj-op-verify) が既存 3 hook と異なる `current_dir()` ベースの config 解決を実装し、pre-push simplicity-review が REJECT (`SIM-NEW-jjopverify-cwd-config-L179`、High) → fix step が `current_exe().parent()` へ修正した実例。Bash の cwd drift による silent fail-open (`enabled=false` 扱い) は新規 hook 追加のたびに再発しうる。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 1 #1、`.claude/custom-lint-rules.toml` (新規ルール)、順位 287 (convention 明文化、同一 PR bundle 推奨)
>
> **実行優先度**: 🚀 Tier 1 — Severity High / Effort S。

#### 作業計画

- [ ] custom-lint-rules.toml に「hooks-* の .rs で `current_dir()` + `hooks-config.toml` の組合せ」を検出するルール追加 (bad/good fixture + incident 構造)
- [ ] 順位 287 (convention 明文化) を同一 PR で bundle
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- config を cwd 基準で解決する新規 hook が push 前に決定論的に検出されること。

---

### jj-op-verify の変更系 verb 網羅拡大 (PR #267 post-merge-feedback T1-2 採用)

> **動機**: 現行の検出対象 (new/describe/abandon/rebase/squash/bookmark 変更系) に `undo` / `restore` / `split` / `bookmark move` / `bookmark track` / `bookmark untrack` が含まれない。特に `jj undo` の検出漏れは lost-update 再発リスクが高く、Operation Verification Checklist 自動化の対象を狭める。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 1 #2、`src/hooks-post-tool-jj-op-verify/src/main.rs` (match 文)。**拡張時は `expected_op_keyword` を実際の `jj op log` 出力と要照合**
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort M。

#### 作業計画

- [ ] 各 verb の実際の op description を jj 0.42 実機で確認し keyword map に追加 + テスト
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 変更系 jj 操作の検出網羅率が上がり、`jj undo` 等の op 記録検証が機能すること。

---

### jj-op-verify の verb 検出を command-boundary に anchor (PR #267 post-merge-feedback T1-3 採用)

> **動機**: `split_whitespace()` の非 anchored 検出は、commit message 引用符内の `"jj new"` 等で false positive「operation not recorded」を誘発しうる。実装時に accepted risk として一度見送った経緯あり (実害観測 0 件)。採用は「advisory 層の UX 劣化」防止目的で、着手時に実観測状況を再確認すること。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 1 #3、`src/hooks-post-tool-jj-op-verify/src/main.rs:detect_last_mutating_jj_op`、順位 285 (edge-case テスト、表裏の関係)
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort S。

#### 作業計画

- [ ] verb 検出をコマンド境界 (`&&` / `;` / `|` / 文頭) anchor に変更 + 引用符内の誤検出テスト
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- commit message 内の jj キーワードで警告が誤発火しないこと。

---

### stale_check_enabled の TOML パーステスト追加 (PR #267 post-merge-feedback T2-1 採用)

> **動機**: PR #267 で追加した `StalenessConfig.stale_check_enabled` のパース経路にテストがなく、silent degrade (機能が黙って無効化) のリスク。既存テストへの数行追加で完備できる。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 2 #1、`src/hooks-session-start/src/hooks_config.rs` の既存パーステスト
>
> **実行優先度**: 🔧 Tier 2 — Effort XS。

#### 作業計画

- [ ] 既存 fixture に `stale_check_enabled = true` + assert を追加
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 新フィールドのパースが regression test で固定されていること。

---

### jj keyword を含む commit message の tokenization edge-case テスト (PR #267 post-merge-feedback T2-2 採用)

> **動機**: 順位 283 (anchor 修正) と表裏。283 の着手有無に関わらず、現行挙動 (既知の限界) を regression test で明示的に固定する価値が独立して残る。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 2 #2、`src/hooks-post-tool-jj-op-verify/src/main.rs` の tests module
>
> **実行優先度**: 🔧 Tier 2 — Effort S。283 と同一 PR での消化が効率的。

#### 作業計画

- [ ] `token_detection_ignores_jj_in_message_quotes` 等の edge-case テスト追加 (283 実施後は新挙動を固定)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- tokenization の既知の限界/修正後挙動がテストで明文化されていること。

---

### config path 解決の cwd 跨ぎ integration test (PR #267 post-merge-feedback T2-3 採用)

> **動機**: PR #267 で FIXED 済の `SIM-NEW-jjopverify-cwd-config-L179` は、既存テストが pure parser のみで file-lookup 経路を未カバーだったため混入した。非 repo-root cwd から hook を起動して config が読み込まれることを検証する統合テストは、cwd drift シナリオ (ADR-045 の核心リスク) の re-incident 検知網になる。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 2 #3、`hooks-post-tool-jj-op-verify` test suite。Adoption Risk: OS 依存 (temp dir / path 形式)
>
> **実行優先度**: 🔧 Tier 2 — Severity High / Effort M。

#### 作業計画

- [ ] 実 exe spawn + 非 repo-root cwd で config 読込を assert する `#[ignore]` integration test
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- exe-relative 解決の退行が統合テストで検出されること。

---

### 「config 読み hook は exe-relative 解決必須」convention の明文化 (PR #267 post-merge-feedback T3-1 採用)

> **動機**: 順位 281 (lint rule) の文書層の補完。ADR-045 (または dev-conventions) と該当 hook の inline comment に規約として明文化する。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 3 #1。**順位 281 と同一 PR での bundle 実装を推奨** (別作業に切り出す価値は低い)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] 順位 281 の PR に同乗して convention を明文化
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 新規 hook 作成時に参照できる規約が存在し、lint rule (281) と 2 層で防御されていること。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo10.md / todo9.md 末尾を参照。)
