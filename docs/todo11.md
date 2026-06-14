# TODO (Part 11)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo9.md がファイルサイズ 75KB 超 (890 行) に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して、PR-specific follow-up entries (PR #174 以降の post-merge-feedback 採用 entry) を本ファイルに分離 (2026-06-06)。todo9.md には「既存ルール仕組み化バンドル + 週次レビュー拡張」themed entries が残る。todo.md / todo2.md 〜 todo10.md の既存エントリは引き続き有効、相互に独立。新セッションでは十三つすべてを確認すること (todo.md / todo2-12.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### Bundle 1 dogfood checklist 実行 — `__test.ps1` block + override env 確認 (PR #174 T2-#2 採用、ADR-039 bounded lifetime data point #1)

> **動機**: PR #174 で実装した `scratch_file_warning` stage は ADR-039 § 3 Bounded lifetime 準拠で「3-5 PR の dogfood 後に default-ON 昇格 or 却下を判定」する設計。PR #174 の PR body に未消化の dogfood checklist が残っており (`__test.ps1` を意図的に作って push し block 動作確認 / override env でバイパス確認)、これが ADR-039 bounded lifetime の初回データポイント。次の PR (Bundle 2 等) merge 前の前提条件として消化が必要。
>
> **本タスクの位置づけ**: PR #174 post-merge-feedback Tier 2 #2 採用 (Severity Low / Frequency Low / Effort XS / Adoption Risk None)。manual operation で完結、Bundle 1 自身の運用検証 + ADR-039 bounded lifetime 体系の初回稼働確認。
>
> **参照**: `.claude/feedback-reports/174.md` Tier 2 #2、PR #174 PR body の Test Plan unchecked items、`docs/adr/adr-039-experimental-feature-standard-pattern.md` § 3 Bounded lifetime、`src/cli-push-runner/src/stages/scratch_file_warning.rs` (`SCRATCH_FILE_WARNING_OVERRIDE` env)
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。手動 dogfood 1 セット、~10 分。

#### 設計決定 (案)

- 手順:
  1. ローカル working dir に `__test_dummy.ps1` (or `.txt`) を作成 (中身は無害な dummy)
  2. `jj describe -m "test: scratch hook dogfood"` 等で commit
  3. `pnpm push` を実行 → scratch_file_warning stage が block する (EXIT_SCRATCH_FILE_WARNING = 6) を確認
  4. `$env:SCRATCH_FILE_WARNING_OVERRIDE = "1"; pnpm push` で override → 通過確認
  5. dogfood 完了後、`__test_dummy.ps1` ファイル削除 + commit abandon で working dir clean
- 記録: dogfood 結果 (block message / override 動作 / false positive 有無) を Bundle 2 PR body に「ADR-039 bounded lifetime data point #1」として記載
- 注意: 本 dogfood は本リポジトリで実施。派生プロジェクトへの deploy 後の dogfood は別タスク (派生プロジェクト側の bounded lifetime data point として記録)

#### 作業計画

- [ ] `__test_dummy.ps1` を working dir に作成
- [ ] `jj describe + pnpm push` で block 動作確認
- [ ] `$env:SCRATCH_FILE_WARNING_OVERRIDE = "1"; pnpm push` で override 動作確認
- [ ] cleanup: `__test_dummy.ps1` 削除 + commit abandon
- [ ] 結果を Bundle 2 PR body に記録
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- block 動作: scratch_file_warning stage が `__test_dummy.ps1` を検出し EXIT 6 で push を block する
- override 動作: env var 設定後に同 stage を通過、push が成功する
- ADR-039 bounded lifetime data point #1 が記録される

#### 詰まっている箇所

なし。Effort XS、manual operation で完結。

---

### docs-governance.md に「ADR multi-variant pattern section 追加時の checklist」を codify (PR #176 T3-#1 採用)

> **動機**: PR #175 (Minor: variant 網羅性不足) + PR #176 (Nitpick: 擬似コード vs 実コード齟齬) の 2 連続観測で、ADR の multi-variant pattern section を追加する際の「参照実装リスト完全性」「実装コード例の表記精度」取りこぼしが pattern 化された。本 PR #176 で追加した ADR-041 § State Preservation Invariant section が CR Nitpick を受けた事例も同パターン。Frequency Medium (2 観測) + Effort XS で採用条件成立。
>
> **本タスクの位置づけ**: PR #176 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。`~/.claude/rules/common/docs-governance.md` に 5-8 行 checklist を追記、ADR 拡張 PR の reviewer / Claude が逆引きで参照できる reusable rule に昇格。`feedback_no_unenforced_rules.md` 例外 = 2 PR で実証 + ADR 形式 (= 設計判断 doc) への追加で機械強制不要、reviewer の judgment 補助。
>
> **参照**: `.claude/feedback-reports/176.md` Tier 3 #1、PR #175 CR Minor finding 1 件、PR #176 CR Nitpick 1 件、`~/.claude/rules/common/docs-governance.md` (global rule、本リポジトリ外)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule への 5-8 行追記、本リポジトリ外 (`~/.claude/`) ファイル編集。

#### 設計決定 (案)

- **配置**: `~/.claude/rules/common/docs-governance.md` の document lifecycle classification 周辺、もしくは新 section "ADR Multi-Variant Pattern Authoring Checklist"
- **追記内容案** (5-8 行 checklist):
  - ADR に multi-variant pattern (variant 1/2/3 等の列挙) section を追加する場合:
    1. **参照実装リストの完全性**: 各 variant に対応する参照実装 (test 関数 or 実装関数) を 1 件以上 cite。variant が言及されているのに参照実装が無い (例: variant 2 だけ書いて test が無い) ことを避ける
    2. **実装コード例の表記精度**: コード例が擬似コード (簡略化) か実コード (literal copy) かを明示。擬似コードなら「(概念)」「(簡略化)」等のマーカーを付け、実コードならパスと行番号を cite (`poll.rs:839-842` 等)
    3. **既存資料との関係**: 該当 ADR の「既存資料との関係」section に cross-link を追加
  - 由来: PR #175 (variant 網羅性不足、Minor) + PR #176 (擬似コード vs 実コード齟齬、Nitpick) の 2 連続観測
