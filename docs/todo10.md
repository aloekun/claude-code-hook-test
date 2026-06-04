# TODO (Part 10)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo9.md がファイルサイズ 50KB を超え行数 1100+ 行に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #185 = Bundle CR-RL land 後、2026-05-29 ユーザー判断)。todo.md / todo2.md 〜 todo9.md の既存エントリは引き続き有効、相互に独立。新セッションでは十一つすべてを確認すること (todo.md / todo2-10.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### check-ci-coderabbit format extraction 関数への variant fixture 追加 (PR #185 T2-#4 採用)

> **動機**: PR #185 (Bundle CR-RL) で `extract_old_format_wait_time` / `extract_new_format_wait_time` の 2 helper 関数に分離し 3 新規 fixture (full / minutes-only / 旧新混在) を追加したが、analyzer (post-merge-feedback) は **bold-wrapper variant** (例: `**More reviews will be available in N minutes and S seconds**`) や **その他の組合せ variant** の coverage gap を指摘。PR #182 (30+ 分 polling 浪費の実観測) + PR #185 (format 多様性対応) の 2 PR 連続観測で、CR の format は引き続き variants を生む可能性が高く、防御的 fixture coverage 追加が systemic 価値あり。
>
> **本タスクの位置づけ**: PR #185 post-merge-feedback Tier 2 #4 採用 (Severity High / Frequency Medium / Effort M / Adoption Risk None、2026-05-29 ユーザー承認)。順位 167-169 (Bundle CR-RL) の follow-up として next format drift での silent regression 防止網を厚くする。
>
> **参照**: `.claude/feedback-reports/185.md` Tier 2 #4、`src/check-ci-coderabbit/src/main.rs` の `#[cfg(test)]` mod (既存 9 fixture = 6 旧 format + 3 新 format)、`extract_old_format_wait_time` / `extract_new_format_wait_time` の regex (現状 markdown bold `\*?\*?` は旧 format のみ対応、新 format は bold 想定なし)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。fixture 追加のみで runtime 影響なし。
>
> **注意 (analyzer rationale の一部に弱点)**: post-merge-feedback report は「PR #185 で 6 回の Edit が同一ファイルに集中した incremental development パターンは test coverage gap の兆候」と述べているが、これは incidental development pattern であり test coverage の真の signal ではない。採用根拠は **CR format 多様性 (PR #182 + #185 の 2 PR 連続観測) + bold-wrapper 等の防御的 variant 追加の妥当性** であり、Edit 集中は無関係。

#### 設計決定 (案)

追加候補 fixture (memory `feedback_test_dry_antipattern`: 各 variant 独立 setup):

- **bold-wrapper 新 format**: `**More reviews will be available in 15 minutes and 30 seconds**` — CR が markdown bold を新 format に追加した場合の検出
  - 現 `extract_new_format_wait_time` の regex はこの場合 fail する (旧 format の `\*?\*?` 相当を新 format regex にも追加する必要あり)
  - もし fail を assertion で確認するなら fixture は「現状の振る舞いを pin」、もし regex を pre-emptively 拡張するなら「拡張後の動作 verify」
- **secs だけ provided 新 format**: `More reviews will be available in 45 seconds` (minutes 0 + seconds N の variant、CR が短時間 rate-limit を表現する場合)
- **複数 separator 旧 format**: `Please wait 5 minutes, 13 seconds` (`and` ではなく `,` 使用、観測例なしだが defensive)
- **HTML マーカーのみで wait time 文言なし**: `<!-- rate limited by coderabbit.ai --> ## Review limit reached` のみ → wait time 抽出失敗 = `parse_rate_limit` が None を返すことを assert (graceful failure verify)

#### 設計判断 (regex 拡張 vs fixture のみ追加)

2 つのアプローチ:

1. **regex 拡張先行**: `extract_new_format_wait_time` の regex に `\*?\*?` 等を pre-emptively 追加し、それを fixture で verify する (= 想定 variant への先回り対応)
2. **fixture 先行 + 観測後 regex 拡張**: 現状の regex で fixture を書き、bold-wrapper variant では fail することを assert (= 現状の振る舞いを pin、観測ベース対応に倣う)

どちらを採るかは本タスク着手時に判断。memory `feedback_no_unenforced_rules` の「未観測の preventive over-engineering を避ける」原則からは 2 が一貫性あり。1 を採るならその根拠 (=「regex 拡張は trivial で false positive リスクなし」) を commit description に明示する。

#### 作業計画

- [ ] 既存 9 fixture (`#[cfg(test)]` mod) を Read で全件確認、coverage gap を整理
- [ ] 追加 4 fixture を独立 `#[test]` 関数として追加 (helper 共通化なし、memory `feedback_test_dry_antipattern` 適用)
- [ ] `cargo test -p check-ci-coderabbit` で全 fixture pass を確認 (新 + 既存 backward compat 維持)
- [ ] regex 拡張アプローチを採る場合は `extract_*_format_wait_time` の regex を更新、対応する `--release` test 確認
- [ ] ADR-034 § 既知 CR rate-limit format 一覧 table に新 variant を 1 行 append (発見時期 = 「2026-05-29 防御的追加 (順位 176 land 時)」)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 4 新規 fixture が `cargo test -p check-ci-coderabbit` で全 pass
- bold-wrapper / 短形態 / graceful failure 等の variant coverage 確立
- silent regression を test で 1 件以上検出できる構造 (= regex を意図的に元に戻すと新 fixture test が落ちる)
- ADR-034 § 既知 format 一覧 table の append による永続 reference 整合

#### 詰まっている箇所

