# TODO (Part 10)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo9.md がファイルサイズ 50KB を超え行数 1100+ 行に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #185 = Bundle CR-RL land 後、2026-05-29 ユーザー判断)。todo.md / todo2.md 〜 todo9.md の既存エントリは引き続き有効、相互に独立。新セッションでは十二つすべてを確認すること (todo.md / todo2-11.md / todo-summary.md)。
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
- **grep target**: `docs/todo.md docs/todo2-11.md` 全件 (= 現在の todo file 集合、新 todoN+1.md 追加時は SKILL.md update が必要)

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

### Companion helper group 署名整合 compile-time validation test (PR #196 T2-1 採用)

> **動機**: Bundle 195-FB (PR #196) で `count_empty_in_pr_range` だけ `default_branch` 引数化が漏れていた問題 (CR Major + pre-push F-1) を rule⑫ で **literal hardcode 層** では機械検出するようになったが、companion helper group (`assert_descriptions_absent/present_in_pr_range` / `count_empty_in_pr_range` / 将来追加される helper) の **API signature 整合性** は lint rule では catch できない (= AST レベル complexity)。4 番目以降の helper 追加時に signature drift が発生しても rule⑫ は fire しない silent regression リスク。
>
> **本タスクの位置づけ**: PR #196 post-merge-feedback Tier 2 #2 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None、2026-06-05 ユーザー承認)。test-level validation で構造強制、Bundle 195-FB Layer 1 (rule⑫) + Layer 2 (parameterize) の seal 層として位置付け。analyzer は Tier 1 lint rule (item 1) を ROI 不釣合いとして却下推奨済、本 test approach は Tier 2 内 alternative。
>
> **参照**: `.claude/feedback-reports/196.md` Tier 2 #2、`src/cli-pr-monitor/src/fix_commit.rs` (test module 内 companion helper group)、PR #195 commit `9663dd68` (前 2 関数の修正)、PR #196 commit `qntnzyxt` (Layer 2 = 3 関数目の整合)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。compile-time witness (関数ポインタキャスト) で signature drift を test 不通過にする構造。

#### 設計決定 (案)

Rust の compile-time check で signature drift を検出する pattern:

```rust
#[test]
fn companion_helpers_share_default_branch_signature() {
    // Compile-time witness: 各 helper が (&Path, &str, ...) signature を取ることを強制。
    // 新 helper を group に追加した際は本 test の末尾に同型 cast を追加して compile-time
    // 整合性を seal する。signature が drift すると本 test が compile error で落ちる。
    let _: fn(&std::path::Path, &str, &[&str]) = assert_descriptions_absent_in_pr_range;
    let _: fn(&std::path::Path, &str, &[&str]) = assert_descriptions_present_in_pr_range;
    let _: fn(&std::path::Path, &str) -> usize = count_empty_in_pr_range;
}
```

- 関数ポインタへの cast は compile-time check (= test 関数 body 内の statement だが実行時 cost ≒ 0)
- signature drift → compile error → cargo test 不通過
- 新 helper 追加時の運用: companion group の prefix (`*_in_pr_range` 等) で命名一致するなら本 test に 1 行追加を **`code-review.md` § Review Checklist** で reviewer 注意喚起 (rule⑫ + 本 test + Reviewer 注意の 3 層防御)

#### 作業計画

- [ ] `src/cli-pr-monitor/src/fix_commit.rs` の `#[cfg(test)] mod tests` 内に `companion_helpers_share_default_branch_signature` test を追加
- [ ] `cargo test --bin cli-pr-monitor fix_commit::tests::companion_helpers_share_default_branch_signature` で pass 確認
- [ ] mutation regression check: 意図的に 1 関数の signature を変更 (例: `count_empty_in_pr_range(&Path) -> usize`) して compile error で落ちることを手動確認
- [ ] `~/.claude/rules/common/code-review.md` § Review Checklist の末尾に「companion helper group の signature 整合は compile-time witness test で seal、新 helper 追加時は test に 1 行追加」を 1 項目追加 (3 層防御の reviewer 喚起層)
- [ ] cargo clippy clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- compile-time witness test が `fix_commit.rs` test module に追加され pass
- signature 意図変更で compile error 観測 (dogfood)
- code-review.md § Review Checklist に reviewer 注意項目追加 (global rule、派生プロジェクト波及)

#### 詰まっている箇所

なし。Effort S、3-5 行 test 追加 + code-review.md 1 行追加で完結。

---

### development-workflow.md 「1. Plan First」に「task 着手前に grep で既存 section 確認」step 追記 (PR #196 T3-5 採用)

> **動機**: PR #196 pre-push reviewer OBS-1 で「tasks 191/192 が既実装 sections を再度計画対象としていた」と指摘 (実態は cleanup diff の誤読だが、similar pattern は PR #123 でも観測済で Frequency Medium)。task 計画段階で「対象 section が既に global rules / ADR に存在するか `grep` で確認する」step を `~/.claude/rules/common/development-workflow.md` "1. Plan First" に追記し、後続 task 計画時の redundant 提案を構造的に予防する。
>
> **本タスクの位置づけ**: PR #196 post-merge-feedback Tier 3 #5 採用 (Severity Medium / Frequency Medium / Effort XS / Adoption Risk None、2026-06-05 ユーザー承認)。development-workflow.md への 1-2 行追記のみ、派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及。
>
> **参照**: `.claude/feedback-reports/196.md` Tier 3 #5、PR #196 pre-push OBS-1 (`.takt/runs/20260605-054100-pre-push-review/`)、PR #123 同型事象 (analyzer report 内 cite)、memory `feedback_global_config_backup` (snapshot 必須)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule への 1-2 行追記。

#### 設計決定 (案)

`~/.claude/rules/common/development-workflow.md` の **Feature Implementation Workflow** "1. Plan First" sub-step に以下を追記:

```markdown
- **Codification 重複の事前確認**: 計画段階で「対象 section が既に global rules (`~/.claude/rules/common/*.md`) / ADR (`docs/adr/*.md`) / 既存 docs に存在するか」を `grep -n` で必ず確認する。重複追加は reviewer 混乱 + global rule の冗長化を招く。確認手順:
  - `grep -rn "<section title>" ~/.claude/rules/common/ docs/adr/ docs/`
  - hit があれば既存 codification を読み、追記 vs 新規 vs skip を判断
  - 由来: PR #123 / PR #196 で同型「既実装 section の重複計画」事象を観測
```

- **適用範囲**: 「ADR/global rule への新規 section codify」を含む全 task 計画
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/development-workflow.md` Feature Implementation Workflow "1. Plan First" sub-step に上記項目を追加
- [ ] PR #123 + #196 を実例として inline cite
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- development-workflow.md "1. Plan First" に Codification 重複確認 step が追記
- 派生プロジェクトに global rule として波及
- 由来 cite (PR #123, #196) で reviewer / Claude が rule 背景を理解可能

#### 詰まっている箇所

なし。Effort XS、docs 編集のみ。

---

### `hooks-session-start` orphan age 計算に proptest 追加 (PR #199 post-merge-feedback T2-1 採用)

> **動機**: PR #199 で `cli-pr-monitor::lock` に PastTime newtype + proptest properties を導入し、`saturating_sub` silent semantic mismatch (Finding D) を構造的に排除した。同じ bug class が `src/hooks-session-start/src/main.rs:236` の `now_unix.saturating_sub(start_unix)` にも存在し、clock rewind / future timestamp で `age=0` を返すと orphan reaper が「young」判定でスキップ → `.failed` marker 未生成 → ADR-030 L2 recovery 停止という具体的 failure chain が確認済。Bundle W の pattern (PastTime newtype or proptest properties) を `hooks-session-start` に展開し、同型 silent failure を防ぐ。
>
> **本タスクの位置づけ**: PR #199 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Medium / Effort M / Adoption Risk None、2026-06-08 ユーザー承認)。Bundle W の派生プロジェクト deploy 計画 (PR #199 PR description で「別 task として todo 登録予定」と明記済) のうち、本リポジトリ内の即時 deploy 先として最優先。
>
> **参照**: `.claude/feedback-reports/199.md` Tier 2 #1、PR #199 (Bundle W = `cli-pr-monitor::lock` の PastTime + proptest land)、`src/hooks-session-start/src/main.rs:236` (saturating_sub orphan age 計算)、ADR-030 (orphan reaper L2 recovery)、proptest 1.x ([dev-dependencies] 既存)
>
> **実行優先度**: 🔧 **Tier 2** — 工数 Medium。PastTime newtype 導入 + `parse_iso8601_to_unix` の proptest properties を Bundle W と同等構成で実装。proptest 依存は `cli-pr-monitor/Cargo.toml` 既存、新規 crate 依存追加は `hooks-session-start/Cargo.toml` のみ。

#### 背景

- `src/hooks-session-start/src/main.rs:236` で orphan age = `now_unix.saturating_sub(start_unix)` を計算しており、`start_unix > now_unix` (clock rewind / 破損 future timestamp) で age=0 を返す
- ADR-030 L2 recovery は orphan age が threshold を超えた場合に `.failed` marker を生成する設計。age=0 = "young" 扱いで marker 未生成 = recovery が機能しない silent failure
- Bundle W で `cli-pr-monitor::lock` に同型 fix を land 済 = pattern 移植コスト低
- proptest properties (P1-P5 と同等構成):
  - past_time_age_is_correct_when_in_past
  - past_time_rejects_future
  - parse_iso8601_to_unix_never_panics (任意 string で panic しない、cli-pr-monitor::lock との parser 共通化検討も別 task で)
  - parse_iso8601_to_unix_rejects_pre_epoch_year
  - parse_iso8601_to_unix_accepts_well_formed

#### 設計決定 (案)

- **アプローチ A**: PastTime newtype + proptest の **完全移植** (Bundle W と同等構成)
  - 利点: 設計一貫性、両 crate で同じ pattern
  - 欠点: hooks-session-start の orphan reaper 周辺 refactor scope が広がる
- **アプローチ B**: proptest properties **のみ追加** (saturating_sub は維持、bug を proptest で検出するだけ)
  - 利点: scope 最小、test 追加のみ
  - 欠点: silent fallback の型層防御は得られない (= properties が落ちたら手動で fix が必要、構造的防止にはならない)
- **アプローチ C**: hybrid (parse_iso8601_to_unix を共通 lib に extract + 両 crate で proptest + PastTime newtype は hooks-session-start にも導入)
  - 利点: parser 重複 (hooks-session-start / check-ci-coderabbit / cli-pr-monitor の 3 箇所) を解消し DRY 化、全 caller に防御
  - 欠点: lib crate 作成 + 派生プロジェクト deploy への影響範囲が大きい
- **MVP**: アプローチ A (Bundle W 完全移植) で 1 crate に閉じる。共通 lib 化 (アプローチ C) は別 task として todo 登録
- **派生プロジェクト deploy**: 本 task land 後、Bundle W deploy 計画と統合して techbook-ledger / auto-review-fix-vc に展開

#### 作業計画

- [ ] `src/hooks-session-start/Cargo.toml` の `[dev-dependencies]` に `proptest = "1"` を追加
- [ ] `src/hooks-session-start/src/main.rs` に PastTime newtype (Bundle W から portion 移植 or 共通設計を hooks-session-start に複製) を導入
- [ ] orphan age 計算経路 (line 236 周辺) を PastTime ベースに refactor、`saturating_sub` を排除
- [ ] proptest module を `#[cfg(test)]` に追加 (P1-P5 相当 5 件)
- [ ] 既存 orphan reaper test (`L1180` 周辺) との回帰確認
- [ ] cargo test + clippy clean を確認
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- `hooks-session-start` の orphan age 計算経路で `saturating_sub` が排除され、future timestamp が age=0 にならない invariant が型/proptest で保証される
- proptest properties が CI で pass、実行時間 +1 秒以内
- ADR-030 L2 recovery の failure chain (age=0 silent skip) が再発不可能であることを Bundle W PR description と同様に明記

#### 詰まっている箇所

- アプローチ C (parser 共通 lib 化) は `parse_iso8601_to_unix` が 3 crate で微妙に異なる実装 (`check-ci-coderabbit` は fractional seconds 対応、`cli-pr-monitor` は対応なし) のため統合コストが見えない。MVP (アプローチ A) で 1 crate に閉じ、共通 lib 化は別 task。

---

### ADR-NNN (採番未確定、land 時に確定): Timestamp invariant safety — 時刻計算 silent failure class の codify (PR #199 post-merge-feedback T3-2 採用)

> **動機**: PR #96 Finding D (`cli-pr-monitor::lock` の `parse_age_secs` で `saturating_sub` silent semantic mismatch) と PR #199 Bundle W (PastTime newtype + proptest で構造的予防) で同型 bug class が 2 件観測 (Frequency Medium)。本 ADR は「時刻計算における silent failure class と型レベル防御」を永続化し、派生プロジェクト (techbook-ledger / auto-review-fix-vc) への transferability を確保する。
>
> **本タスクの位置づけ**: PR #199 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort M / Adoption Risk None、2026-06-08 ユーザー承認)。順位 135 codified placeholder policy を適用し ADR 番号は land 時 PR で空き番号を確定する (本 entry 登録時点で ADR-038/039/040/041/042/043 占有済、044 が最有力候補だが land 時に再確認)。
>
> **参照**: `.claude/feedback-reports/199.md` Tier 3 #2、PR #96 Finding D、PR #199 Bundle W (PastTime newtype 実装 + proptest properties 5 件)、`~/.claude/rules/rust/patterns.md` § Newtype Pattern (extension 候補)、順位 135 (ADR 番号 hardcode 撤廃 policy)、順位 78 (旧 ADR-038 → 041 → NNN の 3 段振り直し実証)
>
> **実行優先度**: 💎 **Tier 3** — 工数 Medium。新規 ADR 1 件作成 (記述のみ、コード変更なし) + CLAUDE.md ADR list 追記。

#### 背景

- **Bug class の定義**: `saturating_sub(now, then)` 等の silent fallback が dominate ドメイン的に誤った値 (age=0) を返し、後段の判定で「fresh」「young」等の誤判定を生む
- **発生条件**: clock rewind (NTP 巻き戻し / VM snapshot restore) / 破損 future timestamp (corrupted lock file / 不正 input) / 時刻取得失敗 → silent fallback
- **防御原則**: 業務ロジック的に不可能な状態 (future timestamp の存在) を型層で unrepresentable にする。construction 時に invariant 検証、`age_secs()` 等の derived 値は invariant により安全に計算
- **実証パターン**: PR #199 Bundle W で `PastTime { epoch_secs, captured_now }` newtype + `from_iso8601_now` / `from_parts` 2 経路 + `age_secs()` non-negative invariant + proptest 5 properties で構造化

#### 設計決定 (案)

- **ADR title (案)**: 「Timestamp invariant safety — 時刻計算 silent failure class と型レベル防御」
- **ADR sections (案)**:
  1. **コンテキスト**: bug class 定義 + 観測実例 (PR #96 Finding D / PR #199 Bundle W)
  2. **決定**:
     - 原則 1: `saturating_sub` を時刻計算で使用しない (silent fallback 禁止)
     - 原則 2: 「過去性」を型で表現する (newtype + construction 時 invariant)
     - 原則 3: proptest properties で type invariant を executable contract として記述
  3. **設計哲学**: 「業務ロジック的に impossible な状態を型層で unrepresentable にする」(parse, don't validate 派生)
  4. **派生プロジェクト適用**: cli-pr-monitor (実装済) / hooks-session-start (順位 197 で実装予定) / 派生プロジェクトの時刻計算箇所 (検出→展開計画)
  5. **完了状態 / 関連 ADR**: PR #199 (実証)、ADR-021 / ADR-024 等の参照
- **CLAUDE.md ADR list**: 「ADR-NNN: Timestamp invariant safety + saturating_sub による silent fallback 禁止 *(試験運用)*」として追記

#### 作業計画

- [ ] land 時 PR で ADR 空き番号を確定 (現状最有力は ADR-044)
- [ ] `docs/adr/adr-NNN-timestamp-invariant-safety.md` を新規作成 (試験運用)
- [ ] CLAUDE.md ADR list に追記
- [ ] PR #96 Finding D / PR #199 Bundle W を実例として inline cite
- [ ] (任意) `~/.claude/rules/rust/patterns.md` § Newtype Pattern に link back を追記 (順位 T3-1 様子見と連動で判断)
- [ ] 本 todo10.md エントリを削除

#### 完了基準

- ADR-NNN が docs/adr/ に存在し試験運用 status で 1 PR で land
- CLAUDE.md ADR list に追記され ADR タイトル + 試験運用 marker が表示される
- bug class が以降の reviewer (人間 / CodeRabbit / takt facet) から ADR 参照で言及可能になる

#### 詰まっている箇所

- ADR 番号確定タイミング (land 時 PR) と他並走 entry (順位 78 等) の競合可能性。順位 135 placeholder policy に従い land 時 PR で grep 確認すれば構造的に解決
- `~/.claude/rules/rust/patterns.md` への展開 (順位 T3-1 が 様子見) との順序関係。本 ADR が land してから patterns.md 拡張を再評価する流れで矛盾なし

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo9.md 末尾を参照。)