- **派生プロジェクト transferability**: global rule のため本リポジトリで合意した内容は派生プロジェクトにも自動波及 (本 PR で `~/.claude/` 配下を直接編集する必要がある制約)

#### 作業計画

- [ ] memory `feedback_global_config_backup` 適用でバックアップ取得 (`~/.claude/rules/common/docs-governance.md` を `.backup-YYYYMMDD` 等で snapshot)
- [ ] `~/.claude/rules/common/docs-governance.md` に checklist 5-8 行を新 section "ADR Multi-Variant Pattern Authoring Checklist" として追記
- [ ] PR #175 / PR #176 を実例 cite として 1-line 引用
- [ ] markdownlint clean 確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `docs-governance.md` に ADR multi-variant pattern checklist が明文化される
- 将来の ADR 拡張 PR で variant 網羅性 + 表記精度の取りこぼしが reviewer 視点で防止される
- PR #175 / PR #176 が実例として reverse-lookup 可能

#### 詰まっている箇所

- 本タスクは `~/.claude/` 配下 (本リポジトリ外) のため、repo PR には含められない。実装は別途グローバル設定編集として実施
- バックアップ要 (memory `feedback_global_config_backup` 適用)

---

### Subprocess timeout+kill lifecycle 検証テスト追加 (PR #177 T2-#1 採用)

> **動機**: PR #177 で CR Major #2 「`run_jj_with_timeout` が timeout 後に jj 子プロセスを kill しない」を fix push したが、修正の正当性 (child process が timeout 到達時に確実に terminate される) を OS レベルで assert する回帰テストが現在ゼロ。fix は `spawn()` + `try_wait()` polling + timeout 時 `kill()` + `wait()` に書き換えたが、テストなしでは将来の変更で同型 leak 再導入が silent regression する。
>
> **本タスクの位置づけ**: PR #177 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Medium / Effort M / Adoption Risk None)。Major fix の回帰テスト + 今後の hook 実装で subprocess timeout pattern を使う際の reference test。Severity High = subprocess リーク (resource leak) は debug 困難な silent failure mode。Frequency Medium = 2 hook ファイル (hooks-session-start / hooks-pre-tool-validate) で同一 pattern 確認済、今後の hook 実装でも反復見込み。
>
> **参照**: `.claude/feedback-reports/177.md` Tier 2 #1、PR #177 CR Major finding (id 3309140888 hooks-session-start / 関連 fix in hooks-pre-tool-validate)、`src/hooks-session-start/src/main.rs` `run_jj_with_timeout` / `src/hooks-pre-tool-validate/src/main.rs` `run_jj_with_timeout` (両方が同一 pattern)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。両 hook test module で integration test 風の subprocess lifecycle 検証 (~80-120 行 + helper)。

#### 設計決定 (案)

- **対象 helper**: `run_jj_with_timeout` (両 hook で実装、ADR-024 で shared lib 統合候補)
- **検証内容**:
  1. **正常完了 case**: jj コマンドが timeout 内に完了 → output が返る、child は `try_wait` で reaped 済
  2. **timeout case**: 意図的に slow command (例: `jj log` で巨大 revset / 存在しない remote への `git fetch`) → timeout 到達 → kill 発火 → child が is_finished 状態に遷移していることを assert
  3. **kill 後の resource cleanup**: kill 後 `wait()` で zombie 化していないことを assert (Unix では `waitpid` で確認、Windows では `Child::id()` の OS handle が closed か)
- **テスト fixture**:
  - `Child::is_finished()` (Rust 1.18+) で kill 後の状態確認
  - `Command::new("sleep")` or `Command::new("cmd")` `/c "ping -n 100 127.0.0.1 > NUL"` (Windows) で意図的 slow command
  - timeout は短く (~500ms) して test 全体を 1-2 秒で完結
- **OS 依存性**: Windows / Linux 両対応のため `#[cfg(target_os = ...)]` で fixture を分ける、または `jj log` で確実に時間がかかる revset を使う方式に統一
- **配置**: 両 hook の `#[cfg(test)] mod tests` 内 + 共通 helper を `tests/common/mod.rs` 等に切り出す検討
- **memory `feedback_test_dry_antipattern.md`**: 各 test は独立 fixture で記述 (DRY 適用しない)

#### 作業計画

- [ ] `Child::is_finished` (or `wait_timeout`) で lifecycle 検証手段を確定
- [ ] hooks-session-start / hooks-pre-tool-validate の `run_jj_with_timeout` test module に 3 case 追加
- [ ] OS 依存 fixture (slow command) を Windows / Linux で動作確認
- [ ] dogfood: 意図的に timeout を踏ませる test を CI で安定して走らせられるか確認 (flaky test 回避)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 両 hook の `run_jj_with_timeout` で timeout 後の child kill + cleanup が OS レベルで検証される
- 同型 leak の silent regression が future PR で検出可能
- ADR-024 (shared jj helpers library) 統合時に test も統合対象として再評価可能な構造

#### 詰まっている箇所

- OS 依存性: Windows の subprocess lifecycle API (`is_finished`) と Linux の `waitpid` で挙動差異あり。`Child::is_finished` (stable 1.78+) が両 OS 対応で推奨
- flaky test 回避: timeout を踏ませる test は CI 環境の jitter で flaky 化リスク、500ms ~ 1s の余裕を持つ調整必要

---

### fail-closed error path (Option::None) 個別テスト追加 (PR #177 T2-#2 採用)