regex 拡張アプローチ (#1) vs fixture のみ追加 (#2) の選択。本タスク着手時に bold-wrapper の CR 実観測例が増えていれば #1、increase なしなら #2 を採る判断が memory `feedback_no_unenforced_rules` の原則に整合する。

---

### PostToolUse hook — Edit / Write したファイルのサイズ閾値超過を検出してファイル分割を促す (2026-05-29 ユーザー追加要望)

> **動機**: 本セッション (PR #181 → #182 → #183 → #184 → #185 chain) で **docs/todo9.md が 50KB 超 + 1168 行に到達し読み取り安定性に支障**、user 判断で docs/todo10.md に split した実体観測がある (本ファイル自身がその split 結果)。同型の問題はこれまでも todo.md → todo2.md (PR #133) / todo8.md → todo9.md (PR #172) で繰り返し発生しており、現状は user 判断ベースでファイル分割している。**PostToolUse hook で Edit / Write 直後にサイズチェックを自動化** し、閾値超過時にファイル分割を促す error feedback を出すことで、user が認知負荷で気づく前に mechanical layer で promote できる構造的改善。
>
> **本タスクの位置づけ**: PR #185 land 後の本セッション内 user 追加要望 (2026-05-29、post-merge-feedback 経由ではない直接タスク化)。memory `feedback_pipeline_over_rules` の体系適用 — user が「ファイル大きくなりすぎたら split する」を rule で覚えるのではなく、hook で機械強制する。touch-trigger ratchet pattern (= 既存超過ファイルは触られるまで grandfather) で backward compat 確保。
>
> **参照**: `src/hooks-post-tool-comment-lint-rust/` (PostToolUse hook 既存実装、関数長 50 行制限の touch-trigger ratchet 参考)、`src/hooks-post-tool-linter/` (汎用 linter hook 既存)、`.claude/hooks-config.toml` の `[post_tool_use]` config 構造、PR #133 (todo.md → todo2.md split) / PR #172 (todo8.md → todo9.md split) / 本セッション PR (todo9.md → todo10.md split) の 3 PR 観測
>
> **実行優先度**: 🚀 **Tier 1** — Effort S-M。`hooks-config.toml` への新 sub-feature 追加 + hook binary の Edit/Write 拡張で完結、touch-trigger ratchet で既存超過 grandfather。

#### 設計決定 (案)

##### 1. 配置先 (2 案、着手時判断)

- **option A**: 新 hook binary `hooks-post-tool-file-size-check` を新設。専用性高く責務分離明確、ADR-026 Cargo workspace の lib-* / hooks-* pattern に整合
- **option B**: 既存 `hooks-post-tool-linter` (generic linter) に新 check として統合。新 binary 追加せず Edit/Write 1 hook で済む、deploy 簡素

option B が Effort S 寄り、option A が将来拡張 (例: バイナリサイズ / generated ファイルサイズ等の別 check と分離) しやすい。

##### 2. config schema

`.claude/hooks-config.toml` に新 section:

```toml
# [post_tool_use.file_size_check]
# Edit / Write 直後にファイルサイズを確認し、threshold 超過なら error で
# split を促す。touch-trigger ratchet で既存超過ファイルは grandfather。
[post_tool_use.file_size_check]
enabled = false              # ADR-039 opt-in (default OFF、repo config で明示 enable)
threshold_bytes = 51200      # default 50KB (= 50 * 1024 bytes)
# 対象ファイル glob。default は markdown + Rust source。
paths = ["docs/**/*.md", "src/**/*.rs"]
# touch-trigger ratchet: true = 既存超過ファイルは触られるまで grandfather (= 触られたら即チェック)
# false = strict mode (= 触ったかどうかに関わらず全 enabled paths を毎回チェック)
touch_trigger = true
```

##### 3. 動作仕様

- PostToolUse Edit / Write 直後に発火
- 編集された file path が `paths` glob に match するか確認 (no match → skip)
- file size が `threshold_bytes` 超過か確認 (no 超過 → skip)
- 超過時の error 出力 (stderr JSON で hook protocol に整合):
  - error message: `"<file>: ファイルサイズ <N> bytes が threshold <M> bytes を超過しています。ファイル分割を推奨します。"`
  - recovery hint: `"docs/todo*.md の場合は新 todo<N+1>.md を新設、Rust source の場合は module 分割を検討。"`
  - kill-switch: `enabled = false` で完全停止 (ADR-039 § Kill-switch 整合、診断メッセージは実装の受理値を網羅する原則も適用)

##### 4. touch-trigger ratchet の意義

- 既存 `docs/todo.md` (~30KB) や `docs/todo8.md` (~50KB 弱) など、本 hook 導入時に閾値近辺のファイルが存在する
- `touch_trigger = true` (default) なら未編集ファイルは grandfather、編集した瞬間にチェックが発火 = 「触ったら直す」原則
- `touch_trigger = false` (strict) は全 enabled paths を毎回 fail する可能性 = 導入直後に大量 error を生む、適用は dogfood 後に判断

#### 作業計画

- [ ] 配置先選定 (option A = 新 binary vs option B = 既存 linter 統合) を `src/hooks-post-tool-linter/` の structure を Read で確認して決定
- [ ] `.claude/hooks-config.toml` に `[post_tool_use.file_size_check]` section を追加 (default OFF、上記 config schema)
- [ ] hook binary 実装: Edit / Write path 取得 → glob match → size 確認 → error/PASS
- [ ] memory `feedback_test_dry_antipattern` 適用の test 追加: enabled=false / paths 不一致 / size 未超過 / size 超過 / touch_trigger=false の 5+ variant 独立 setup
- [ ] cargo clippy + cargo test pass 確認
- [ ] dogfood: 本タスク実装後に `docs/todo10.md` を意図的に閾値超過させて hook が error を返すことを実観測
- [ ] `pnpm build:all` + `pnpm deploy:hooks` で派生プロジェクト 2 件 (techbook-ledger / auto-review-fix-vc) へ配布判断 (各派生プロジェクトの `hooks-config.toml` で個別 enable / disable 制御可能)
- [ ] ADR-007 (custom-linter layer boundary) に本 hook の位置付けを 2-3 行追記 (= ファイルサイズは AST 解析不要の正規表現未満の単純 check 層に位置)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- PostToolUse Edit / Write で対象 path 編集 → サイズ閾値超過時に error 通知が出る
- `enabled = false` で完全停止可能 (kill-switch、ADR-039 整合)
- threshold_bytes / paths / touch_trigger が config から設定可能
- touch-trigger ratchet で既存超過ファイルは未編集なら grandfather
- 5+ variant test で各分岐独立検証
- `cargo clippy --workspace -- -D warnings` clean (順位 175 land 後は stop_quality でも mechanical 強制)

#### 詰まっている箇所

なし。Effort S-M で structural improvement、本セッション体験の直接対策。配置先 option A vs B のみが着手時の判断点。

---

### `state.rs` の behavioral invariant test を ADR-041 pattern で追加 (週次レビュー 2026-05-30 S02 採用)

> **動機**: 週次レビュー WR-2026-05-30-S02 で検出。`src/cli-pr-monitor/src/state.rs:226-510` の test は JSON round-trip (serde 直列化 / 逆直列化) のみを検証し、**behavioral invariant** (例: `rate_limit` が `Some` の場合に `update_state_from_check_result()` が `ci` field を populate しない) を test していない。状態遷移 regression が test suite を通り抜ける構造的リスク。ADR-041 (Test Isolation Patterns for Multi-Condition Guards) で確立された「sentinel 事前投入 + mutation 不在を assert」pattern が本リポジトリの canonical 対策。
>
> **本タスクの位置づけ**: 週次レビュー (ADR-031) 2026-05-30 dogfood で採用 (severity=medium, facet=simplicity, category=test-anti-pattern、2026-05-30 ユーザー承認)。analyzer rationale: "Concrete, low-effort (add 3-5 tests), high value (catches state regression bugs). ADR-041 is already the project's documented pattern for this exact problem type."
>
> **参照**: `.claude/weekly-reviews/2026-05-30.md` § Findings、`src/cli-pr-monitor/src/state.rs:226-510` (既存 test、JSON round-trip のみ)、`docs/adr/adr-041-test-isolation-patterns.md` (適用 pattern source)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。3-5 test 追加で済む、既存 ADR-041 pattern 流用。

#### 設計決定 (案)

- **対象 invariant 候補** (analyzer 提案 + 派生):
  1. `rate_limit` が `Some` 時、`update_state_from_check_result()` は `ci` field を更新しない
  2. `rate_limit.until_unix_secs` が過去時刻になった場合、次回 update で `rate_limit` が `None` に reset される (timer expiry)
  3. `notified` flag が `true` の場合、再度 `update_state_*` を呼んでも `notified` は維持される (idempotency)
  4. (実装側で発見し次第追加)
- **ADR-041 pattern 適用**:
  - 各 test variant は独立 setup (`memory feedback_test_dry_antipattern`)
  - sentinel value を事前投入 (`ci.overall = "MUTATION_CHECK_SENTINEL"` 等)、mutation が起こったか否かを明示的に assert
  - guard condition を partial に偽にする setup で「他 guard が真でも mutation が起こらないこと」を保証

#### 作業計画

- [ ] `src/cli-pr-monitor/src/state.rs:226-510` を Read で全件確認、現状の test スコープと不足 invariant を整理
- [ ] ADR-041 § Test Isolation Patterns を Read で再確認
- [ ] 3-5 behavioral invariant test を `#[cfg(test)]` mod に追加 (memory `feedback_test_dry_antipattern` 適用、各 variant 独立 setup)
- [ ] cargo test -p cli-pr-monitor で全 pass 確認
- [ ] mutation regression check: 意図的に invariant を破る変更 (例: rate_limit check を削除) を local で適用して新 test が落ちるか手動検証
- [ ] cargo clippy clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3-5 behavioral invariant test が追加され全 pass
- silent regression を test で 1 件以上検出できる構造 (意図破壊で新 test 落ちる確認済)
- ADR-041 pattern の rationale を test コメントで cite (sentinel 事前投入 + mutation 不在 assert)

#### 詰まっている箇所

なし。Effort S、既存 pattern + 既存 test 構造への追加で完結。

---

### rate-limit retry decision boundary test を rstest parameterized で追加 (週次レビュー 2026-05-30 S03 採用)

> **動機**: 週次レビュー WR-2026-05-30-S03 で検出。`src/cli-pr-monitor/src/config.rs:94-122` の rate-limit retry logic + `stages/poll.rs` 周辺で、`max_retries=3` (固定値) のみが test されており **decision boundary** (`max_retries=0` で retry されない / `max_retries=1` で 1 回だけ retry / `max_retries=3` で boundary 通過後 `action_required` 遷移) が未検証。off-by-one error (`<` vs `<=`) が silent regression として通る構造的リスク。rstest crate は既に本リポジトリで使用済のため新 dep 不要。
>
> **本タスクの位置づけ**: 週次レビュー (ADR-031) 2026-05-30 dogfood で採用 (severity=medium, facet=simplicity, category=test-anti-pattern、2026-05-30 ユーザー承認)。Bundle CR-RL (順位 167-169) と隣接領域 (= rate-limit detection の周辺 logic) のため follow-up 価値高。analyzer rationale: "Low effort (rstest parameterized test, ~15 lines). rstest is already in use in the codebase. High value: catches the exact off-by-one class that single-value tests miss."
>
> **参照**: `.claude/weekly-reviews/2026-05-30.md` § Findings、`src/cli-pr-monitor/src/config.rs:94-122` (RateLimitConfig + max_retries field)、`src/cli-pr-monitor/src/stages/poll.rs` (retry 適用 site)、Bundle CR-RL (順位 167-169) の隣接 context、`feedback_test_dry_antipattern` 適用

#### 設計決定 (案)

- **rstest parameterized test 構造**:

  ```rust
  #[rstest]
  #[case(0, vec![])]                                      // max_retries=0: retry なし
  #[case(1, vec![true, false])]                           // max_retries=1: 1 retry 後 stop
  #[case(3, vec![true, true, true, false])]               // max_retries=3: full boundary coverage
  fn rate_limit_retry_boundary(#[case] max_retries: u32, #[case] expected_continues: Vec<bool>) {
      // setup + execution + assert
  }
  ```

- **boundary 観点**:
  - 0 retry: 最初の attempt の後 `action_required` 遷移を確認
  - max_retries 到達: 連続 retry 後の最終 attempt で `action_required` に遷移
  - off-by-one: `< max_retries` か `<= max_retries` かを test 経由で pin (実装の `<` を `<=` に変えると新 test が落ちる)

#### 作業計画

- [ ] `src/cli-pr-monitor/src/config.rs` の `RateLimitConfig::max_retries` 周辺 + `stages/poll.rs` の retry decision logic を Read で確認
- [ ] 既存 rstest 使用箇所を grep で確認、import / pattern を踏襲
- [ ] `#[cfg(test)]` mod に `rate_limit_retry_boundary` parameterized test を追加 (3-4 case)
- [ ] cargo test -p cli-pr-monitor で全 pass 確認
- [ ] off-by-one regression check: `<` を `<=` に意図的変更で test が落ちることを手動検証
- [ ] cargo clippy clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3-4 parameterized case が全 pass
- off-by-one error が test で検出可能 (`<` ↔ `<=` mutation で test 落ちる)
- 既存単一値 test は維持 (backward compat)

#### 詰まっている箇所

なし。Effort S、rstest pattern 既存使用 + 約 15 行で完結。

---

### `lib-report-formatter` に markdown pipe / newline escape を追加 (週次レビュー 2026-05-30 C01 採用)

> **動機**: 週次レビュー WR-2026-05-30-C01 で検出。`src/lib-report-formatter/src/lib.rs:51-79` の `format_table()` が PR title / commit message を markdown table 行に直接埋め込むが、`|` / `\n` を escape していない。「`Fix | Critical | src/main.rs`」のような PR title が markdown table 構造を破壊し、**downstream AI facet が malformed row を Read 時に misinterpret する prompt injection リスク** が存在。PR title は外部 actor (= PR 作成者) が制御可能な input source のため defense-in-depth 重要。
>
> **本タスクの位置づけ**: 週次レビュー (ADR-031) 2026-05-30 dogfood で採用 (severity=medium, facet=security, category=prompt-injection、2026-05-30 ユーザー承認)。本セッションの 5 PR chain で AI facet 連鎖が systemic 化したため、prompt injection 防御層は今後の facet 拡張 (順位 153 / 154 等) でも継続価値あり。
>
> **参照**: `.claude/weekly-reviews/2026-05-30.md` § Findings、`src/lib-report-formatter/src/lib.rs:51-79` (format_table 実装)、`src/cli-merge-pipeline/src/feedback.rs:114-123` (PR title データ source)、ADR-022 (責務分離) との整合 (= utility は lib-* に集約)

#### 設計決定 (案)

- **`escape_markdown_pipe(s: &str) -> String`** ユーティリティを `lib-report-formatter` に追加:

  ```rust
  pub fn escape_markdown_pipe(input: &str) -> String {
      input.replace('|', "\\|").replace('\n', " ")
  }
  ```

- **call site 修正**: `format_table()` で user-controlled field (PR title / commit message / author) を embed する箇所に `escape_markdown_pipe()` を適用
- **test 追加** (memory `feedback_test_dry_antipattern` 適用、各 variant 独立):
  - 通常 ASCII (pipe / newline なし) → 変更なし
  - pipe 単独 (`a | b`) → `a \| b`
  - newline 単独 (`a\nb`) → `a b`
  - pipe + newline 混合
  - empty string

#### 作業計画

- [ ] `src/lib-report-formatter/src/lib.rs:51-79` を Read で `format_table()` 全体確認、user-controlled embed 箇所を特定
- [ ] `escape_markdown_pipe()` を lib に追加、pub export
- [ ] `format_table()` の embed call site を escape 経由に書き換え
- [ ] `#[cfg(test)]` に 5 variant test を独立 setup で追加
- [ ] cargo test + cargo clippy clean
- [ ] 受け先 (cli-merge-pipeline 等) で test がまだ pass することを cargo test --workspace で確認 (call signature 変更なしのため backward compat 維持)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `escape_markdown_pipe()` が `lib-report-formatter` に pub function として追加される
- 5 variant test が独立 pass
- `format_table()` の user-controlled embed が escape 経由になる
- markdown table 構造破壊 PR title (`Fix | Critical`) を fixture で渡しても table 整合維持を assert

#### 詰まっている箇所

なし。Effort S、5 行 utility + call site 修正 + 5 test で完結。

---

### `aggregate-weekly` facet の `findings.json` 出力を raw JSON にする (Phase D dogfood D-A 採用)

> **動機**: Phase D dogfood (週次レビュー 2026-05-30 実行) で検出した skill 統合 bug。`aggregate-weekly` facet が write する `findings.json` が ` ```json ... ``` ` の markdown code fence で wrap されており、Phase C skill (`/weekly-review`) が JSON parser に直接渡せない。本 dogfood では skill 内で fence を手動 strip して pending JSON を構築したが、**facet 出力を raw JSON にすれば skill 側の workaround が不要**になる。
>
> **本タスクの位置づけ**: Phase D dogfood (本セッション 2026-05-30 実施) の skill flow 実観測で発見、本 PR の dogfood 観測点 (D-A) として user 承認 (2026-05-30)。週次レビュー (ADR-031) facet 出力の整合性確保。
>
> **参照**: `.takt/facets/instructions/aggregate-weekly.md` (修正対象)、`.takt/runs/20260529-150611-weekly-review-2026-05-30/reports/findings.json` (Phase D dogfood で実観測した fence 付き出力)、`~/.claude/skills/weekly-review/SKILL.md` Phase 2 (現 skill が手動 fence strip した workaround)

#### 設計決定 (案)

- **`aggregate-weekly.md` の output 指示を明確化**:
  - 現状: instruction が「JSON は ... `findings.json` というファイル名で write する」と書いてあるが、facet LLM が markdown 出力癖 (` ```json...``` ` 自動 wrap) で fence 付きで write してしまう
  - 修正: instruction で「**raw JSON のみ** (markdown code fence なし) で write する。先頭は `{` で始まり、末尾は `}` で終わる必要がある」を明示
  - test 文言例: `{"run_date": "...", ...}` から始まる、` ```json` で始まらない、を強調
- **alternative**: skill 側で fence 検出 + strip を実装する (但し source-of-truth が facet 側であるべき)

#### 作業計画

- [ ] `.takt/facets/instructions/aggregate-weekly.md` の `## Phase 3` (= JSON 生成 section) に「**raw JSON 出力必須、markdown code fence で囲まない**」warning を追加
- [ ] JSON 出力例の前後 context を Edit で明確化
- [ ] dogfood: 修正後に次の `/weekly-review` 実行で `findings.json` が raw JSON で出力されることを実観測 (Phase E で確認)
- [ ] (option) Phase C skill SKILL.md にも「fence wrap された場合の defensive strip 手順」を補足追記 (= belt-and-suspenders)
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `aggregate-weekly.md` instruction で raw JSON 出力要件が明示される
- 次回 `/weekly-review` 実行で `findings.json` が raw JSON (= ` ```` ` で wrap されない) で出力される dogfood 観測

#### 詰まっている箇所

facet 側の文言修正のみで facet LLM の出力 habit を矯正できるかは未確定。修正後の dogfood 結果次第で alternative (= skill 側 strip) に切り替える判断あり。Effort XS-S。

---

### `/weekly-review` skill に重複検出 (簡易 grep) を Phase 4 で追加 (Phase D dogfood D-B 採用)

> **動機**: Phase D dogfood (週次レビュー 2026-05-30 実行) で **WR-2026-05-30-S05 (`combine_output` dead-code) が既存 順位 173 (PR #182 dry-run S01 採用) と完全重複**であることを実観測。ADR-031 § Phase 4 で「**重複検出は MVP では実装しない**」と明示済だが、本 dogfood で「2 PR で同じ finding が出る」を実証したため、最低限の grep ベース簡易検出を後追い追加する妥当性が確立。MVP は description 先頭 40 chars の grep ヒットを警告表示するのみで、自動 merge は行わない (user 判断に委ねる ADR-031 原則維持)。
>
> **本タスクの位置づけ**: Phase D dogfood (本セッション 2026-05-30 実施) で observability gain (= 重複が見える) を実証、user 承認 (2026-05-30) で skill 拡張採用。Phase E 試験運用 dogfood の前に整備しておくと user 判断負荷を圧縮。
>
> **参照**: `~/.claude/skills/weekly-review/SKILL.md` Phase 4 (修正対象)、ADR-031 § todo.md 反映ルール (「重複検出は MVP では実装しない」記述、本タスクで「MVP+1」相当に拡張)、Phase D dogfood の実観測 (WR-2026-05-30-S05 ↔ 順位 173 重複)

#### 設計決定 (案)

- **簡易 grep 重複検出 in Phase 4**:

  ```bash
  # finding を docs/todo.md 系列に書き込む前に実行
  TITLE_PREFIX=$(echo "$finding_description" | head -c 40)
  HITS=$(grep -li "$TITLE_PREFIX" docs/todo.md docs/todo*.md 2>/dev/null)
  if [ -n "$HITS" ]; then
      # AskUserQuestion で「augment / 新規 / skip」を聞く
  fi
  ```

- **3 択 AskUserQuestion**:
  1. **augment**: 既存 entry に補足追記 (= 「重複 observation を別 dogfood で再確認、優先度上昇」記録)
  2. **新規**: 重複と認識した上で別 entry 化 (= scope or角度 が異なる場合)
  3. **skip**: 重複と認識して書き込まない (= 既存 entry で十分)
- **自動 merge は行わない**: ADR-031 原則の「重複検出は MVP では実装しない」(自動 merge は MVP 超過、observability のみ提供) は維持
- **grep target**: `docs/todo.md docs/todo2-10.md` 全件 (= 現在の todo file 集合、新 todoN+1.md 追加時は SKILL.md update が必要)

#### 作業計画

- [ ] `~/.claude/skills/weekly-review/SKILL.md` Phase 4 § 重複検出 (簡易) を expansion: 現状の grep + 警告のみ → 警告 + 3 択 AskUserQuestion に変更
- [ ] grep target file 列を docs/todo*.md glob 化、追加ファイル時の自動追従 (固定 list 回避)
- [ ] dogfood 観測の cite を skill 内 inline で明示 (= Phase D 2026-05-30 で WR-2026-05-30-S05 ↔ 順位 173 重複検出済、本 logic が機能した実例)
- [ ] **`feedback_global_config_backup` 適用**: ~/.claude/skills/ 編集前 snapshot 取得
- [ ] markdownlint clean
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc 等) への skill 配布判断は別タスク (本 skill は global 配置だが ADR-031 自体は本リポジトリ ADR、派生展開は要検討)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- skill Phase 4 で grep 重複検出 → 3 択 AskUserQuestion → user 判断 経路が実装される
- 次回 `/weekly-review` 実行で重複候補が user 提示される dogfood 観測
- ADR-031 「重複検出 MVP 未実装」を「MVP+1 (簡易 grep)」相当に格上げ、但し自動 merge なし原則は維持
- skill 編集前後の ~/.claude snapshot が backup される

#### 詰まっている箇所

なし。Effort XS-S、SKILL.md の Phase 4 section 拡張 + Bash snippet 追加で完結。

---

### `behind = None` fail-closed テスト追加 (PR #194 T2-#1 採用)

> **動機**: PR #194 で `hooks-pre-tool-validate/main.rs:847` の `behind?` (= None で early-return = fail-open) を CodeRabbit Major #5 として指摘され、takt-fix で `behind.is_none_or(|n| n > 0)` (= None で stale=true = fail-closed) に修正済。既存テストは `Some(0)` / `Some(2)` / `Some(3)` のみで `None` ケースが未検証のため、将来 `behind?` 系の semantic mismatch が回帰しても test で捕捉できない。
>
> **本タスクの位置づけ**: PR #194 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Low / Effort XS / Adoption Risk None、2026-06-04 ユーザー承認)。fail-closed contract を test で明示化し回帰防止網を確立。
>
> **参照**: `.claude/feedback-reports/194.md` Tier 2 #1、`src/hooks-pre-tool-validate/src/main.rs:790-820` の `build_todo_staleness_message` 関数 + 同ファイル test module、ADR-041 (Test Isolation Patterns) の sentinel 事前投入 + mutation 不在 assert pattern
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。test 1 件追加で fail-closed contract が機械化される。

#### 設計決定 (案)

- 追加 test 名 (案): `build_todo_staleness_message_returns_some_when_behind_is_none`
- 検証内容: `build_todo_staleness_message(file_path, None, &[], "master")` が `Some(message)` を返し、message が "判定不能" / "fail-closed" を含むことを assert
- 既存 test (`Some(0)` / `Some(>0)`) とは独立に書く (memory `feedback_test_dry_antipattern` per AAA 各テスト独立性優先)
- 関連: `check_todo_staleness` 経路の `behind = None` 統合 test 追加も検討 (`count_commits_branch_ahead` モック困難なら skip 可)

#### 作業計画

- [ ] `src/hooks-pre-tool-validate/src/main.rs` の test module に `behind = None` ケースを 1 件追加
- [ ] `cargo test --workspace` で pass 確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `behind = None` 時の fail-closed 挙動が test で明示
- 既存テストと non-regression

#### 詰まっている箇所

なし。XS effort、test 1 件追加で完了。

---

### Revset scope + sweep 統合テスト拡張 (PR #194 T2-#2 採用)

> **動機**: PR #194 で `sweep_empty_commits_in_pr_range` の初版 revset `empty() & (master..@)` が CodeRabbit Major #2「削除対象 revset が広すぎる」で指摘され、takt-fix で `description(substring:"fix(review):")` フィルタを追加して `create_fix_commit` 由来の空 commit のみに scope 限定。現状の integration test は `fix(review):` prefix 付き empty のみ作成して `master` branch + happy path のみ検証しており、(1) 他メッセージ型 (feat: / fix: / docs:) を作成して誤 abandon されないことを検証していない、(2) `SweepConfig.default_branch` を `main` / `staging` 等の alternative 設定で検証していない、(3) boundary cases (match 0 / 全 match) が分離されていない。
>
> **本タスクの位置づけ**: PR #194 post-merge-feedback Tier 2 #2 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-04 ユーザー承認)。SweepConfig 設定可能化に伴う variant test 整備で revset 過剰 scope の回帰防止網を厚くする。
>
> **参照**: `.claude/feedback-reports/194.md` Tier 2 #2、`src/cli-pr-monitor/src/fix_commit.rs` の `sweep_empty_commits_in_pr_range` + 既存 integration tests (`integration_sweep_empty_commits_*`)、`src/cli-pr-monitor/src/config.rs` の `SweepConfig`
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。test 3-4 件追加で scope limit 機械検証 + alternative config dogfood。

#### 設計決定 (案)

- 追加 test (`#[ignore]` integration):
  1. `integration_sweep_skips_non_fix_review_empty_commits`: `feat: empty` / `fix: empty` / `docs: empty` 等の description で empty commit を作成し、sweep 後にこれらが残存することを assert (negative case)
  2. `integration_sweep_respects_default_branch_config`: `default_branch = "main"` / `"staging"` で sweep が正しく該当範囲のみ対象にすることを assert
  3. `integration_sweep_no_op_on_zero_matches`: `fix(review):` empty が 0 件のとき abandon 試行が走らないことを assert (jj log 出力空時の early return)
  4. (既存) `integration_sweep_empty_commits_abandons_multiple_in_range` を全 match シナリオとして boundary 化

#### 作業計画

- [ ] 上記 1-3 の追加 test を `src/cli-pr-monitor/src/fix_commit.rs` test module に追加
- [ ] helper 関数 (例: `build_jj_empty_with_description`) を抽出し test 本体を 50 行以内に保つ
- [ ] `cargo test --workspace -- --ignored --test-threads=1` で pass 確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 4 variant test (positive / negative / boundary 0 / boundary all) が pass
- non-`fix(review):` empty commit の誤 abandon が test で検出される構造

#### 詰まっている箇所

なし。Effort S、既存 integration test 拡張で完結。

---

### jj integration test companion パターン codify (PR #194 T2-#3 採用)

> **動機**: PR #194 で `integration_sweep_empty_commits_abandons_multiple_in_range` の初版が `count_empty_in_pr_range == 0` を assert する設計で書かれたが、jj は abandon 後に空 WC を自動生成するため `count != 0` で false assertion 失敗 (実際には sweep は意図通り動作していた) という設計ミスが session 内で発生。`assert_descriptions_absent_in_pr_range` helper で description-based assert に修正済。jj の内部動作 (auto-snapshot + 空 WC 自動生成) に対する incomplete mental model で同型ミス再発確率 Medium。
>
> **本タスクの位置づけ**: PR #194 post-merge-feedback Tier 2 #3 採用 (Severity Low / Frequency Medium / Effort M / Adoption Risk None、2026-06-04 ユーザー承認)。jj integration test の正しい不変式パターン (count NG / description-based assert OK) を **companion テンプレート** として codify。
>
> **参照**: `.claude/feedback-reports/194.md` Tier 2 #3、`src/cli-pr-monitor/src/fix_commit.rs` の `assert_descriptions_absent_in_pr_range` helper、T3-3 (jj revset composability ADR 拡張) と相補関係 (revset 設計 ADR + test 設計 pattern の対)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。jj 操作コード全般の test pattern 確立で将来 jj 関連 PR の reviewer cost を削減。

#### 設計決定 (案)

- 追加内容:
  1. `src/cli-pr-monitor/src/fix_commit.rs` の test module 冒頭に「jj integration test 不変式パターン」doc comment を追加 (count NG / description-based assert OK / sentinel 事前投入の 3 点を例示)
  2. `~/.claude/rules/common/testing.md` に「jj 操作コードの integration test pattern」section を追記 (memory `feedback_global_config_backup` per `~/.claude` snapshot 必須)
  3. ADR-021 (T3-3 で拡張) で jj revset 設計と test pattern を相互参照

#### 作業計画

- [ ] `~/.claude` snapshot 取得
- [ ] `assert_descriptions_absent_in_pr_range` helper の doc comment を強化 (NG/OK パターン明示)
- [ ] `~/.claude/rules/common/testing.md` § "jj 操作コードの test pattern" section 追加
- [ ] (T3-3 と同 PR 推奨) ADR-021 拡張で test pattern reference を入れる
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- companion pattern が global rule に codify され派生プロジェクトへ波及
- 既存 helper の doc が NG/OK パターンを明示

#### 詰まっている箇所

なし。Effort M (global rule 編集 + ADR 拡張のため Medium effort)、複数ファイル更新だが scope 明確。

---

### ADR-039 補完 — experimental feature 設計チェックリスト (PR #194 T3-#1 採用)

> **動機**: PR #194 で `SweepConfig` の初版が ADR-039 (experimental feature 標準パターン) の 3 要件 (config opt-in / kill-switch / bounded lifetime) のうち **kill-switch + bounded lifetime が漏落** した状態で実装され、CodeRabbit Major #4 で指摘 → takt-fix で `enabled = false` default + config-driven gate を追加して修正。ADR-039 は手動 checklist のみで機械検証なしのため、今後の experimental feature 追加時に同型ミス再発確率 Medium。
>
> **本タスクの位置づけ**: PR #194 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None、2026-06-04 ユーザー承認)。ADR-039 に 6 点チェックリスト追加 + `~/.claude/rules/common/patterns.md` に "experimental feature 追加時の ADR-039 参照必須" section 追加で設計段階での誤り防止。
>
> **参照**: `.claude/feedback-reports/194.md` Tier 3 #1、`docs/adr/adr-039-experimental-feature-standard-pattern.md` (拡張対象)、`~/.claude/rules/common/patterns.md` (新 section 追加対象、memory `feedback_global_config_backup` per snapshot 必須)、`src/cli-pr-monitor/src/config.rs` の `SweepConfig` (実装事例)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。docs 拡張 + global rule 追加で設計段階の機械化に近づく (完全機械化は T1-2 lint rule が 🤔 様子見、本 task はその docs 補完)。

#### 設計決定 (案)

- ADR-039 に追加する 6 点チェックリスト:
  1. `enabled: bool` フィールド with `#[serde(default = "default_enabled_false")]`
  2. `kill_switch: bool` フィールド (緊急停止用、env var override も検討)
  3. `ttl_days` または `expires_at` フィールド (bounded lifetime)
  4. デフォルト値関数の明示 (`fn default_enabled() -> bool { false }`)
  5. 実行ゲートで `config.enabled && !config.kill_switch && !is_expired(&config)` の 3 段完全チェック
  6. integration test で `enabled = false` 時に skip され機能が完全 OFF になることを assert
- `~/.claude/rules/common/patterns.md` 追加 section: 「experimental feature 追加時は ADR-039 を参照し 6 点チェックリストを通すこと」を 3-5 行で codify

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `docs/adr/adr-039-*.md` に § 設計チェックリスト section を追加 (6 点)
- [ ] `~/.claude/rules/common/patterns.md` に "experimental feature" section 追加
- [ ] PR description で ADR-039 リンクを cite し SweepConfig を実装事例として参照
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-039 にチェックリストが追記され、次の experimental feature PR で参照される
- patterns.md 経由で派生プロジェクトに波及

#### 詰まっている箇所

なし。Effort S、docs 編集のみ。`~/.claude` snapshot を取れば revert 可能。

---

### Fail-closed patterns ADR 新設 (PR #194 T3-#2 採用)

> **動機**: PR #194 で `behind?` (Option<usize>) を使った fail-open bug が観測され、CodeRabbit Major #5 で「security gate は判定不能時 fail-closed であるべき」と指摘 → takt-fix で `is_none_or` イディオムに修正。Rust `?` 演算子は便利だが、security gate / quality gate 関数で `Option<T>?` を使うと None で関数全体が早期 return = gate を bypass する fail-open 挙動になり、直感に反する semantic mismatch。今後も繰り返すパターンのため設計原則として codify する。
>
> **本タスクの位置づけ**: PR #194 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-04 ユーザー承認)。新 ADR で fail-closed 原則と `is_none_or` イディオムを永続化、設計時の判断根拠を明文化。
>
> **参照**: `.claude/feedback-reports/194.md` Tier 3 #2、PR #194 commit `dfad56ff` (`hooks-pre-tool-validate/main.rs:847` の `behind?` → `is_none_or` 修正)、ADR-021 (jj 操作の fail-safe 方向との対比、T3-3 で拡張)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。ADR 1 ファイル新設で設計判断の永続化。

#### 設計決定 (案)

- ADR-NNN タイトル (案): "Security/Quality Gate での Fail-Closed 原則" (NNN は land 時 PR で確定、順位 135 codified placeholder policy 適用)
- セクション構成:
  1. **背景**: PR #194 で観測した fail-open bug の最小再現
  2. **原則**: 判定不能 (None / Err / timeout) はデフォルト blocking
  3. **Rust idiom**: `Option<T>?` は fail-open、`is_none_or(|n| ...)` / `map_or(true, |n| ...)` は fail-closed
  4. **反例**: `if behind.is_none() { return None; }` 系 (= fail-open) を gate 関数で使ってはいけない
  5. **適用範囲**: hooks-pre-tool-validate、hooks-stop-quality、cli-push-runner stages 等の gate 関数
  6. **参照**: PR #194、CodeRabbit Major #5、ADR-021 (jj 操作の fail-safe 方向との対比)

#### 作業計画

- [ ] `docs/adr/adr-NNN-security-gates-fail-closed.md` 新設 (NNN は land 時 PR で確定)
- [ ] CLAUDE.md の ADR リスト table に新 ADR を追加
- [ ] PR #194 commit を参照例として inline cite
- [ ] (オプション) T3-3 ADR-021 拡張で「sweep の fail-open」と「gate の fail-closed」の対比 cross-ref
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 新 ADR が land され、CLAUDE.md table に登録
- 今後の gate 系コードレビューで本 ADR が参照される

#### 詰まっている箇所

なし。Effort S、ADR 起草で完結。番号は land 時 PR の `ls docs/adr/adr-*.md | sort | tail -1` で確定。

---

### jj revset composability ADR 拡張 (PR #194 T3-#3 採用)

> **動機**: PR #194 で `sweep_empty_commits_in_pr_range` の初版が「全 empty commit を Rust 側で取得 → for ループで `description` field を check」という generate-then-filter 設計だったが、`description(substring:"fix(review):")` を **revset 側で filter** することで output が最初から絞られる方が efficient + reviewer cost 低 + scope 明確という改善が takt-fix iteration で観測。今後の jj 操作コードで同型 inefficiency が初期ドラフトに入る確率 Medium。
>
> **本タスクの位置づけ**: PR #194 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None、2026-06-04 ユーザー承認)。ADR-021 (jj 変更検出ロジックの設計原則) を拡張し revset composition patterns + 設計チェックリストを追記。
>
> **参照**: `.claude/feedback-reports/194.md` Tier 3 #3、`docs/adr/adr-021-jj-change-detection-principles.md` (拡張対象)、`src/cli-pr-monitor/src/fix_commit.rs:217-218` (実装事例)、T2-3 (jj test pattern companion) と相補関係
>
> **実行優先度**: 💎 **Tier 3** — Effort S。ADR 拡張で設計原則を永続化。

#### 設計決定 (案)

- ADR-021 に追加する § Revset Composability:
  1. **原則**: 「何を取得するか」を jj revset で最小化してから Rust に返す (generate-then-filter 回避)
  2. **典型 filter 関数**:
     - `empty()` — file change なし
     - `description(substring:"...")` — description 部分一致 (parens 含む文字列も safe)
     - `description(exact:"...")` / `description(regex:"...")` — 厳密 / 正規表現マッチ
     - `(branch..@)` — range 限定
     - `author(...)` / `mine()` — author 絞り込み
  3. **設計チェックリスト** (新規 jj 操作コード書く前に):
     - [ ] 取得目的は明示 (例: "fix(review): empty commits in PR range")
     - [ ] 各条件を revset で表現できないか検討
     - [ ] `&` (AND) / `|` (OR) で組合せ可能か
     - [ ] description マッチは parens / 記号を含む場合 `substring:` 修飾子を必須化 (default `exact:` で 0 hit する bug 対策)
  4. **anti-pattern**: 「全列挙 → Rust for ループ filter」(scope 過剰 / token 浪費 / scope drift リスク)

#### 作業計画

- [ ] ADR-021 末尾に § Revset Composability section を追記
- [ ] PR #194 の `sweep_empty_commits_in_pr_range` を実装事例として参照
- [ ] (T2-3 と同 PR の場合) T2-3 test pattern と相互参照
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-021 に composability 原則 + チェックリストが codify される
- 今後の jj 操作コード PR で本 section が参照される

#### 詰まっている箇所

なし。Effort S、ADR 拡張のみ。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo9.md 末尾を参照。)
