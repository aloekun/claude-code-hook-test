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

### hardcoded `"master..@"` lint rule 追加 (PR #195 T1-#1 採用) ★ Bundle 195-FB

> **動機**: PR #195 で `assert_descriptions_absent_in_pr_range` が `"master..@"` を hardcode していた問題 (CR Major 指摘 → fix commit `9663dd68` で default_branch 引数化済) と同型の hardcode が `count_empty_in_pr_range` (`src/cli-pr-monitor/src/fix_commit.rs:602-619`) にも残留。pre-push simplicity reviewer は F-1 として non-blocking で挙げたが、`"main"` 等の alternative branch 分岐 test が追加された際の silent breakage リスクが顕在。手動 grep / reviewer 判断で塞ぐと再発するため、機械検出層 (custom-lint-rule) で押さえる。
>
> **本タスクの位置づけ**: PR #195 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk Low、2026-06-04 ユーザー承認)。`"master\.\.@"` 文字列リテラルを Rust テストコード内で検出して parameterize 候補としてフラグ立て。`jj` revset 固有構文のため false positive リスク低。
>
> **参照**: `.claude/feedback-reports/195.md` Tier 1 #1、PR #195 CR Major (2026-06-04T07:58:27Z)、commit `9663dd68` (assert helper の default_branch 引数化)、`src/cli-pr-monitor/src/fix_commit.rs:602-619` (`count_empty_in_pr_range` 残留 hardcode)、ADR-007 (custom-linter layer boundary)
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。lint rule 1 件 + extensions test 4 件 (rs / 主要拡張子 main_ext_tests) で機械検出層を確立。T2-#1 (順位 190) と直接連動 = 機械検出が auto-fix promotion 候補を提示し、T2-#1 で受け修正。

#### 設計決定 (案)