> **動機**: PR #177 の CR Major #1 「`check_todo_staleness` / `build_todo_staleness_message` が `behind.unwrap_or(0) > 0` で None を non-stale 扱いし fail-closed をバイパス」については現状コード (`src/hooks-pre-tool-validate/src/main.rs:796, 846-849`) で `check_todo_staleness` 側が依然 `behind.unwrap_or(0) > 0` のまま gate バイパスの可能性が残り、`build_todo_staleness_message` 側は `if behind.is_none() { return None; }` で early return しているが回帰テスト不在。本タスクは **実装側 fix (unwrap_or → map_or(true, ...) への修正)** + **回帰テスト追加** の両方を scope に含める。security gate 関数 (Option 返値 + jj 呼び出し) の error path 検証は今後の hook でも反復必要。
>
> **本タスクの位置づけ**: PR #177 post-merge-feedback Tier 2 #2 採用 (Severity High / Frequency Medium / Effort S / Adoption Risk None)。Major fix の回帰テスト + security gate pattern の standard reference。Severity High = fail-closed バイパスは silent security 退化。Frequency Medium = security gate + Option return pattern は今後の hooks でも反復適用見込み。
>
> **参照**: `.claude/feedback-reports/177.md` Tier 2 #2、PR #177 CR Major finding (id 3309140878)、`src/hooks-pre-tool-validate/src/main.rs` の `check_todo_staleness` / `build_todo_staleness_message` / `count_commits_branch_ahead`
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。test module への追加 ~30-50 行、unit test で独立検証可能。

#### 設計決定 (案)

- **対象 function**: `check_todo_staleness` (fail-closed 判定)、`build_todo_staleness_message` (None ケース message 出力)
- **実装側 fix (本 PR で同時に land)**:
  - `check_todo_staleness` line 796: `behind.unwrap_or(0) > 0` → `behind.map_or(true, |n| n > 0)` (None を stale=true として fail-closed 化)
  - `build_todo_staleness_message` line 846-849: 現状 `if behind.is_none() { return None; }` で early return しているが、明示的な fail-closed message を返す形に変更検討 (caller が None を「メッセージ無し」と非 stale 解釈しないよう調整)
- **検証 case** (memory `feedback_test_dry_antipattern.md` 適用、各 variant 独立 fixture):
  1. **`check_todo_staleness_returns_stale_when_lineage_none`**: `count_commits_branch_ahead` mock で None を返すよう注入 → result.stale = true、message に「lineage 判定不能」を含む
  2. **`build_todo_staleness_message_none_behind_marks_stale`**: `behind = None` で msg を生成 → "fail-closed で block" 文言を含む
  3. **`check_todo_staleness_normal_paths_unchanged`**: behind = Some(0) / Some(3) で従来通り動作 (regression 防止)
- **mock 戦略**: `count_commits_branch_ahead` は jj 実行依存のため、function を引数で受け取る形に refactor or test 専用 stub を導入。簡易には `count_commits_branch_ahead` を `pub(crate)` で公開し、test で別ロジック (constant None / Some(n) を返す closure) を builder で渡す pattern
- **回帰検出**: 将来 `map_or(true, ...)` を `unwrap_or(0)` 等に戻す変更で test が failing する構造を確保
- **memory `feedback_test_dry_antipattern.md`**: 各 case は独立 setup (mock 値別)、共通 helper 化しない

#### 作業計画

- [ ] `check_todo_staleness` を mock 注入可能な形に minor refactor (or test 専用 stub 追加)
- [ ] 3 case の unit test 追加
- [ ] cargo test で pass 確認 + 意図的に fail-closed 削除して test が落ちることを手動検証
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `check_todo_staleness` / `build_todo_staleness_message` の None ケース挙動 (fail-closed) が unit test で independent 検証
- 将来 `map_or(true, ...)` を逆向きに変更した時に 1 test が落ちる構造
- security gate + Option return pattern の test reference として hook 実装者が参照可能

#### 詰まっている箇所

- mock 注入 vs 簡易 stub の trade-off: dependency injection で全 hook で reusable にするか、test 専用 closure で local 化するか。後者 (local stub) のが Effort S で確実
- function signature 変更の影響範囲: `check_todo_staleness` を refactor すると call site (main.rs handle_write_edit_tool) も追従必要。最小 diff 優先で stub closure 内 mock 推奨

---

### Cross-ref edge case test coverage 追加 (PR #179 T2-#1 採用)

> **動機**: PR #179 で cli-docs-lint の cross_ref validator を新規実装し push-runner quality_gate に統合したが、percent-encode (`%20` / `%23`)、GFM heading slug、relative path normalize (`../`) の各 variant が fixture テストで明示的に保護されていない。validator のロジック劣化を silent regression として放置するリスクがある。
>
> **本タスクの位置づけ**: PR #179 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort S / Adoption Risk None、2026-05-28 ユーザー承認)。cross_ref validator の edge case coverage 拡充による silent regression 防止。
>
> **参照**: `.claude/feedback-reports/179.md` Tier 2 #1、`src/cli-docs-lint/src/cross_ref.rs` (既存 9 tests に追加)、PR #179 (cli-docs-lint 本体 land)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。既存 tests と同 pattern で fixture 追加。

#### 設計決定 (案)

- **対象 edge case**:
  1. **percent-encode**: 日本語 file name の percent-encode (例: `%20` 空白、`%E3...` UTF-8) を含む link を resolve できるか
  2. **GFM heading slug**: heading anchor (`#section-with-spaces` 等) の小文字化 / 空白→`-` 変換が GFM 仕様に従うか
  3. **relative path normalize**: 多段 `../` を含む link (例: docs/ から 2 階層上 root → 別 path) を正しく resolve できるか (現状の base_dir.join + canonicalize 経路)
- **fixture pattern**: 既存 cross_ref.rs の `#[cfg(test)]` mod 内の tempdir + 動的 fixture 生成 pattern を踏襲
- **memory `feedback_test_dry_antipattern`**: 各 variant 独立 setup、共通 helper 化しない

> NOTE: 本 entry の編集時に edge case の link 例を Markdown link 形式 (角括弧 + 丸括弧) で書くと、cli-docs-lint の cross_ref validator が backtick 内 link も誤検出する (= 本 entry land 時に発覚した false positive)。validator 自体の backtick-aware 化も本 entry 着手時に検討余地あり (現状は description + 拡張子のみで回避)。

