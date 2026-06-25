# ファイルサイズチェックフロー改善 (Phase 1-2)

> **状態**: 試験運用 (本ドキュメントは "計画書" であり、PR-W0 〜 PR-W5 が全て land + dogfood 完了で **本ファイルを削除** して役割を終える)
>
> **目的**: 800 行超 file の現状蓄積 (PR-3a 時点で 7 件) を解消し、再発防止の機構 (weekly audit + Stop hook gate) を導入する
>
> **削除条件**: 以下 3 条件を全て満たす
>
> 1. PR-W0 〜 PR-W5 が全て master に land 済
> 2. `find src -name "*.rs" -not -path "*/target/*" -exec wc -l {} + | awk '$1 > 800'` で 0 件
> 3. PR-W5 land 後の Stop hook gate dogfood で `FILE_LENGTH_CHECK_OVERRIDE=1` を使わず 1-2 セッション通過

## 背景

- PR-3a (refactor PR、merged #217、2026-06-23) で 3 hook (hooks-session-start / hooks-pre-tool-validate / hooks-post-tool-linter) を module 分割した際、ratchet が触発されて初めて行数制限違反が顕在化
- 調査の結果、800 行超 file が **計 7 件** 存在 (lint hook 本体を含む) ことが判明:

  | 行数 | ファイル |
  |---|---|
  | 1606 | `src/hooks-post-tool-comment-lint-rust/src/main.rs` (lint hook 本体、self-host irony) |
  | 1432 | `src/cli-merge-pipeline/src/feedback.rs` |
  | 1404 | `src/cli-pr-monitor/src/stages/poll/mod.rs` |
  | 982 | `src/cli-push-runner/src/stages/lint_screen.rs` |
  | 972 | `src/cli-pr-monitor/src/fix_commit.rs` |
  | 946 | `src/cli-push-runner/src/config.rs` |
  | 890 | `src/cli-merge-pipeline/src/main.rs` |

- 現状の file_length lint (順位 147、PR #202 land) は `additionalContext` の soft-nag のみで `decision: block` を使わない設計のため、AI / 人が警告を無視して進められる構造
- Stop hook quality_gate / pre-push quality_gate のどちらにも file_length check は含まれない

## 設計方針 (2026-06-23 ユーザー判断)

| 段階 | 対応 | 強度 |
|---|---|---|
| 現状 | PostToolUse soft-nag のみ | 弱 |
| Phase 1 | 7 file を 800 行以下に分割 | (clean state 確立) |
| Phase 2 | E (weekly audit) + C (Stop hook gate) を追加 | 中 |
| (将来) Phase 3 | C → B (PostToolUse block) 移行を検討 | 強 |

**B 化の根拠 (将来検討)**: 1 件のファイル変更時点で 800 行を超えないと成り立たないファイルが作られた時点で、十中八九設計内容に不備があるか単一責任の原則に反している可能性が高い。Phase 3 では設計時の早期 signal として強制する。

---

## Phase 1 各 PR 共通の前提

W1 〜 W4 (file split PR) は全て同じ制約・push 手順・検証 procedure に従う。本 section は各 PR section の重複を避けるため共通項目を集約。

### 制約条件 (Write 時の遵守事項)

#### `// foo` 非 doc コメントは Write 時に削除

`hooks-post-tool-comment-lint-rust` の Bundle Z #B-α rule により、関数 body 内の `// foo` 形式の非 doc コメントは禁止 (Stop hook quality gate で block される)。

**許可されるコメント**:

- `///` doc コメント (関数 / 構造体)
- `//!` module doc コメント (ファイル頭)
- `// SAFETY:` / `// NOTE:` プレフィックス付き marker コメント

**禁止される pre-existing コメント例** (Write tool で新規 file に書き込むと violation 化):

```rust
fn main() {
    // stdin から JSON を読み取り       ← 禁止
    let mut input = String::new();
    // CLAUDE_ENV_FILE はセッションごとに異なるため  ← 禁止
}
```

→ 移動時に **削除**。意図は関数名 / 識別子名 / 関数分割で表現する (`read_stdin_hook_input()` 等の名前で読み取り意図を表現)。

PR-3a (#217) で 16 violations が同時発生した失敗事例あり (file 移動時に pre-existing コメントを carry over → 全 violation → re-Write で削除)。

#### 関数長 50 行制約 (順位 48)

分割時に新 helper 関数が 50 行を超えていないか確認。PR-3a で `default_pipelines()` を `default_ts_pipeline()` + `default_py_pipeline()` に分割した事例あり。

#### test helper は per-module duplicate

`unique_temp_root` / `write_meta` / `parked_state` 等の test helper は共有 module を抽出せず、各 test module に独立 copy する (memory `feedback_test_dry_antipattern.md` per)。共有 test util module は anti-pattern。

#### Cross-module visibility は `pub(crate)`

別 module から参照する struct / function は `pub(crate)` を付与。`pub` は crate 外公開を意味するため使用しない (binary crate では効果なし、library crate なら API contract 化)。

### push 手順

各 split PR の `jj diff -r 'master..@'` は ~15K 行規模 (削除 + 追加の合計) のため、push-runner の `pr_size_check` stage (block_threshold = 1500、順位 151) で block される。

mechanical refactor のため override env で通す:

```bash
PR_SIZE_CHECK_OVERRIDE=1 pnpm push
```

順位 151 の override 想定 use case (「大型 refactoring 等で意図的なら」) に該当。PR description で「**mechanical refactor、behavior 不変、test count 不変**」を明記すれば pre-push self-review との衝突は出ない (PR-3a #217 で実証、approved 状態で land)。

### takt-fix iteration 中の commit ハンドリング

post-pr-review iteration で takt が修正 commit を生成する場合、jj auto-snapshot により新 commit が @ に発生するが **description 未設定** の状態:

```text
Working copy  (@) : abc123 def4567 (no description set)
Parent commit (@-): <元の PR commit>
```

新セッションが引き継ぐ場合の手順:

1. `jj diff -r @` で内容確認
2. `jj describe -m "fix(...): takt-fix iter N — <内容>"` で description 付与
3. `jj bookmark move <bookmark> --to @` で bookmark 進める
4. `PR_SIZE_CHECK_OVERRIDE=1 pnpm push` で再 push

PR-3a (#217) で実際に 3 iterations 発生。iter 2 で local helper 抽出 (`spawn_stdout_drainer` 等)、iter 3 で `lib-subprocess` への統合まで自動進化した。

### 検証コマンド (PR ごと)

完了確認の標準コマンド集:

```bash
# 1. 全 file が 800 行以下
wc -l src/<crate>/src/*.rs src/<crate>/src/**/*.rs 2>/dev/null | sort -rn | head -10

# 2. test 全 pass (count 不変)
cargo test -p <crate> 2>&1 | grep -E "^test result:"

# 3. clippy clean (crate 単独)
cargo clippy -p <crate> -- -D warnings

# 4. workspace 全体に regression なし
cargo test --workspace 2>&1 | grep -E "^test result:" | wc -l
cargo clippy --workspace -- -D warnings
```

#### PR ごとの crate 名と元 test count 参照値

| PR | `<crate>` | 元 file (master = PR-3a 後) | 元 test count |
|---|---|---|---|
| W1 | `hooks-post-tool-comment-lint-rust` | `src/.../src/main.rs` (1606 行) | `cargo test -p hooks-post-tool-comment-lint-rust 2>&1 \| grep "^test result:"` で master HEAD 取得 |
| W2 | `cli-pr-monitor` | `poll/mod.rs` (1404) + `fix_commit.rs` (972) | 同上 |
| W3 | `cli-merge-pipeline` | `feedback.rs` (1432) + `main.rs` (890) | 同上 |
| W4 | `cli-push-runner` | `stages/lint_screen.rs` (982) + `config.rs` (946) | 同上 |

PR-3a の crate 別 baseline (参考、merged commit `862eb1e3`):

- hooks-session-start: 71 tests
- hooks-pre-tool-validate: 221 tests
- hooks-post-tool-linter: 145 tests

W1-W4 では各 crate の baseline を **作業開始時に master HEAD で測定** し、refactor 後と数値を一致させる。

### CR review 対応の Convention

post-pr-review (CodeRabbit) は本リポジトリ convention と異なる提案を出すことがある (= 偽陽性)。明確な convention 違反は **reject reply** で resolution する:

```text
resolved: 却下 — <reason>
```

`resolved:` prefix で auto-resolve トリガー (memory `project_coderabbit_auto_resolve`)。

#### auto-resolve 不発時の手動 resolve

CR が `<review_comment_withdrawn>` marker を返したのに thread state が更新されない場合は GraphQL mutation で手動 resolve:

```bash
THREAD_ID=$(gh api graphql -f query='query { repository(owner:"aloekun", name:"claude-code-hook-test") { pullRequest(number:<PR>) { reviewThreads(first:20) { nodes { id isResolved } } } } }' --jq '.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved == false) | .id')
gh api graphql -f query="mutation { resolveReviewThread(input: {threadId: \"$THREAD_ID\"}) { thread { id isResolved } } }"
```

#### よくある却下 pattern (本リポジトリ固有)

| CR 提案 | 却下根拠 |
|---|---|
| `path = "../lib-X"` → `workspace = true` | 却下: root `Cargo.toml` line 17 で workspace deps 採用を deferred、5 crate 全てが path 統一 (ADR-026) |
| `master` branch → `main` branch | 却下: 本リポジトリは master を default branch として運用 (各 hook config / ADR で確立) |
| docs lint hook 提案 (typo 検出等) | 却下: memory `feedback_no_doc_hook_lint.md` に明示的に抵触、週次レビュー (ADR-031) で対応する範囲 |
| `Stdio::piped()` を一律 block | 却下: 開発 friction 大、subprocess drain 実装済の正当な spawn も止まる |

### 各 PR の post-merge-feedback handling

各 PR の merge 後に post-merge-feedback skill が **自動発火** (ADR-029 / ADR-030)、`.claude/feedback-reports/<PR>.md` に report 生成。

新セッションが取るべき action:

1. report を確認 (✅ 採用候補 / 🤔 様子見 / ❌ 却下推奨 の 3 tier 分類が表示される)
2. **ユーザー確認なしに採用しない** (memory `feedback_post_merge_feedback_adoption_requires_user_approval.md` per)
3. ユーザー承認後に docs/todo*.md series に entry 追加 (順位採番 + 詳細 entry 両方)
4. 却下推奨は todo 登録不要だが、ユーザー承認は必要 (Claude 単独で却下処理しない)

参考実例: PR-3a (#217) では 9 findings → 2 採用 (T2-1 + T3-1) / 5 様子見 / 2 却下。W1-W4 も同様の比率になる見込み。

### 並列作業のシリアライズ point

複数セッションで W1 〜 W4 を並列実装する場合の調整事項:

#### Cargo.lock 競合

workspace 全体で 1 つの `Cargo.lock` のため、別 PR が先に merge されると後発 PR の `Cargo.lock` が rebase 必要。

**対処手順**:

```bash
jj git fetch
jj rebase -d master
cargo build --workspace   # Cargo.lock 再生成
jj describe -m "<元の commit message>"   # rebase 後の commit に再 describe
PR_SIZE_CHECK_OVERRIDE=1 pnpm push
```

#### Ordering rule

- 先に `jj bookmark create` + `pnpm push` した PR が **優先**
- 後発は merge 後に `jj rebase -d master` で master を取り込む
- 並列開始時に本 file の各 PR-W<N> section の `status` 欄を `[in progress] @<session-id>` に更新
- land 後に `[x] PR-W<N> (#<num>)` に変更

#### bookmark 命名規約

`pr-w<N>-<descriptor>` 形式を推奨 (例: `pr-w1-comment-lint-split` / `pr-w2-pr-monitor-split`)。PR-3a (#217) の bookmark `pr3a-hooks-module-split` を踏襲。

---

## 作業計画

### PR-W0: Weekly audit (E) を ADR-031 workflow に追加

- **status**: ✅ land 済 (#219, merged 2026-06-24T16:07:42Z)
- **owner**: -
- **effort**: XS
- **依存**: なし (最序盤に land 推奨、進捗 dashboard として機能)

#### スコープ

ADR-031 weekly-review workflow に deterministic Rust pre-step として file_length scan を追加。LLM facet 不要、純機械測定。

#### 実装内容

- `.takt/workflows/weekly-review.yaml` に新 step を追加 (LLM facet の前段に配置)
- shell command: `find src -name '*.rs' -not -path '*/target/*' -exec wc -l {} + | awk '$1 > 800 { print }' | sort -rn`
- 出力を `aggregate-weekly` facet の input に注入、weekly report に "file_length watchlist" section として記載
- severity = warning (block しない、健康診断目的)

#### 完了基準

- 次回 `/weekly-review` で 800 行超 file 一覧が weekly report に出力される
- Phase 1 の各 split PR が land すると次回 weekly で件数減少を確認可能

---

### PR-W1: hooks-post-tool-comment-lint-rust 分割 (1606 行)

- **status**: ✅ land 済 (#220, merged 2026-06-24T18:04:56Z)
- **owner**: -
- **effort**: M
- **依存**: PR-W0 land 後推奨 (進捗 visualize のため)

#### スコープ

[`src/hooks-post-tool-comment-lint-rust/src/main.rs`](../src/hooks-post-tool-comment-lint-rust/src/main.rs) を module 分割。lint hook **本体** が自分自身のルールに違反している self-host irony を最優先で解消。

#### 分割候補 (~5-6 modules、暫定)

| module | 想定行数 | 内容 |
|---|---|---|
| `main.rs` | ~150 | `HookInput` + `main` + dispatch |
| `violations.rs` | ~100 | `LintViolation` / `ViolationLocation` / `ViolationFix` / `ViolationExample` + `emit_feedback` |
| `comment_lint.rs` | ~400 | コメント検出ロジック (Bundle Z #B-α、順位 48 関連) |
| `function_length.rs` | ~150 | 関数長 50 行 check (順位 48) |
| `file_length.rs` | ~100 | ファイル長 800 行 check (順位 147) |
| `line_filter.rs` | ~150 | Edit/Write の line range 解釈、touch-trigger ratchet |

(test は各 module に co-locate、helper は per-module duplicate per `feedback_test_dry_antipattern`)

#### 実績 (land 済、#220)

実際の分割は **7 module** (候補表の 5-6 から `metrics.rs` を追加分離):

| module | 行数 |
|---|---|
| `comment_lint.rs` | 429 |
| `line_filter.rs` | 379 |
| `metrics.rs` | 286 |
| `main.rs` | 197 |
| `file_length.rs` | 197 |
| `function_length.rs` | 183 |
| `violations.rs` | 38 |

全 file ≤ 800 行 (最大 429)。merge 後 master で `find src/hooks-post-tool-comment-lint-rust/src -name '*.rs' -exec wc -l {} +` により検証済 (test pass / clippy clean は #220 の CI で確認)。

#### 完了基準

- 全 file ≤ 800 行
- `cargo test -p hooks-post-tool-comment-lint-rust` 全 pass (count 不変)
- `cargo clippy -p hooks-post-tool-comment-lint-rust -- -D warnings` clean
- lint hook 自身が self-consistent (PR-W5 の Stop gate 投入後に self-host で証明可能)

#### 進め方

Agent 委譲 (general-purpose) を活用、PR-3a の hooks-session-start 分割 (1611 → 7 modules、71 tests pass) と同型 procedure。

---

### PR-W2: cli-pr-monitor 分割 (2 file、計 2376 行)

- **status**: not started
- **owner**: -
- **effort**: M
- **依存**: PR-W1 と並列可 (別 crate、merge conflict リスク低)

#### スコープ

- [`src/cli-pr-monitor/src/stages/poll/mod.rs`](../src/cli-pr-monitor/src/stages/poll/mod.rs) (1404 行)
- [`src/cli-pr-monitor/src/fix_commit.rs`](../src/cli-pr-monitor/src/fix_commit.rs) (972 行)

`stages/poll/` は既に sub-module 化されているが `mod.rs` 自身が 1404 行。さらに sub-split が必要 (例: `poll/state_handlers.rs` / `poll/transitions.rs` 等)。

#### 完了基準

- 両 file が 800 行以下に分割
- `cargo test -p cli-pr-monitor` 全 pass (count 不変)
- `cargo clippy -p cli-pr-monitor -- -D warnings` clean

#### 進め方

Agent 委譲。ADR-018 (cli-pr-monitor の takt 移行) を参照させる必要あり。

---

### PR-W3: cli-merge-pipeline 分割 (2 file、計 2322 行)

- **status**: not started
- **owner**: -
- **effort**: M
- **依存**: PR-W1 / W2 と並列可

#### スコープ

- [`src/cli-merge-pipeline/src/feedback.rs`](../src/cli-merge-pipeline/src/feedback.rs) (1432 行) — post-merge-feedback flow 中核 (ADR-029 / 030)
- [`src/cli-merge-pipeline/src/main.rs`](../src/cli-merge-pipeline/src/main.rs) (890 行)

`feedback.rs` は ADR-029 / 030 の決定論的 post-merge-feedback flow を担うため、Agent 委譲時に **両 ADR を参照させる**。

#### 完了基準

- 両 file が 800 行以下に分割
- `cargo test -p cli-merge-pipeline` 全 pass (count 不変)
- `cargo clippy -p cli-merge-pipeline -- -D warnings` clean
- post-merge-feedback の 3 層分離原則 (機械 / takt / ask) を refactor 後も保持

---

### PR-W4: cli-push-runner 分割 (2 file、計 1928 行)

- **status**: not started
- **owner**: -
- **effort**: M
- **依存**: PR-W1 / W2 / W3 と並列可

#### スコープ

- [`src/cli-push-runner/src/stages/lint_screen.rs`](../src/cli-push-runner/src/stages/lint_screen.rs) (982 行) — ADR-038 試験運用 (local LLM lint screen)
- [`src/cli-push-runner/src/config.rs`](../src/cli-push-runner/src/config.rs) (946 行) — 各 stage の config struct 集約

`config.rs` は struct 集約のため、stage 別 module (`config/lint_screen.rs` / `config/pr_size_check.rs` 等) への分割が素直。

#### 完了基準

- 両 file が 800 行以下に分割
- `cargo test -p cli-push-runner` 全 pass (count 不変)
- `cargo clippy -p cli-push-runner -- -D warnings` clean

---

### PR-W5: Stop hook gate (C) 追加

- **status**: not started
- **owner**: -
- **effort**: S
- **依存**: PR-W1 + W2 + W3 + W4 が **全て land 済** (clean state 必須、未 land 状態で C を入れると Stop が常に block)

#### スコープ

Stop hook quality_gate に file_length check を追加。Phase 1 完了後の clean state を恒久維持するための強制層。

#### 実装方針 (Option C-2 採用)

既存 `hooks-post-tool-comment-lint-rust` に **batch mode** を追加し、Stop hook step として呼ぶ。

- `--check-modified-files` flag を追加 (新 CLI mode)
- `jj log -r 'master..@' --name-only` で PR 範囲 file を取得 (working copy 含む)
- 各 `.rs` file の行数を count、800 超なら exit 1 + 詳細 stderr 出力
- override 機構: `FILE_LENGTH_CHECK_OVERRIDE=1` 環境変数で skip 可能 (順位 151 `pr_size_check` と同 pattern、emergency 用)
- `hooks-config.toml` `[stop_quality.steps]` に新 step を追加:

  ```toml
  [[stop_quality.steps]]
  name = "file-length"
  cmd = "./.claude/hooks-post-tool-comment-lint-rust.exe --check-modified-files"
  ```

#### 完了基準

- Stop hook が PR 範囲の `.rs` file の行数を check し、800 超で session 終了 block
- override env で skip 可能を確認 (kill-switch test)
- ADR-039 § 4 self-review checklist (config schema / default OFF / docs / kill-switch test の 4 点) を満たす
- dogfood: 意図的に 800 行超 file を作って block されることを確認 + override で通過することを確認

#### 進め方

batch mode 実装 (~50 行) + tests (~30 行) + config schema 更新。Agent 委譲不要、直接実装可能な規模。

---

## (Phase 3、将来検討) PR-W6: C → B 移行

- **status**: future, not in current scope
- **依存**: PR-W5 land + dogfood 1-2 ヶ月観測

### 移行判断条件

3 条件を全て満たす場合に B (PostToolUse block) 化を検討:

1. PR-W5 land 後、新規 800 行超 file の発生件数 = 0 が 1-2 ヶ月維持
2. Stop hook gate が override env で bypass されていない (実発火 0)
3. weekly audit (W0) で ratchet 違反が累積していない

### B 化の論点

- **Pros**: 1 件目の Edit で block = 設計不備の signal を最早期に検出 (ユーザー意図と一致)
- **Cons**: 大規模 refactor 時に Edit が止まる → override env 必要
- **判断材料**: Phase 1 で 7 file split を経験済なので、機械分割は「override env で 1 回だけ通す」運用が現実的と判明している

本検討は **本ドキュメント削除後に別 planning doc または ADR で起案** する。

---

## 進捗追跡

各 PR の status は 1 セッション内では更新可能、跨ぐ場合は本 file の status 欄を更新して残す。

```text
PR-W0  [x] #219 (merged at 2026-06-24T16:07:42Z)
PR-W1  [x] #220 (merged at 2026-06-24T18:04:56Z)
PR-W2  [ ] not started
PR-W3  [ ] not started
PR-W4  [ ] not started
PR-W5  [ ] not started
```

land 後は `[x]` + PR 番号を記入し、最終的に 6 件全て `[x]` で本 file を削除。

---

## 関連 ADR / memory / 順位

- 順位 147 (file_length lint、PR #202 land)
- 順位 151 (`pr_size_check` stage、override env pattern の precedent)
- 順位 215 (Defensive State Reset、layered config の前提知識)
- 順位 220 (subprocess stress test、weekly pipeline 拡張の precedent)
- ADR-031 (weekly-review pipeline、W0 の編集対象 workflow)
- ADR-039 (Experimental feature standard pattern、§ 4 self-review checklist が W5 の完了基準)
- memory `feedback_no_doc_hook_lint` (docs lint を hook 強制しない方針、本作業は code file 対象なので非衝突)
- memory `feedback_minimize_pr_count_during_rate_limit` (iteration cost 高期は scope creep 許容、本作業は 6 PR で構成)
- memory `feedback_test_dry_antipattern` (test helper は per-module duplicate、Agent 委譲時にも reminder 必要)
- memory `project_coderabbit_auto_resolve` (`resolved:` prefix reply の auto-resolve トリガー)
- memory `feedback_post_merge_feedback_adoption_requires_user_approval` (post-merge-feedback の採用判断はユーザー承認必須)
- PR-3a merge commit `862eb1e3` (master HEAD 時点、3 hook split の実例 diff 参照用)

---

## Appendix A: Agent 委譲 prompt template (W1-W4 共通)

PR-3a (#217) で hooks-pre-tool-validate / hooks-post-tool-linter の split で実証済の Agent prompt を base template として保存。新セッションが W1 〜 W4 を実装する際は **本 template を copy + 変数置換** して Agent を起動する (general-purpose agent、`run_in_background=true` 推奨)。

### Template 本体

`<VARIABLES>` は次の変数置換表 (W1-W4) で具体値に置換する:

````text
You are doing a mechanical file split refactor for `<TARGET_FILES>` (currently <CURRENT_LINES> lines total, exceeds the project's 800-line file_length lint limit).

## Context

This is PR-W<N> (file_length enforcement Phase 1) — a behavior-unchanged refactor. The project uses `cargo workspace` (ADR-026). The file_length lint is enforced by `hooks-post-tool-comment-lint-rust` and the project's `coding-style.md § File Organization` requires files ≤ 800 lines.

The crate (<CRATE_NAME>) is responsible for: <CRATE_PURPOSE>.

## Reference: hooks-session-start was successfully split in PR-3a (#217, merge commit 862eb1e3)

See `src/hooks-session-start/src/` for the established pattern (main.rs + 6 sub-modules). All sub-modules use `pub(crate)` for items shared across modules and `#[cfg(test)] mod tests` with co-located tests. Test helpers are duplicated per module (NOT extracted to shared util module — per `feedback_test_dry_antipattern.md`).

## Target module layout for <CRATE_NAME>

Read each <TARGET_FILES> carefully, then split into modules under `src/<CRATE_NAME>/src/`:

<MODULE_LAYOUT_SUGGESTION>

Adjust dynamically based on actual code structure. Target: every resulting file ≤ 800 lines.

## Test distribution

The original file(s) have ~<TEST_LINES> lines of tests at the bottom. Distribute each test to the module containing its target function. Each module gets a `#[cfg(test)] mod tests` block with `use super::*;`. Some tests may use helpers (`unique_temp_root` etc.) — DUPLICATE these helpers per module.

## Critical constraints

- **Behavior must not change**. No function signature changes, no field renames, no default value changes. Only mechanical moves + visibility adjustments.
- **No new non-doc comments** (`// foo`). The project's `comment-lint-rust` hook blocks non-doc comments. If pre-existing `// foo` comments are in function bodies, REMOVE them during the move. Look at `src/hooks-session-start/src/main.rs` for the cleanup precedent.
- **Use `pub(crate)`** for items needed across modules. Strictly module-private items stay private.
- **No file > 800 lines** including tests. If a planned module exceeds 800 lines, sub-split it.
- **Functions ≤ 50 lines** (順位 48 rule). If a helper exceeds, split into smaller helpers (PR-3a precedent: `default_pipelines()` → `default_ts_pipeline()` + `default_py_pipeline()`).

## Verification (run these and confirm clean)

After splitting:

1. `cargo build -p <CRATE_NAME>` — must succeed
2. `cargo test -p <CRATE_NAME>` — all existing tests must pass (count unchanged from master baseline)
3. `cargo clippy -p <CRATE_NAME> -- -D warnings` — must be clean
4. `cargo fmt -p <CRATE_NAME>` — must be clean
5. `wc -l src/<CRATE_NAME>/src/**/*.rs` — all files ≤ 800 lines

## Report back

When done, report:

- Final module structure (file names + line counts)
- Test count before vs after (must be equal)
- Confirmation of clippy clean + fmt clean + file_length compliance
- Any rule_test_coverage_check or similar meta-validation adjustments needed (hooks-post-tool-linter precedent: directory walk over `src/**/*.rs`)
- Any cross-module visibility issues encountered

Work in the actual repository (not a worktree). The user wants this committed as part of PR-W<N>.
````

### 変数置換表 (W1-W4)

| 変数 | W1 | W2 | W3 | W4 |
|---|---|---|---|---|
| `<N>` | 1 | 2 | 3 | 4 |
| `<TARGET_FILES>` | `src/hooks-post-tool-comment-lint-rust/src/main.rs` | `src/cli-pr-monitor/src/stages/poll/mod.rs` + `src/cli-pr-monitor/src/fix_commit.rs` | `src/cli-merge-pipeline/src/feedback.rs` + `src/cli-merge-pipeline/src/main.rs` | `src/cli-push-runner/src/stages/lint_screen.rs` + `src/cli-push-runner/src/config.rs` |
| `<CURRENT_LINES>` | 1606 | 1404 + 972 = 2376 | 1432 + 890 = 2322 | 982 + 946 = 1928 |
| `<CRATE_NAME>` | `hooks-post-tool-comment-lint-rust` | `cli-pr-monitor` | `cli-merge-pipeline` | `cli-push-runner` |
| `<CRATE_PURPOSE>` | comment-lint hook (function-body コメント検出 + 関数長 50 行 check + ファイル長 800 行 check の 3 lint group を提供する PostToolUse hook、Bundle Z #B-α) | PR monitoring (CI / CodeRabbit polling + takt-fix 連携 + state file management + park signal、ADR-018) | PR merge pipeline (gh pr merge + post-merge-feedback トリガー + .failed marker recovery、ADR-013/029/030) | push pipeline (quality gate / pr_size_check / lint screen / takt pre-push-review 連携、ADR-015) |
| `<MODULE_LAYOUT_SUGGESTION>` | 5-6 modules: `main.rs` (~150) / `violations.rs` (~100) / `comment_lint.rs` (~400) / `function_length.rs` (~150) / `file_length.rs` (~100) / `line_filter.rs` (~150) | `stages/poll/<sub-split>` (state handlers / transitions / output 等で sub-split) + `fix_commit/<sub-split>` (commit creation / abandonment 等で sub-split) | `feedback/<sub-split>` (takt invocation / report generation / pending file handling 等で sub-split) + `main.rs` 縮小 (entry + dispatch のみ) | `stages/lint_screen/<sub-split>` (LLM invocation / context building / response parsing 等で sub-split) + `config/<sub-split>` (stage 別 config struct 分離) |
| `<TEST_LINES>` (推定) | ~1000 | ~1500 | ~1200 | ~900 |

### 注意事項

- W3 (`cli-merge-pipeline`) は ADR-029 (post-merge-feedback 自動起動) / ADR-030 (決定論的 post-merge-feedback) を参照する必要あり、Agent prompt に ADR 参照を追加すること
- W4 (`cli-push-runner`) の `stages/lint_screen.rs` は ADR-038 (local LLM lint screen) 試験運用配下、Agent prompt に ADR-038 参照を追加すること
- W2 (`cli-pr-monitor`) の `stages/poll/mod.rs` は park signal / wakeup 関連の複雑な state machine を含むため、特に behavior 不変性の verification を強化すること (parked state transitions の test を入念に)
- 全 PR で `cargo test --workspace` を実行して **他 crate に regression が出ていない** ことを確認 (lib-subprocess 等を共有しているため)