- rule id (案): `no-hardcoded-jj-revset-range`
- pattern (案): `"master\\.\\.@"` (引用符内のみ検出、false positive 抑制のため `args!\[` / `.args\(` の文字列リテラル要素のみマッチさせる絞り込みも検討)
- extensions: `["rs"]`
- severity: `warning` (auto-fix は scope 外)
- paths filter: source folder 限定 (test fixture / docs は除外)
- TOML meta field `[rules.test_coverage]` で `main_ext_tests.rs` 宣言、`rule_test_coverage_check` cargo test に統合 (順位 137 / 138 land 済 infrastructure 流用)
- positive test: `master..@` リテラルを含む sample で fire
- negative test: doc comment / inline jj 関連コメント内の `master..@` は除外 (現状の rule⑨ doc-comment 除外と同 pattern)

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に `no-hardcoded-jj-revset-range` rule 追加 (TOML meta field 含む)
- [ ] `src/hooks-post-tool-linter/src/main.rs` test module に positive / negative test 4 件追加 (case insensitive flag は不要、revset 構文は case sensitive)
- [ ] `cargo test --package hooks-post-tool-linter` で pass 確認 (新規 test + 既存 `rule_test_coverage_check` 整合)
- [ ] 既存 `count_empty_in_pr_range` が fire することを dogfood で確認 (= T2-#1 の前提)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- lint rule が `count_empty_in_pr_range` 内の `"master..@"` を warning として検出
- 順位 137-138 の TOML meta + cargo test 統合に整合
- `assert_descriptions_*` 系の修正済 helper では fire しない (false positive 0)

#### 詰まっている箇所

なし。Effort S、既存 rule 追加 pattern を踏襲。T2-#1 と同一 PR で land して dogfood 即時化推奨。

---

### `count_empty_in_pr_range` パラメータ化 + "main" variant test (PR #195 T2-#1 採用) ★ Bundle 195-FB

> **動機**: PR #195 で integration test helper のうち 3 関数 (`assert_descriptions_absent_in_pr_range` / `assert_descriptions_present_in_pr_range` / `count_empty_in_pr_range`) のうち、前 2 つは `default_branch` 引数化済だが、`count_empty_in_pr_range` のみ `"master..@"` を hardcode 残留。pre-push F-1 で non-blocking 観測 + CR Major 指摘 (commit 9663dd68 で 2 関数のみ修正) で systemic に発見。API cohesion (companion helper group の引数 signature 整合) を確保する。
>
> **本タスクの位置づけ**: PR #195 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Medium / Effort XS / Adoption Risk None、2026-06-04 ユーザー承認)。順位 189 (T1-#1 lint rule) と同 PR 推奨 = lint rule が fire することを実装層で受けて修正、dogfood の循環を 1 PR で成立。
>
> **参照**: `.claude/feedback-reports/195.md` Tier 2 #1、`src/cli-pr-monitor/src/fix_commit.rs:602-619`、commit `9663dd68` (前 2 関数の修正)、順位 189 (lint rule)
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。1 関数の signature 変更 + 1 caller (= test 内部) 更新 + alternative branch variant test 追加。

#### 設計決定 (案)

- `count_empty_in_pr_range(repo_dir, default_branch: &str) -> usize` に signature 変更
- `format!("empty() & ({}..@)", default_branch)` で revset を組立
- 既存 caller (`integration_sweep_empty_commits_abandons_multiple_in_range` 内) は `count_empty_in_pr_range(repo_dir, "master")` に更新
- 追加 test: `integration_sweep_respects_alternative_default_branch` 内で `count_empty_in_pr_range(repo_dir, "main") >= 0` の正常性検証を加える (現状は count 不要だが、helper の "main" 動作を test で保護)
- 命名候補: 関数名は `count_empty_in_branch_range` に renaming するか議論余地あり (本 PR では signature 変更のみで rename 見送り、follow-up で再検討可)

#### 作業計画

- [ ] `src/cli-pr-monitor/src/fix_commit.rs` の `count_empty_in_pr_range` を引数化
- [ ] caller (= test 内部) を `"master"` 引数で更新
- [ ] `integration_sweep_respects_alternative_default_branch` に `count_empty_in_pr_range(repo_dir, "main")` の sanity check 追加
- [ ] `cargo test --package cli-pr-monitor integration_sweep -- --ignored --test-threads=1` で 5 件 pass 確認
- [ ] 順位 189 lint rule の fire / pass 状態を dogfood (rule 追加直後 fire → 修正後 0 fire の遷移を確認)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `count_empty_in_pr_range` が default_branch 引数を受け取る形に改修済
- helper group 3 関数の API signature が `(repo_dir, default_branch, ...)` で整合
- alternative branch (`"main"`) での動作が test で保護
- 順位 189 lint rule が修正後コードで fire しない

#### 詰まっている箇所

なし。Effort XS、signature 変更 + caller 1 箇所更新で完結。

---

### non-blocking finding 留置基準を `code-review.md` に追記 (PR #195 T3-#1 採用) ★ Bundle 195-FB

> **動機**: PR #195 で pre-push simplicity reviewer が F-1 (`count_empty_in_pr_range` の `"master..@"` hardcode) を **non-blocking** として挙げたが、これを「即修正 / 後続 PR / そのままマージ」のどれに振り分けるかの基準が `~/.claude/rules/common/code-review.md` 等に未 codify。PR #195 ではマージ判断時に根拠未明示で残置した形になり、後続 reviewer / 自分自身が再 visit する際に判断揺れが発生する構造リスク。
>
> **本タスクの位置づけ**: PR #195 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-04 ユーザー承認)。non-blocking findings の留置基準を明文化し reviewer / Claude judgment call の一貫性確保。
>
> **参照**: `.claude/feedback-reports/195.md` Tier 3 #1、PR #195 pre-push F-1 (non-blocking)、`~/.claude/rules/common/code-review.md` (追加対象、`feedback_global_config_backup` 適用必須)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule 編集のみ。派生プロジェクトに自動波及。

#### 設計決定 (案)

`~/.claude/rules/common/code-review.md` に新 section「Non-blocking findings の留置基準」を追加。基準 (案):

| 状況 | 推奨 action |
|---|---|
| `current impact: none` かつ「発火条件が未実装の usage pattern でのみ顕在化」 | **後続タスク化** (todo*.md に登録、本 PR では修正しない) |
| `current impact: low` かつ「fix を当該 PR の scope に追加すると iteration cost > value」 | **後続タスク化** または **そのままマージ** (PR description に "Out of scope" として明記) |
| `current impact: medium+` または「同型 finding が当該 PR 内で複数観測」 | **即修正** (auto-fix promotion) |

PR description / merge commit body に「non-blocking finding を残置した場合は理由を明記」する原則も追記 (= 残置の判断根拠を永続化、後続 reviewer が re-visit 不要にする)。

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/code-review.md` に § Non-blocking findings の留置基準 section 追加
- [ ] PR #195 を実例として inline cite (count_empty_in_pr_range hardcode = case 1 の典型)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- non-blocking findings の留置判断が table 形式で codify
- 残置時の理由明記原則が追記
- 派生プロジェクトに global rule として波及

#### 詰まっている箇所

なし。Effort XS、docs 編集のみ。

---

### test helper API 整合性ルール + ADR-041 参照を `patterns.md` / `testing.md` に追記 (PR #195 T3-#2 採用) ★ Bundle 195-FB

> **動機**: PR #195 で test helper の 3 関数グループ (assert_absent / assert_present / count_empty) のうち、parameterize したのは 2 関数のみで 1 関数 (count_empty_in_pr_range) を見落とした anti-pattern が観測 (CR Major で同型指摘)。companion helper group の引数 signature 整合性 = API cohesion を確保するルールが `~/.claude/rules/common/{patterns,testing}.md` に未 codify。順位 189 (T1-#1 lint rule) の mechanical 検出を docs 層で補完。`testing.md` の sentinel pattern section に ADR-041 リンクを追加してクロス参照を整理。
>
> **本タスクの位置づけ**: PR #195 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-06-04 ユーザー承認)。順位 189 / 190 / 191 と同 Bundle 195-FB で land 推奨 = mechanical + 実装 + docs の 3 層を 1 PR で揃える。
>
> **参照**: `.claude/feedback-reports/195.md` Tier 3 #2、PR #195 pre-push T3-1、CR Major (2026-06-04T07:58:27Z)、`~/.claude/rules/common/patterns.md` (追加対象)、`~/.claude/rules/common/testing.md` (sentinel section に ADR-041 リンク追加)、ADR-041 (Test Isolation Patterns)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule 編集のみ。

#### 設計決定 (案)

`~/.claude/rules/common/patterns.md` に新 section「Companion Helper Group の API Cohesion」を追加 (`Design Patterns` 配下):

- **原則**: 同一モジュール内で companion 関係にある test helper 群 (例: assert_* / count_* / build_* の系列) は **同 PR ですべての関数を揃える** ことを必須化
- **anti-pattern 例 (PR #195)**: 3 関数のうち 2 つを `default_branch` 引数化、1 つ (`count_empty_in_pr_range`) を見落とし → CR Major + pre-push F-1 で systemic 発覚
- **判定基準**: signature 変更で API contract が変わるとき、`grep -n "fn <prefix>_" <file>` で companion 関数を列挙し全てに同 signature を適用したか確認

`~/.claude/rules/common/testing.md` の sentinel pattern section (jj 操作コードの integration test pattern 内) に ADR-041 への inline link を追加し、cross-reference を整理:

- 「sentinel 事前投入パターン」の段落末尾に「詳細は [ADR-041 Test Isolation Patterns for Multi-Condition Guards](adr/adr-041-test-isolation-patterns.md) を参照」を追加 (派生プロジェクトでは相対 path 解決を考慮、`docs/adr/` が存在しないなら ADR cite を inline で完結させる)

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (`feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/patterns.md` に § Companion Helper Group の API Cohesion section 追加
- [ ] `~/.claude/rules/common/testing.md` の sentinel pattern section に ADR-041 cross-reference 追加
- [ ] PR description で順位 189 (lint rule) との補完関係を明記
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- companion helper group の API cohesion 原則が docs 層に codify
- testing.md ↔ ADR-041 の cross-reference が確立
- 派生プロジェクトに global rule として波及

#### 詰まっている箇所

なし。Effort XS、docs 編集のみ。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo9.md 末尾を参照。)