#### 作業計画

- [ ] `src/cli-docs-lint/src/cross_ref.rs` の `#[cfg(test)]` mod に 3 case の fixture test を追加
- [ ] cargo test で pass 確認 + 意図的に validator から正規化ロジックを抜いて test が落ちるか手動検証
- [ ] (任意) validator の backtick-aware 化 (inline code 内の link を無視) を本 entry に同梱検討
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3 edge case (percent-encode / GFM heading slug / relative path normalize) が unit test で independent 検証
- silent regression を test で 1 件以上検出できる構造
- 既存 9 tests と整合性を保つ

#### 詰まっている箇所

なし。Effort S、cli-docs-lint 内のみで完結。

---

### `pnpm create-pr` PR body truncation 回避を検証する e2e/integration test 追加 (PR #181 T2-#1 採用)

> **動機**: PR #134 + #181 で 2 回観測された `pnpm create-pr` (= `cli-pr-monitor.exe` の PR 作成モード) における PR body 切り詰め問題。複数 section・複数行の body を `--body "..."` で渡すと shell argument 解釈で改行が delimiter 処理されて body が途中で切れる silent UX 劣化が発生する。memory `feedback_pnpm_create_pr_body` で `--body-file <path>` workaround を採用済だが、回避策が正常動作することを担保する自動 regression gate が存在しない。
>
> **本タスクの位置づけ**: PR #181 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-05-29 ユーザー承認)。PR #134・#181 の 2 回観測で Medium frequency に昇格、`--body-file` workaround の regression gate として採用条件成立。
>
> **参照**: `.claude/feedback-reports/181.md` Tier 2 #1、memory `feedback_pnpm_create_pr_body`、`src/cli-pr-monitor/src/main.rs` (PR 作成モード本体)、`src/cli-pr-monitor/src/stages/` 周辺の `run_create_pr` 実装、PR #134 / #181 の create-pr 実行例
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。既存 cli-pr-monitor test infra の流用、shell argument truncation 境界の fixture 測定。

#### 設計決定 (案)

- **検証対象**: `pnpm create-pr -- --title "..." --body-file <path>` 経由で PR を作成した際、body 内容が source file と一致すること (truncation なし、改行保持)
- **境界測定**: PR body 文字数 (行数 / バイト数) を段階的に増やし、shell 直渡し `--body "..."` パスが切り詰める閾値と `--body-file` が切り詰めない閾値の境界を fixture で測定、regression gate として記録
- **test 方式**: `gh pr create` の dry-run option がないため、cli-pr-monitor の argv 組み立て層を unit test 対象にする (実 PR 作成は行わない)、または integration test で mock gh CLI を介して argv の最終 shape を assert
- **memory `feedback_test_dry_antipattern`**: 各 variant 独立 setup、共通 helper 化しない

#### 作業計画

- [ ] cli-pr-monitor の PR 作成モードで argv 組み立て層を関数化 (test 可能な shape に refactor、必要なら)
- [ ] `#[cfg(test)]` mod に 3 fixture を追加: (a) 短い single-line body、(b) 複数行 body 経由 `--body-file`、(c) 直接 `--body` を渡した場合の truncation 再現
- [ ] cargo test で pass 確認 + 既存 cli-pr-monitor test との独立性確認
- [ ] truncation 境界の測定結果を test コメントに記録 (将来の閾値変更時の reference)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `--body-file` 経由が複数行 body で truncation なしに動作することが unit test で保護
- shell 直渡し `--body "..."` で truncation が起こる境界が fixture で測定済
- silent regression を test で 1 件以上検出できる構造
- 既存 cli-pr-monitor test との独立性 (mock 設定の交差なし)

#### 詰まっている箇所

`gh pr create` 自体に dry-run option がないため、実 PR 作成を伴わない検証戦略を要設計 (argv 組み立て層の関数化 or mock gh CLI)。Effort S 想定だが test 戦略次第で M に膨らむ可能性あり。

#### 補足 (PR #182 T2-#2 採用候補との関係、2026-05-29 ユーザー判断)

- PR #182 post-merge-feedback の T2-#2 (`pnpm-create-pr-body-guard` hook の guard test 追加) は本 165 と test 層 scope が重複するため、独立 entry 化せず本 entry に集約。analyzer は「本 session で `pnpm-create-pr-body-guard` hook による mitigate 実施」と articulate したが、これは hallucination (本 session では `--body-file` workaround を使ったのみで guard hook は触れていない)
- 重要な supplementary fact: PR #134 post-merge-feedback で `pnpm-create-pr-body-guard` hook 追加が **✅ 採用判定されたが、実装は完了していない状態** (= unfulfilled adoption、`.claude/feedback-reports/134.md` Tier 1 #1 参照)。順位 152 (todo 削除時の事前 land 確認手順) と関連する process learning として記録
- 本 165 着手時に guard hook が実装済なら test 範囲を 2 層に拡張する:
  1. (本 entry の主旨) `--body-file` workaround が複数行 body で truncation なしに動作することを verify
  2. (拡張) guard hook が `pnpm create-pr -- ... --body "..."` を block して `--body-file` に誘導することを verify
- hook 未実装のまま本 165 を land する場合、guard 層の test は将来の guard 実装 follow-up entry に切り出す

---

### `git-workflow.md § Multi-PR chaining` を「1 PR 内 multi-commit + intent 明記」パターンに拡張 (PR #183 T3-#1 採用)

> **動機**: PR #119/#120/#121 + 本 PR #183 で **4 回観測された** multi-commit single-PR bundling パターンを `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining に codify する。現状の同 section は「複数 PR の分割」を扱うが、「1 PR 内で commit を分離する判断基準」「各 commit message での intent 明記の重要性」が未記載。reviewer (CodeRabbit / 人間) が PR diff を読む際、commit description 単位の intent が明確だと review 効率が向上する。Frequency High に到達したため Tier 3 codify 条件成立。
>
> **本タスクの位置づけ**: PR #183 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency High / Effort S / Adoption Risk None、2026-05-29 ユーザー承認)。`~/.claude/` global 配下のため派生プロジェクト (techbook-ledger / auto-review-fix-vc) へ自動波及。
>
> **参照**: `.claude/feedback-reports/183.md` Tier 3 #1、`~/.claude/rules/common/git-workflow.md` § Multi-PR chaining ベストプラクティス (既存 section、拡張対象)、観測 PR: #119/#120/#121/#183

#### 設計決定 (案)

`git-workflow.md § Multi-PR chaining ベストプラクティス` に以下を追記:

- **「1 PR 内の multi-commit 分離」の判断基準**: 異なる論理単位 (例: docs update + feature impl) は **commit を分けて 1 PR で land** することで、reviewer が論理単位ごとに review focus を切り替えられる
- **commit message の intent 明記**: 各 commit description は単独で「何を / なぜ」を理解できる形で記述。`docs(todo): X 採用` / `feat(takt): Y 実装` 等の Conventional Commits + intent suffix のパターンを推奨
- **典型例**: PR #181 (handoff doc + post-merge-feedback adoption の 2 commit)、PR #183 (Bundle CR-RL todo + A01 ADR fix の 2 commit) を実例として cite
- **single-commit vs multi-commit の境界**: 同一論理単位は 1 commit (例: 単一 facet の implementation + test)。**論理単位が異なる** ときに分離する (例: docs update commit + impl commit)

#### 作業計画

- [ ] `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining ベストプラクティス に新 sub-section 「1 PR 内 multi-commit の判断基準」を追加 (~10-15 行)
- [ ] PR #181 / #183 の commit 構成を実例として inline cite
- [ ] **`feedback_global_config_backup`** 適用: ~/.claude/* を触る前に snapshot 取得 (`cp -r ~/.claude ~/__claude-backup-YYYYMMDD`)
- [ ] markdownlint clean 確認
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への展開は別タスク
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/git-workflow.md` に「1 PR 内 multi-commit 分離」と「intent 明記」のガイドが codify される
- 将来の AI / 人間セッションで commit 分割判断と intent 記述が一貫した形で適用される
- markdownlint clean

#### 詰まっている箇所

なし。Effort S、既存 section への追記のみで scope 明確。

---

### `docs-governance.md` に「Operational reference vs Pointer reference」区別 section を追加 (PR #183 T3-#2 採用) ★ Bundle DG-RULES

> **動機**: PR #183 で A01 修正 (8 ADR の ephemeral todo 参照を permanent reference に置換) を実施する際、各 reference が以下のどちらに該当するか判定する作業が発生した:
> - **operational reference**: workflow / behavior が ephemeral artifact をどう扱うかを記述するもの (例: 「ADR-031 workflow が `docs/todo.md` に追記する」)。dead-pointer リスクなし、保持可能
> - **pointer reference**: 特定の section 名 / 順位 N / Phase A-F 等を指すもの (例: 「Phase A-F section を参照」)。dead-pointer リスクあり、置換必要
>
> 現状の `~/.claude/rules/common/docs-governance.md` § Cross-File Reference Lifecycle は「permanent → ephemeral 参照は dead-pointer 化する」を codify しているが、**「operational reference は除外」という重要な判定基準が未記載**。本 PR の修正で ADR-031 lines 79-302 の中で line 270 のみが真の pointer reference だった実例が示すように、operational reference を pointer と誤認すると過剰修正で workflow 記述自体を壊す可能性がある。
>
> **本タスクの位置づけ**: PR #183 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None、2026-05-29 ユーザー承認)。`~/.claude/` global 配下のため派生プロジェクトへ自動波及。Bundle DG-RULES (本 entry + 順位 172) で同 PR land 推奨。
>
> **参照**: `.claude/feedback-reports/183.md` Tier 3 #2、`~/.claude/rules/common/docs-governance.md` § Cross-File Reference Lifecycle (既存 section、拡張対象)、PR #183 の 8 ADR 修正 commit (実例として cite)

#### 設計決定 (案)

`docs-governance.md` § Cross-File Reference Lifecycle に新 sub-section「Operational vs Pointer Reference」を追加:

- **Operational reference の定義**: workflow / 仕様 / behavior が ephemeral artifact (todo.md 等) を「どう扱うか」を記述するもの。**保持可能**。dead-pointer 化しない理由 = ephemeral artifact の特定 entry を指していないため。
  - 例: 「skill `/weekly-review` は採用 finding を `docs/todo.md` の新セクションに追記する」(動作記述、section 名は workflow が生成するため stale 化しない)
  - 例: 「reviewer は `docs/todo.md` を作業計画ファイルとして扱う」(classification、特定 entry を指さない)
- **Pointer reference の定義**: 特定の section 名 / 順位 N / Phase A-F 等を指すもの。**dead-pointer 化リスクあり = 置換必要**。
  - 例: 「Phase B-F は `docs/todo.md` の section X を参照」(stale 化)
  - 例: 「順位 42 を読む」(entry 削除で dead pointer)
- **判定基準**: reference が指す対象が「現在存在する specific entry / section」なら pointer、「workflow が描く general behavior」なら operational
- **実例**: PR #183 の ADR-031 line 270 (pointer、置換) vs lines 79-302 内の workflow 記述 (operational、保持)。ADR-034 の 順位 N + PR # pair (PR # 側が permanent reference として fallback、ephemeral 単独参照ではない) も example として cite

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` § Cross-File Reference Lifecycle に新 sub-section 「Operational vs Pointer Reference」を追加 (~15-20 行)
- [ ] PR #183 の修正例を inline cite (8 ADR の修正と「operational reference として保持」の判断根拠)
- [ ] **`feedback_global_config_backup`** 適用: ~/.claude/* を触る前に snapshot 取得
- [ ] markdownlint clean 確認
- [ ] 順位 172 (memory 追加) と同 PR で land 推奨 (Bundle DG-RULES、docs/rule + memory の 2 層)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `~/.claude/rules/common/docs-governance.md` に operational vs pointer の区別が codify される
- 将来の reviewer / AI が ADR 修正時に過剰修正 (operational reference の誤置換) を回避できる
- 派生プロジェクトへの自動波及で一貫した判定基準が確立
- markdownlint clean

#### 詰まっている箇所

なし。Effort S、既存 section への sub-section 追加で scope 明確。

---

### CR ephemeral artifact Nitpick の統一 skip 基準を memory に codify (PR #183 T3-#3 採用) ★ Bundle DG-RULES

> **動機**: PR #183 で CodeRabbit が docs/todo9.md (= ephemeral artifact) 内の行番号参照 (`lines 1298-1370` 等) を Nitpick として指摘した。これは「行番号は将来 drift する」という general principle としては正しいが、**ephemeral artifact (todo entry) は完了時に削除される設計** のため、永続化を求めるルールを適用するのは over-engineering。本 PR では skip 判断したが、同パターンが構造的に recurring と予想される (CR は ephemeral artifact を permanent doc と同等に扱う傾向)。判断基準を memory entry に codify することで、将来のセッションで一貫した skip 判断が可能になる。
>
> **本タスクの位置づけ**: PR #183 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-05-29 ユーザー承認)。`~/.claude/projects/.../memory/` 配下のため**派生プロジェクトには波及しない** (本リポジトリ専用)。Bundle DG-RULES (順位 171 + 本 entry) で同 PR land 推奨。
>
> **参照**: `.claude/feedback-reports/183.md` Tier 3 #3、既存 memory `feedback_coderabbit_no_actionable_merge_signal.md` (補完関係)、PR #183 の Nitpick 2 件 (CR-N1: 順位 168 line 1298-1370 / CR-N2: 順位 169 line 64/185-186)

#### 設計決定 (案)

新 memory ファイル `feedback_coderabbit_ephemeral_nitpick.md` を作成:

- **rule 名**: `feedback_coderabbit_ephemeral_nitpick`
- **type**: feedback
- **description**: CR が ephemeral artifact (`docs/todo*.md` 等) 内の行番号参照を Nitpick (💤 Low value) として指摘した場合は skip 推奨
- **content**:
  - **why**: ephemeral artifact (todo entry) は完了時に削除される設計のため、永続化を求めるルール (line drift 防止 = symbol/section 参照推奨) の適用は over-engineering
  - **how to apply**: CR Nitpick が `docs/todo*.md` 系 ephemeral artifact に対する line/symbol drift を指摘した場合、skip + merge 判断を維持。既存 memory `feedback_coderabbit_no_actionable_merge_signal` の「Nitpick 💤 Low value は skip 推奨」の補完。entry 実装着手時には自然に symbol 参照に置き換わる流れになるため、todo entry レベルで先取り fix する価値は低い
  - **境界**: permanent artifact (ADR / coding-style.md 等) への同種指摘は通常通り対応する。判定基準 = 対象 file の lifecycle (ephemeral or permanent)。本 rule は ephemeral artifact 専用
  - **実例**: PR #183 の CR-N1 / CR-N2 (docs/todo9.md の行番号参照を skip した実例)

#### 作業計画

- [ ] `~/.claude/projects/E--work-claude-code-hook-test/memory/feedback_coderabbit_ephemeral_nitpick.md` を新規作成 (~30-50 行、frontmatter 含む)
- [ ] `~/.claude/projects/E--work-claude-code-hook-test/memory/MEMORY.md` index に 1 行追加 (各 entry が「タイトル + 1 行 hook」の MEMORY.md 規約に従い、新 memory `feedback_coderabbit_ephemeral_nitpick.md` への 1 行 link を追加)
- [ ] **`feedback_global_config_backup`** 適用: 念のため memory ディレクトリの snapshot 取得 (`cp -r ~/.claude/projects/.../memory ~/__memory-backup-YYYYMMDD`)
- [ ] markdownlint clean 確認 (memory ファイル + MEMORY.md の両方)
- [ ] 順位 171 (docs-governance.md 拡張) と同 PR で land 推奨 (Bundle DG-RULES、docs/rule + memory の 2 層補強)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 新 memory ファイル `feedback_coderabbit_ephemeral_nitpick.md` が作成される
- MEMORY.md index に登録される
- 将来のセッションで CR が ephemeral artifact 内 Nitpick を出した場合、本 rule から逆引き可能になる
- markdownlint clean

#### 詰まっている箇所

なし。Effort XS、新規 memory ファイル + index 1 行追加のみ。

---

### subprocess utils 5 crate 重複を `lib-subprocess` に extract (PR #182 dry-run S01 + Phase E dogfood WR-2026-06-01-S01 採用、2026-06-14 挙動保存型 sub-PR 分割)

> **動機**: PR #182 Phase B dry-run で検出された finding WR-2026-05-29-S01 に加え、**Phase E dogfood (2026-06-01) で WR-2026-06-01-S01 (High) として scope が拡大**: subprocess 管理 utility 群が 4-5 crate 横断で重複している。
>
> **重複対象**:
> - `combine_output(stdout, stderr)` 8 行関数: 5 crate (`cli-pr-monitor` / `cli-push-runner` / `cli-push-pipeline` / `cli-merge-pipeline` / `hooks-post-tool-linter`) で重複 (cli-pr-monitor では `#[allow(dead_code)]` 付与で生産 path 未到達)
> - `drain_pipe` / `wait_with_timeout` / `run_cmd`: 4-5 crate (`cli-push-runner/src/runner.rs` / `cli-pr-monitor/src/runner.rs` / `cli-merge-pipeline/src/main.rs` / `cli-push-pipeline/src/main.rs` / `hooks-stop-quality/src/main.rs`) で重複かつ挙動 variant あり
> - `MAX_LINES` 定数: 4 crate 間で不整合 (20 / 40 / 200 / 無制限) — callsite 設定可能パラメータとして公開する
>
> ADR-026 Cargo workspace + ADR-012 lib-* naming の既存パターンで解決コスト低、保守リスクが各 PR で蓄積中。
>
> **本タスクの位置づけ**: PR #182 dry-run S01 採用 (2026-05-29 ユーザー承認) + Phase E dogfood (2026-06-01) WR-2026-06-01-S01 (Severity High) で augment 採用。ADR-031 § Phase 4 「重複検出は MVP では実装しない」運用の partial overlap 検出 → augment 判断のフローが機能した実例 (skill 重複検出 → 3 択 → user augment 選択)。
>
> **参照**: PR #182 dry-run report、Phase E dogfood report (`.claude/weekly-reviews/2026-06-01.md` および `.takt/runs/20260601-095710-weekly-review-2026-06-01/reports/`)、`src/cli-pr-monitor/src/runner.rs:80-89` (function) + `:282-298` (tests)、`src/cli-push-runner/src/runner.rs` / `src/cli-merge-pipeline/src/main.rs` / `src/cli-push-pipeline/src/main.rs` (drain_pipe 等の重複)、`src/hooks-post-tool-linter/` (combine_output の `\n` suffix 吸収版実装)、ADR-024 (shared jj-helpers library パターン)、ADR-026 (Cargo workspace)
>
> **実行優先度**: 🔧 **Tier 2** → 🚀 **Tier 1 検討余地** — Effort S-M (sub-PR 分割で各 ~150-250 行)。Phase E dogfood で High severity 再確認、4-5 crate 横断で更新コスト線形成長中。

#### 挙動保存型 sub-PR 分割方針 (2026-06-14 ユーザー合意)

scope 確認の結果、`drain_pipe` / `wait_with_timeout` / `run_cmd` は **crate ごとに挙動 variant が存在** することが判明。1 PR で全部やると挙動統一の意思決定が混入して PR が肥大化 + リスク増大するため、以下原則で分割:

- **原則**: 挙動保存。`lib-subprocess` に複数 variant 関数を export し、各 callsite はこれまでと同じ挙動の variant を呼ぶだけにする
- **variant merge 検討は最終 sub-PR (173e) に分離**: 「実装上ほぼ同じ variant を 1 つに統合できるか」は dogfood 後判断
- ADR-024 (`lib-jj-helpers`) の異なる semantics の helper を別関数として export する pattern に倣う

#### sub-PR 一覧

| sub | 対象 | 推定 diff | リスク | 依存 |
|---|---|---|---|---|
| **173a** | `combine_output` 抽出 + `lib-subprocess` crate 新設 | ~150 行 | 低 (pure 関数、`hooks-post-tool-linter` の `\n` suffix 吸収版に統一しても既存 test 全 13 件 pass) | なし |
| 173b | `wait_with_timeout` 2 variant 抽出 | ~150 行 | 中 (subprocess lifecycle、integration test 望ましい) | 173a |
| 173c | `drain_pipe` 3 variant 抽出 | ~200 行 | 中 (API 形状変更、MAX_LINES の callsite parameter 化) | 173a |
| 173d | `run_cmd` variant 抽出 | ~250 行 | 中-高 (173a/b/c に依存) | 173a/b/c |
| 173e (optional) | variant merge 検討 (dogfood 後判断) | 〜 | 〜 | 173a-d 完了後 |

##### 173a: `combine_output` 抽出 (本 PR scope) ✅ 実装完了 (2026-06-14)

- **採用 variant**: `hooks-post-tool-linter` の `\n` suffix 吸収版 (4 crate の basic 版が既存 test で全 case pass するため後方互換)
- **抽出先**: `src/lib-subprocess/` 新規 crate (`lib-subprocess` 名称、ADR-026 workspace + ADR-012 lib-* naming に整合、既存 lib-jj-helpers / lib-ollama-client / lib-pending-file / lib-report-formatter のいずれも subprocess utility の自然な home ではないため新規)
- **作業計画**:
  - [x] `src/lib-subprocess/Cargo.toml` + `src/lib-subprocess/src/lib.rs` 作成 (`combine_output(stdout: &str, stderr: &str) -> String` pub 関数 + 5 variant test)
  - [x] ルート `Cargo.toml` workspace members に `src/lib-subprocess` 追加
  - [x] 4 crate (cli-push-runner / cli-push-pipeline / cli-merge-pipeline / hooks-post-tool-linter) の `Cargo.toml` に `lib-subprocess = { path = "../lib-subprocess" }` dep 追加。cli-pr-monitor は dead-code (`#[allow(dead_code)]`) のため関数 + test 削除のみで dep 不要
  - [x] 各 crate の `combine_output` impl + duplicate test 削除、`use lib_subprocess::combine_output;` に置換
  - [x] `cargo test --workspace` で全 pass 確認 (lib-subprocess 5 test + 5 crate 含む workspace 全体 pass)
  - [x] `cargo clippy --workspace -- -D warnings` production code clean (cli-pr-monitor の `#[allow(dead_code)]` 削除確認)。NOTE: `--all-targets` 付与時に `cli-merge-pipeline/src/feedback.rs` に PR #159 (Bundle l) 以来の事前 clippy 違反 2 件 (`items-after-test-module` / `assertions-on-constants`) が残存。本 PR の編集対象外のため別 entry として follow-up 候補
  - [x] 173a 完了マーク (本 sub section)、173b-e は本 entry 内に未着手として残置

##### 173b: `wait_with_timeout` 2 variant 抽出 ✅ 実装完了 (2026-06-14)

- **variant A `wait_with_timeout_safe`**: Err 経路で `child.kill()` + `child.wait()` 実施 — cli-pr-monitor (`runner.rs` 内 def → `stages/push_jj_bookmark.rs` で 1 callsite) で使用
- **variant B `wait_with_timeout_basic`**: Err 経路でそのまま `Err` 返却 — cli-push-runner (`runner.rs` 内部 1 callsite + `stages/` 5 callsite = 計 6 callsite) で使用
- variant 統一は本 sub では行わない (173e 判断)
- **scope 調整 (2026-06-14)**: 当初 plan で「cli-pr-monitor `classifier_runner.rs` (2 callsites)」が `_basic` を共用すると想定していたが、実コード確認の結果 channel-based + Child move + `wait_with_output()` 同時 drain の **completely 異なる設計** (signature: `fn(Child, Duration) -> Option<Output>`、pipe buffer overflow 対策の意図的構造) であることが判明。signature 互換性なし + 設計意図が独立のため本 sub では touch せず、必要なら 173e で別 variant として再評価
- **作業計画**:
  - [x] `lib-subprocess/src/lib.rs` に `wait_with_timeout_safe` / `wait_with_timeout_basic` 2 関数 + 各 variant 2 test (success/timeout) 追加
  - [x] cli-pr-monitor `Cargo.toml` に `lib-subprocess` dep 復活 (173a で removed)
  - [x] cli-pr-monitor `runner.rs` の `wait_with_timeout` impl + `use ExitStatus` 削除
  - [x] cli-pr-monitor `stages/push_jj_bookmark.rs:172` callsite を `lib_subprocess::wait_with_timeout_safe` に置換
  - [x] cli-push-runner `runner.rs` の `wait_with_timeout` impl + 1 test (`wait_with_timeout_returns_exit_status_directly`) + `use ExitStatus/Instant` 削除
  - [x] cli-push-runner `runner.rs` 内 1 callsite + `stages/{scratch_file_warning, push_jj_bookmark, pr_size_check, lint_screen, bookmark_check}.rs` 5 callsite を `lib_subprocess::wait_with_timeout_basic` に置換
  - [x] `cargo test --workspace` で全 pass 確認 (lib-subprocess 9 test + workspace 全体 pass)
  - [x] `cargo clippy --workspace -- -D warnings` clean
  - [x] 173b 完了マーク (本 sub section)、173c/d/e は本 entry 内に未着手として残置

##### 173c: `drain_pipe` 3 variant 抽出 ✅ 実装完了 (2026-06-14、CR Major fix 含む)

- **variant A `drain_pipe_unlimited`**: `read_to_string` 全読み — cli-pr-monitor (`runner.rs` 内部 2 callsites + `stages/push_jj_bookmark.rs` 2 callsites = 計 4 callsites)
- **variant B `drain_pipe_capped(pipe, max_lines)`**: `read_until` + silent truncate — cli-push-runner (`runner.rs` 2 callsites + `stages/{scratch_file_warning, push_jj_bookmark, pr_size_check, lint_screen, bookmark_check}.rs` 10 callsites = 計 12 callsites)、cli-push-pipeline (2 callsites)、hooks-stop-quality (2 callsites)
- **variant C `drain_pipe_capped_reporting(pipe, max_lines)`**: variant B + `"... (N lines truncated)"` 末尾報告 — cli-merge-pipeline (2 callsites、MAX_LINES=200)
- 計 22 callsites を 5 crate 横断で migration、`MAX_LINES` 定数は各 crate に保持して callsite で parameter として渡す形に変更 (挙動保存、crate 別の設計意図 = ログ用なら 40、メモリ保護なら 200、stop hook なら 20 を維持)
- **作業計画**:
  - [x] `lib-subprocess/src/lib.rs` に 3 variant + 6 test (各 variant の代表 case: 短/長/over-cap/under-cap/reporting on/off) 追加
  - [x] cli-pr-monitor `runner.rs` の `drain_pipe` impl + NOTE コメント削除、4 callsite を `lib_subprocess::drain_pipe_unlimited` に置換
  - [x] cli-push-runner `runner.rs` の `drain_pipe` impl 削除、`MAX_LINES` を `pub(crate) const` 化して stage から参照可能に、12 callsite を `drain_pipe_capped(pipe, MAX_LINES)` に置換
  - [x] cli-push-pipeline `main.rs` の `drain_pipe` impl 削除、2 callsite を `drain_pipe_capped(pipe, MAX_LINES)` に置換 (lib-subprocess は 173a で dep 追加済)
  - [x] hooks-stop-quality `Cargo.toml` に `lib-subprocess` dep 追加、`drain_pipe` impl 削除、2 callsite を `drain_pipe_capped(pipe, MAX_LINES)` に置換
  - [x] cli-merge-pipeline `main.rs` の `drain_pipe` impl 削除、2 callsite を `drain_pipe_capped_reporting(pipe, MAX_LINES)` に置換 (lib-subprocess は 173a で dep 追加済)
  - [x] `cargo test --workspace` で全 pass 確認 (lib-subprocess 15 test + 6 影響 crate 含む workspace 全体 pass)
  - [x] `cargo clippy --workspace -- -D warnings` clean
  - [x] 173c 完了マーク (本 sub section)、173d/e は本 entry 内に未着手として残置

##### 173d: `run_cmd` variant 抽出

- 173a/b/c に依存。`run_cmd` / `run_cmd_direct` / `run_cmd_inherit` 系の variant を別々に抽出
- 作業計画: 173a-c 完了後に詳細展開

##### 173e (optional): variant merge 検討

- 173a-d 完了後の dogfood 観測で「variant 共存に意味があるか/merge 可能か」を判断
- 例: `wait_with_timeout_safe` と `_basic` の 2 variant は「Err 経路で child を kill すべきか」の policy 違い。dogfood で `_basic` 側で zombie process 顕在化したら `_safe` に統合
- 判断時点まで延期 (= 現時点で merge 計画を立てない)

#### 完了基準 (本 entry 全体)

- 173a-d 全 sub-PR が land し、subprocess utility の単一 source of truth が `lib-subprocess` に確立
- 5 cli-*/hooks-* crate が `lib_subprocess::*` 経由で utility を共有
- `#[allow(dead_code)]` 付与が不要になる
- cargo workspace 全体で cargo test + cargo clippy が pass

#### 詰まっている箇所

なし。挙動保存型分割で各 sub-PR が独立着手可能。173a 着手中。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo9.md 末尾を参照。)
