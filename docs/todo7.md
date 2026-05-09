# TODO (Part 7)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo5.md がファイルサイズ 67KB に到達して Claude Code の読み取り安定性 (50KB 超で不安定化) を損なったため、2026-05-09 に **PR #101〜#109 由来の古い半分のタスクを本ファイルへ分離** した。todo5.md には PR #110 以降のタスクが残存。本ファイルは既存タスクの編集・完了削除専用、新規タスクは追加しない (新規エントリは [docs/todo6.md](todo6.md) へ)。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md の既存エントリは引き続き有効、相互に独立。新セッションでは八つすべてを確認すること (todo.md / todo2-7.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### `>` vs `>=` boundary inconsistency lint rule (PR #101 T1-2)

> **動機**: PR #101 で `parse_listed_findings` の `created_at > push_time` が CodeRabbit から境界 inclusive (`>=`) への揃え修正を指摘された。auto-fix が同一ファイル内 `parse_new_comments` / `parse_findings` にも `>=` を適用 (= 3 関数 latent drift)。`parse_rate_limit` だけが既に `>=` で、後続関数を書くたびに著者が意識せず `>` を選ぶ構造的問題。custom-lint-rule で書いた瞬間に block すれば bug class が排除される。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 1 #2 採用 (高頻度 finding)。Bundle Z #B-α (Rust comment lint hook) と同じ「決定論的防止層」哲学。AST 解析ではなく正規表現層 (ADR-007) で対応可能。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 1 #2、ADR-007 (custom lint rule の正規表現 / AST 層線引き)、CodeRabbit PR #101 round 1 Minor finding
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。`.claude/custom-lint-rules.toml` への regex rule 追加。

#### 設計決定 (案)

- **配置先**: `.claude/custom-lint-rules.toml` に新規 rule entry
- **検出パターン (正規表現案)**:
  - 狭め: `\.(created_at|submitted_at|updated_at)\b.*\.map\(\|\w+\|\s*\w+\s*[><](?!=)\s*(push_time|since)`
  - 広め: `\b(created_at|submitted_at|updated_at|comment_event_time|event_time)\b.*[><](?!=)` で時刻フィールドの strict inequality 全般を flag
- **適用対象**: `.rs` ファイル
- **rule 名 (案)**: `time-boundary-strict-inequality`
- **suppress マーカー**: `// SAFETY: <理由>` 行末付与で suppression (例: 意図的に exclusive 比較する場合)

#### 作業計画

- [ ] 既存 `.claude/custom-lint-rules.toml` の rule 構造を確認
- [ ] regex + path filter を新 rule として記述
- [ ] PostToolUse hook の lint runner で synthetic test (修正前 `parse_findings` 系の `>` パターンを再現してマッチ確認)
- [ ] 既存 codebase で false positive 影響範囲をグレップして確認
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 確認
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- `.claude/custom-lint-rules.toml` に新 rule が追加され `.rs` ファイル内の時刻フィールド strict inequality を検出
- 1〜2 PR で dogfood し false positive がないこと

#### 詰まっている箇所

- false positive の評価 (時刻フィールド以外で legitimate な `>` が誤 block されないか)。着手時に実 codebase でグレップして影響範囲を確認。

---

### `parse_findings` 系の error-path test infrastructure (PR #101 T2-1) ★ Bundle a Sub-PR 2

> **動機**: PR #101 で `run_list_findings` が `unwrap_or_else(|_| "[]")` で gh api 失敗を `[]` に潰していて CR Major finding を受けた。99.md でも `silent fail` (Windows path mismatch で early return) として類似言及あり。**`unwrap_or_else(|_| empty)` の anti-pattern が複数 PR で再発**。test 層で機械検証することで未然に塞ぐ。本タスクは Bundle a Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) で同 API を消費するので、同一 PR land で test 二重投資なし。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 2 #1 採用 (高頻度 anti-pattern finding)。Bundle a Sub-PR 2 (順位 42 / 43 / 46) と同 PR で land 推奨。CLAUDE.md `coding-style.md` "Never silently swallow errors" 原則の test 層実装。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 2 #1、`.claude/feedback-reports/99.md`、`~/.claude/rules/common/coding-style.md` "Never silently swallow errors"
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。新 test ファイル + gh API モック。Sub-PR 2 と一体実装。

#### 設計決定 (案)

- **配置先**: `src/check-ci-coderabbit/tests/parse_error_handling_test.rs` (integration test、既存 unit test と分離)
- **テスト対象シナリオ**:
  - **gh API HTTP error 返却時**: `run_list_findings` がエラーを propagate するか verify (現状 PR #101 fix で `.map_err(...)?` 化済 → regression 防止)
  - **JSON 不正形式入力**: `serde_json::from_str` 失敗時の挙動 (現状 `unwrap_or_else(|e| { eprintln!(...); vec![] })` で warn は出すが空配列返却 = silent fall) — 望ましい設計を test で固定
  - **空 JSON `[]`**: 正常 path (空 findings 返却) の境界条件
- **モック戦略**:
  - gh API 直接モックは不要 (parse 関数は JSON string を受け取る純関数)
  - `run_gh` を trait 化して mock injection or `mockito` HTTP mock — Sub-PR 2 の cli-pr-monitor 実装方針と整合
- **既存 unit test との関係**: 既存 16 件は normal path 中心。本 task は error path 専用

#### 作業計画

- [ ] `src/check-ci-coderabbit/tests/` ディレクトリ作成 (現在 unit test only)
- [ ] gh API モック戦略の選定 (trait injection or shell wrapper stub) — Sub-PR 2 の cli-pr-monitor 実装方針と整合
- [ ] error-path シナリオ 3 件 (HTTP error / 不正 JSON / 空 JSON) を実装
- [ ] `cargo test --workspace` で pass 確認
- [ ] dogfood: 実 PR で `unwrap_or_else(|_| empty)` を一時的に書き戻して test が fail するか sensitivity 検証
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- `parse_listed_findings` / `parse_findings` の error-path 3 シナリオ test が pass
- `unwrap_or_else(|_| empty)` の silent fallback パターンが test で fail 検出される
- Sub-PR 2 の cli-pr-monitor 実装で同 mock infrastructure を流用できる

#### 詰まっている箇所

- gh API モック戦略の選定: HTTP mock library `mockito` vs `run_gh` の trait injection — 単純さ優先なら後者、real API 結合に近づけたいなら前者。
- `eprintln!` (stderr) を assert する仕組みが Rust 標準にないため、`gag::BufferRedirect` や custom logger 注入が必要 — 着手時に評価。

---

### `.takt/review-diff.txt` を fix→review iteration 間で refresh (PR #103 観測)

> **動機**: PR #103 push の実観測で takt pre-push-review が **6-iter outlier (22m 50s)** を発生させ、うち iter 3+4 の ~10 分が wasted。原因は `.takt/review-diff.txt` が push-runner 起動時 snapshot として固定され、fix step の変更が反映されないこと。reviewer は古い diff を読んで「fix されていない」と機械的 false positive (`persists`) を出し、max iter まで escalate して supervise の live Read で打開する以外に経路がない。supervisor 自身が "structural limit" として診断済 (`.takt/runs/20260503-113700-pre-push-review/reports/supervisor-validation.md`)。
>
> **本タスクの位置づけ**: PR #103 セッション知見 (post-merge-feedback の Tier 3 #1 = ADR 化提案を skip し、機構で塞ぐ実装層対策を採用)。Bundle Z 3 層 (#B-α / #B-β / #B-γ) では完全に塞げない独立改善。reviewer の判定精度を構造的に改善することで 6-iter outlier の発生率を 0% 近くに抑える。
>
> **参照**: `.claude/feedback-reports/103.md` (Tier 3 #1 で同根因に別アプローチ提案、本 task で代替)、`.takt/runs/20260503-113700-pre-push-review/reports/supervisor-validation.md` (false positive 構造診断)、[ADR-036: Bundle Z 3 層アーキテクチャ](adr/adr-036-bundle-z-three-layer-review.md) (PR #97 ベースライン observation を含む、本 task は Bundle Z 3 層では塞げない独立改善)
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。takt 設定 / pre-push-review.yaml への hook 追加。

#### 設計決定 (案)

- **refresh タイミング**: reviewer step 起動直前に diff を再生成 (fix step 完了直後の状態を反映)
- **実装方針 (2 案)**:
  - **案 A: takt workflow の reviewer step に precondition step を挟む** — `.takt/workflows/pre-push-review.yaml` で `before:` / `pre-step:` 的な hook を使い、push-runner と同一の diff 生成コマンドを呼ぶ
  - **案 B: cli-push-runner 側で fix step の終了を検出して diff を更新** — Rust コードで takt の step 進行を監視 (実装複雑度大)
- **推奨**: 案 A — takt config で完結、Rust 修正不要、影響範囲が pre-push-review.yaml のみ
- **diff 生成コマンド**: 既存 push-runner と同じロジック (`jj diff` ベース) を再利用、ファイルパス `.takt/review-diff.txt` も同一に保つ
- **冪等性**: 同 fix output から生成される diff は決定的なので複数回 refresh しても問題なし。途中失敗で diff が壊れても次 iteration の冒頭で上書きされる

#### 作業計画

- [ ] takt workflow の hook 仕様 (`before:` / `pre-step:`) を確認 (`.takt/workflows/*.yaml` の他 facets / takt source を grep)
- [ ] case A 不可なら case B (cli-push-runner 改修) にフォールバック
- [ ] `.takt/review-diff.txt` の生成ロジックを単一場所に整理 (DRY、push-runner と shared util にする等)
- [ ] `.takt/workflows/pre-push-review.yaml` に refresh hook を追加
- [ ] 単体動作確認: 意図的に DRY refactor 指摘 + fix を再現する synthetic シナリオで 3-iter 収束を確認
- [ ] dogfood 1〜2 PR で実 6-iter outlier scenario が再発しないことを観測
- [ ] Bundle Z Phase 2 (#B-β) との競合確認 (deterministic check は fix step 内部で動くため、本 task の fix→review 境界 refresh とは独立)
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 確認
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- fix step 完了後の review iteration で `.takt/review-diff.txt` が最新状態を反映
- 6-iter outlier の発生率が **0%** に近づく (PR #103 のような scenario が 3-iter で収束)
- supervisor の live Read 救済が不要になる (= supervisor step は workflow に残るが、false positive 救済責務が消える)

#### 詰まっている箇所

- takt workflow の `before:` / `pre-step:` hook 仕様が公式 docs に明記されていない可能性 → 着手時に takt source / 既存 workflow yaml を grep して確認。

---

### comment-lint hook の MultiEdit 対応 (順位 50 follow-up)

> **動機**: 順位 50 で comment-lint hook の scope を変更行に限定する v1 実装を完了した。v1 は Edit (single new_string) のみフィルタ対象とし、MultiEdit は whole-file lint にフォールバックする (no-regression)。MultiEdit が頻繁に使われる場合、複数 edit の `edits[].new_string` を順次適用して累積 range を計算する拡張が望ましい。
>
> **本タスクの位置づけ**: 順位 50 follow-up。MultiEdit 利用頻度が低いため優先度は Tier 3。MultiEdit 由来の 12.6KB 出力が無視できない頻度になった場合、または Bundle Z Phase 3 (#B-γ) で MultiEdit ベースの大規模リファクタが日常化した場合に着手。
>
> **参照**: 順位 50 PR (`src/hooks-post-tool-comment-lint-rust/src/main.rs` の `compute_changed_lines`)、Claude Code MultiEdit tool spec
>
> **実行優先度**: 💎 **Tier 3** — Effort S。`compute_changed_lines` に MultiEdit branch を追加。

#### 設計決定 (案)

- **MultiEdit input schema**: `tool_input.edits: Vec<{old_string, new_string, replace_all?}>` を順次適用
- **行 range 計算**: 各 edit の `new_string` を post-edit source 内で全件検索 → 全 edit の match 行 range の union を filter として使用
- **空 new_string の扱い**: 個別の edit が純削除の場合、その edit はスキップ。全 edit が純削除なら filter は空 = lint skip
- **fallback 条件**: ある edit の `new_string` が見つからない場合 → 安全側に倒し whole-file lint (現 Edit 実装と同じ動作)

#### 作業計画

- [ ] `ToolInput` struct に `edits: Option<Vec<EditEntry>>` を追加
- [ ] `compute_changed_lines` に `Some("MultiEdit")` branch を追加 (各 edit の new_string を locate して union)
- [ ] 単体テスト: 複数 edit の union が正しく計算されることを確認
- [ ] 単体テスト: 一部 edit が純削除の場合の挙動確認
- [ ] dogfood: MultiEdit を使った PR で hook 出力が変更行のみに絞られることを確認
- [ ] 派生プロジェクト deploy
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- MultiEdit でも変更行外の pre-existing violations が flag されない
- v1 (Edit) の挙動は不変
- Phase 3 (#B-γ) で reviewer の役割が「異常検知」に縮小されると本 task の効果も部分的に縮む可能性 (criterion-based finding がそもそも reviewer から消えるため)。ただし Phase 3 完了前の中間期間 + Phase 3 後も「異常検知」自体は diff を読むので効果は残る。

---


### comment-lint hook test 拡充 (PR #104 T2-1+T2-2 bundle)

> **動機**: PR #104 で CodeRabbit Critical (UTF-8 byte boundary) + Minor (multi-line block comment boundary) の 2 件を auto-fix で解消したが、いずれも回帰防止テストは 1 パターンのみで脆い。tree-sitter / Rust version 更新で区間交差判定や UTF-8 境界処理が壊れた場合に検出できないリスク。
>
> **本タスクの位置づけ**: PR #104 post-merge-feedback Tier 2-1 / Tier 2-2 の bundle。コスト低 (S effort)、test additions のみで scope clean、PR #104 の fix を体系的に固定化する。
>
> **参照**: `.claude/feedback-reports/104.md` Tier 2 #1, #2、PR #104 (`src/hooks-post-tool-comment-lint-rust/src/main.rs` の `locate_string_line_ranges` / `span_overlaps_ranges`)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。Bundle b と独立、いつでも単独着手可。

#### 設計決定 (案)

- **UTF-8 multi-byte test 拡充** (T2-1):
  - 現状: `locate_string_line_ranges_handles_multibyte_utf8` 1 パターン
  - 追加 5 パターン: 漢字 + ASCII 混合 / 漢字単独 / emoji / BMP 外文字 (例: 𝕊) / 結合文字 (例: é = e + ́)
  - 各パターンで `search_start = (absolute + needle.len()).min(source.len())` の境界処理を検証
- **Block comment boundary matrix 拡充** (T2-2):
  - 現状: `find_violations_multiline_block_comment_spanning_range_boundary` 1 パターン
  - 追加 6 パターン: {開始行のみ被覆, 終了行のみ被覆, 内部完全包含} × {単行 block comment, 複数行 block comment}
  - `span_overlaps_ranges(start, end, ranges)` の区間交差判定を体系化

#### 作業計画

- [ ] UTF-8 multi-byte test 5 パターン追加
- [ ] Block comment boundary test 6 パターン追加
- [ ] 既存 1 パターンずつのテストは保持 (regression 防止のため削除しない)
- [ ] 派生プロジェクト deploy は不要 (test のみのため)
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- UTF-8 multi-byte test が 6 パターン以上
- Block comment boundary test が 7 パターン以上
- `cargo test -p hooks-post-tool-comment-lint-rust` 全 pass

#### 詰まっている箇所

- 結合文字 (`e + ́`) を `new_string` に含むケースは Edit tool が実環境で発生するか不明 (理論的検証としては有効、実際の回帰防止としては効果薄の可能性)。1 パターンで足る

---

### Aggregation cap integration test (PR #105 T2-1 採用)

> **動機**: PR #105 の auto-fix で `collect_all_violations` に `violations.truncate(MAX_VIOLATIONS)` を追加した (CodeRabbit Minor finding 解消) が、これは contract の暗黙化に過ぎない。将来 `find_xxx_violations` を追加する PR で `extend()` の後に `truncate` を入れ忘れる regression を構造的に防ぐ test がない。
>
> **本タスクの位置づけ**: PR #105 post-merge-feedback Tier 2 #1 採用。後続の lint 追加 (例: 順位 56 の test 拡充 / 順位 47 の `>=` boundary lint / 将来の Rust 専用 lint) で同 contract を破る regression を test で固定化する。
>
> **参照**: `.claude/feedback-reports/105.md` Tier 2 #1、`src/hooks-post-tool-comment-lint-rust/src/main.rs` `collect_all_violations` (line 545)、PR #105 Finding #2 (Minor) の auto-fix
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。test 1-2 件追加で完結。

#### 設計決定 (案)

- **シナリオ**: `collect_all_violations(file_path, source_with_15_comments_and_15_long_functions, None)` を呼び、結果が **MAX_VIOLATIONS (= 20) 以下** であることを assert
- **source 構築**:
  - 15 個の禁止コメント (`// forbidden 0` 〜 `// forbidden 14`)
  - 15 個の 60 行関数 (`fn big_0` 〜 `fn big_14`)
  - 合計 30 件の violation 候補 → cap で 20 件に truncate
- **test 名**: `collect_all_violations_truncates_to_max_violations` (spec を test 名に反映、PR #105 T2-3 提案は卻下したが naming-as-spec 自体は意義あり)
- **追加検証** (任意): 個別 `find_violations` / `find_function_length_violations` がそれぞれ 20 件以上返しうることも assert (truncate なしだと 30 件返ることを示す)

#### 作業計画

- [ ] 30 件の violation 候補を含む synthetic source を生成する helper 関数を test module に追加
- [ ] `collect_all_violations_truncates_to_max_violations` test を追加
- [ ] 個別 finder の non-truncate 挙動を assert する補助 test を追加
- [ ] cargo test pass 確認
- [ ] 派生プロジェクト deploy は不要 (test のみ)
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- 結合後の violation 件数が `MAX_VIOLATIONS` 以下であることが test で固定化
- 将来 `find_xxx_violations` を追加した PR で truncate 削除すると test fail で検出される

#### 詰まっている箇所

- 順位 56 (PR #104 T2-1+T2-2 test 拡充) と同 PR で bundle するか別 PR とするか。両者とも test additions、同ファイル同 test module で scope clean、bundle 推奨。

---

### analyze-session の transcript filter 絞り込み (旧 #A-3)

> **動機**: `cli-merge-pipeline` が生成する `.takt/post-merge-feedback-transcript.jsonl` は **session 全履歴** を含むため、analyze-session step が読み込む input token が大きい。当該 PR に直接関連する範囲のみ filter すれば input token 削減 = post-merge-feedback の cache_read 削減。
>
> **本タスクの位置づけ**: 旧 `docs/pipeline-token-efficiency.md` の #A-3 entry。同計画書は ADR-036 (Bundle Z 3 層) / ADR-037 (fix-trust shortcut) に主要決定を移し終了予定で、残作業として本 task のみ todo に移管。Bundle 化対象なし、独立 PR 推奨。
>
> **参照**: (削除済) `docs/pipeline-token-efficiency.md` #A-3 セクション、`src/cli-merge-pipeline/` の transcript 生成ロジック
>
> **実行優先度**: 💎 **Tier 3** — Effort M。ROI ★★★ で優先度中程度、dogfood 実測が必要。

#### 設計決定 (案)

- **filter 範囲**: 当該 PR の作成 commit (= cli-pr-monitor が PR を最初に検出した時刻、または `pnpm create-pr` 完了時刻) から merge 完了時刻までの jsonl 行のみ
- **時刻判定**: jsonl の `timestamp` field を使用 (各エントリに ISO 8601 形式で記録あり)
- **境界の扱い**:
  - 開始時刻 *以降*: PR 作業中の Claude 対話 + tool 実行履歴
  - 終了時刻 *まで*: merge 完了 (= post-merge-feedback 起動の直前まで)
  - 境界外 (PR 作成前 / merge 後): 除外
- **既存挙動との互換**: 開始時刻取得失敗時 (state file なし等) は全 session フォールバック (no-regression)

#### 作業計画

- [ ] `cli-merge-pipeline` の transcript 生成ロジックを特定
- [ ] PR 作成時刻 / merge 時刻の取得経路を確定 (`.claude/cli-pr-monitor-state.json` or `gh pr view --json mergedAt` 等)
- [ ] timestamp 比較で jsonl 行を filter する logic を実装
- [ ] 開始時刻取得失敗時のフォールバック (全 session) を保持
- [ ] dogfood 1-2 PR で input token 削減量を実測 (analyze-session の billable input tokens で比較)
- [ ] 削減効果が想定 30-50% に届くか確認、届かない場合は filter 設計を見直し
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- analyze-session の input token が PR 作業範囲のみに絞り込まれる
- dogfood で 30-50% 削減を実測 (削減未達なら filter 設計を見直し)
- 開始時刻取得失敗時のフォールバックが機能 (regression なし)

#### 詰まっている箇所

- 「PR 作成前の議論 (設計判断、却下されたアイデア)」が落ちる可能性 → post-merge-feedback の知見質に影響しうる。dogfood で「重要 finding が拾えなくなった」事象が出たら filter 範囲を広げる (例: PR 作成 commit から 2 時間前まで遡る等)
- transcript jsonl の structure 変更時に filter logic が壊れる risk → field name (`timestamp`) を assert する unit test を追加

---

### post-PR 検証フローに CR review.body 手動スキャン step 追加 (PR #108 T2-1 採用)

> **動機**: PR #108 で CodeRabbit が `Outside diff range comment` として review body 内に投稿した Minor finding (`docs/todo4.md` line 371/378 の retire 済前提と旧フロー混在) を、takt の `analyze-coderabbit` step が検出漏れした。`analyze-coderabbit` は `pulls/N/comments` (= inline review comment) ベースで動作するため、review.body 内のコメントは parse 対象外。結果、PR #108 で line 371/378 の修正が merge 後 follow-up commit (`vokyspww`) になった。
>
> **本タスクの位置づけ**: PR #108 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None / ✅ 採用)。`analyze-coderabbit` の根本解決 (review.body 解析対応) は別 task として実装複雑度が高いため、暫定緩和策として **手動 checklist** で対応する。Tier 1 の analyzer 拡張 (= 将来の根本解決) の先行策として機能する。
>
> **参照**: `.claude/feedback-reports/108.md` Tier 2 #1、PR #108 review (`Outside diff range comments` セクション、reviewer comment id 4217897113)、`.takt/facets/instructions/analyze-coderabbit.md`
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。post-PR checklist documentation の更新のみ。

#### 設計決定 (案)

- **配置先候補**:
  - `docs/workflow.md` (新規 or 既存): post-PR checklist として統一記述
  - `~/.claude/rules/common/git-workflow.md`: 既存 PR workflow ルールに追記
  - 着手時に既存 docs 配置を grep して整合する場所を選定
- **追加する checklist 項目** (案):
  - `pnpm create-pr` 完了後 / takt post-pr-review 完了後に、CodeRabbit の review (= `Outside diff range comments` 含む全 review body) を手動で目視確認する
  - `gh api repos/{owner}/{repo}/pulls/{N}/reviews --jq '.[].body'` で review body を抽出して読む
  - 確認対象: `Outside diff range comments` セクション、`Caution` / `Warning` セクション、行番号参照のある comment 全般
- **検出時の対応**: 該当 finding を inline thread と同じく severity 評価 → 修正 commit を追加 → 手動で acknowledge reply
- **将来対応**: takt analyze-coderabbit に review body parse を追加 (= Tier 1 task として別 entry が必要、本 task の dogfood で頻度が高ければ昇格)

#### 作業計画

- [ ] `docs/workflow.md` または `~/.claude/rules/common/git-workflow.md` の現状を確認、追記場所を選定
- [ ] post-PR checklist 項目を追記 (gh api コマンド + 確認対象 + 検出時対応の 3 項目)
- [ ] dogfood: 次の数 PR で本 checklist を実行、blind spot 検出頻度を観測
- [ ] 観測結果に応じて Tier 1 へ昇格判断 (= analyzer 拡張)
- [ ] 派生プロジェクト deploy 不要 (本リポジトリ workflow 固有)
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- post-PR workflow に「CR review.body 手動スキャン」step が追記される
- 次 1-2 PR の dogfood で本 checklist の実行が観察される
- review body 内の actionable finding が後追い修正にならない (= merge 前に検出される)

#### 詰まっている箇所

- 配置先選定 (本リポジトリ docs/workflow.md vs グローバル `~/.claude/rules/`)。本タスクは本リポジトリ固有の暫定緩和策のため、本リポジトリ docs/ への追記が妥当か
- 手動 checklist は持続性が低い (人間が忘れる) ため、Tier 1 への昇格 (= analyzer 拡張) の優先度判断が dogfood 結果に依存

---

### cli-merge-pipeline に Drop guard / signal handler を追加し abrupt 終了時に `.failed` marker を保証 (PR #109 T1-1 採用) ★ Bundle c

> **動機**: PR #109 merge 直後の post-merge-feedback workflow が SIGPIPE で silent 中断され、`.takt/runs/.../reports/` が空 + `.claude/feedback-reports/109.md` 未生成 + `.failed` marker も無いという fail mode が実証された。原因は `feedback::run()` が `Result::Err` を返した場合のみ `write_failed_marker` を書く実装で、Rust default の SIGPIPE 動作 (parent process abrupt 終了) では Result::Err 経路に到達しない。ADR-030「失敗マーカーによる recovery」仕様を構造的に違反。
>
> **本タスクの位置づけ**: Bundle c (PR #109 post-merge-feedback 堅牢化) の中核。Drop guard で `Result::Err` 経路に依存しない unconditional marker 書き出しを保証する。Pre-emptive marker (案 C) と signal trap (案 A) の組み合わせで abrupt 経路を多層防御。
>
> **参照**: `.claude/feedback-reports/109.md` Tier 1 #1、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)、`src/cli-merge-pipeline/src/feedback.rs:454-475` (`copy_feedback_report`) / `:1100-1180` (`run`) / `main.rs:555` (caller)
>
> **実行優先度**: 🚀 **Tier 1 Critical** — Effort M。仕様 (ADR-030) と実装の根本ギャップ閉鎖。

#### 設計決定 (案)

- **修正方針**: Explore agent が提示した 3 案 (A: signal trap + Drop guard / B: thread + parent timeout / C: pre-emptive marker) のうち、**A + C の組み合わせ** を採用 (agent 推奨)
  - **C (pre-emptive marker)**: `feedback::run` 呼び出し前に `.failed` marker を先制書き込み、正常完了時のみ削除。abrupt 終了の 99% を救済 (Effort XS-S)
  - **A (signal trap + Drop guard)**: `tokio::signal` または `nix` crate で SIGPIPE/SIGTERM を trap、RAII Drop guard で marker 書き込みを保証。panic 経路もカバー (Effort M)
- **race 対策**: 同 PR で concurrent merge が走った場合の race は既存 `CONCURRENT_RUN_GUARD_SECS=1500s` で予防されるが、pre-emptive marker の lifecycle と整合性確認が必要
- **OS 互換性**: signal handling は OS 依存。Windows では SIGPIPE 相当が無いため Ctrl+C / SIGTERM 経路を中心に対応。Unix と Windows のコードパス分岐は cfg gate で実装

#### 作業計画

- [ ] `src/cli-merge-pipeline/src/feedback.rs` に pre-emptive marker 書き出しを追加 (`run` 冒頭で `write_failed_marker(reason: "pending")`)
- [ ] 正常完了時に marker を削除する path を追加 (`copy_feedback_report` 成功後)
- [ ] `nix` または `tokio::signal` で SIGPIPE/SIGTERM trap を実装 (Unix) + Windows 用 cfg 分岐
- [ ] RAII Drop guard 構造体を導入し、scope 終了時に marker 書き込みを保証 (正常時 `disarm()` で skip)
- [ ] 既存 `Result::Err` 経路の `write_failed_marker` 呼び出しは維持 (二重書きにならないよう pre-emptive marker と統合)
- [ ] dogfood: 本機能を有効にした状態で `cli-merge-pipeline.exe \| head -40` を実行し marker が残ることを確認 (今回事故の再現テスト)
- [ ] 派生プロジェクトに deploy (cli-merge-pipeline.exe を再ビルド + 配布)
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- SIGPIPE / SIGTERM / panic / Result::Err いずれの経路でも `.claude/feedback-reports/<PR>.md.failed` が必ず残る
- 正常完了時には `.failed` marker が残らない (false positive ゼロ)
- 今回の事故 (PR #109 SIGPIPE) を再現するテストで pass
- 派生プロジェクト (`techbook-ledger` / `auto-review-fix-vc`) でも同等動作

#### 詰まっている箇所

- Windows での SIGPIPE 相当の挙動: Rust std はデフォルト SIGPIPE handler を install するが、Windows では pipe broken 時の挙動が異なる (CTRL_BREAK / I/O error)。整合性確保のため OS 別の signal mapping 設計が必要
- 順位 64 (orphan reaper) との責務分離: Drop guard は process 内、reaper は process 外。両者の trigger 条件が重複しないよう設計

---

### orphan run reaper (post-merge-feedback の `meta.json status=running` 放置検出 + 自動再起動) (PR #109 T1-2 採用) ★ Bundle c

> **動機**: 順位 63 (Drop guard) では救済できない致命系 (kill -9 / SIGKILL / power loss / OOM Killer) で post-merge-feedback workflow が中断された場合、`.failed` marker も書かれず orphan run のみが残る。仕様 (= フィードバックは必ず実行) を保証するには process 外からの監視層が必要。
>
> **本タスクの位置づけ**: Bundle c (PR #109 post-merge-feedback 堅牢化) の第二防衛層。Drop guard (順位 63) を内側、reaper を外側とする多層防御で「フィードバックは必ず実行する」仕様を multi-layer で保証。
>
> **参照**: `.claude/feedback-reports/109.md` Tier 1 #2、[ADR-029](adr/adr-029-post-merge-feedback-auto-trigger.md) (pending file 経由の再起動)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)
>
> **実行優先度**: 🚀 **Tier 1 Critical** — Effort M。順位 63 と組み合わせて致命系 hole を塞ぐ。

#### 設計決定 (案)

- **配置先候補** (着手時に決定):
  - **案 A**: `cli-pr-monitor` 起動時に `.takt/runs/*/meta.json` を scan (既存 monitor 機構との整合性高い)
  - **案 B**: SessionStart hook (`src/hooks-session-start*/`) で scan (Claude Code session 起動毎に走る確定的 trigger)
  - 推奨: **案 B** (SessionStart) — cli-pr-monitor は backend daemon 廃止 (ADR-018) で takt 経由になっており trigger 機構が複雑、SessionStart は単純で確実
- **検出条件**:
  - `.takt/runs/*/meta.json` の `status: "running"` かつ `startTime` が現時刻から **5 分以上経過**
  - `currentStep` が `analyze` のまま (= 1 step も完了していない極短時間で死んだケース) も含める
- **recovery 動作**:
  - 検出した orphan run の `meta.json` を `status: "failed"` に更新 (アトミックに)
  - `.claude/feedback-reports/<PR>.md.failed` marker を書く (PR 番号は run slug `post-merge-feedback-for-<N>` から抽出)
  - ADR-029 pending file (`.claude/post-merge-feedback-pending.json`) を生成し、UserPromptSubmit hook で再起動 trigger
- **冪等性**: 同 orphan を 2 回検出しても重複 trigger しないよう既存 marker / pending file を check

#### 作業計画

- [ ] 配置先 (案 A / B) を grep + `.claude/hooks-config.toml` 確認のうえ決定
- [ ] `meta.json` parser + 5 分閾値判定ロジック実装
- [ ] `.failed` marker 書き出し + pending file 生成ロジック実装
- [ ] 冪等性 guard (既存 marker / pending file 検出時の skip)
- [ ] integration test: 人為的に orphan meta.json を作成して reaper が再起動 trigger することを assert
- [ ] dogfood: 既存の orphan (`.takt/runs/20260504-101353-post-merge-feedback-for-109/`) を fixture として retroactive detection 確認
- [ ] 派生プロジェクトに deploy
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- kill -9 / power loss シミュレート (forcibly kill) で `.failed` marker と pending file が遅延生成される
- Drop guard (順位 63) が機能している正常 case では reaper が誤検出しない (false positive ゼロ)
- 既存 orphan (PR #109 のもの) を retroactive に処理できる
- 仕様レベル: 「post-merge-feedback はマージ後 5 分以内に必ず完了 or 失敗 marker 化される」が AppCenter 級の SLA で保証される

#### 詰まっている箇所

- SessionStart hook の発火頻度: 1 session 1 回しか走らないと、長時間 session 中に orphan が発生しても拾えない。`cli-pr-monitor` 経路と組み合わせるか、SessionStart + UserPromptSubmit の二段階検出が必要か検討
- 5 分閾値の妥当性: takt の analyze step は最大 5-10 分かかる場合あり。閾値を 5 分にすると進行中の正常 run を誤検出するリスク。10-15 分が妥当か

---

### exe + `--help` を PreToolUse でブロックして `src/<exe-name>/` Read に誘導する hook (PR #109 T1-3 採用) ★ Bundle c

> **動機**: PR #109 SIGPIPE 事故の **直接トリガ** が「AI が `cli-merge-pipeline.exe --help` を実行 → 当該 exe は `--help` 未対応のため merge 本体を実行 → 出力 truncate で SIGPIPE」だった。ユーザー提案: exe ごとに `--help` を実装する案は exe 数増加で漏れが出るが、`exe + --help` をセットで PreToolUse block すればソース閲覧フローに自動誘導でき、想定外実行を構造的に排除。今後追加される exe にも自動適用される一般解。
>
> **本タスクの位置づけ**: Bundle c (PR #109 post-merge-feedback 堅牢化) の trigger pattern 防止層。順位 63 / 64 が「中断されても recovery する」事後対策、本 task は「中断パターンを発生させない」事前対策。
>
> **参照**: `.claude/feedback-reports/109.md` Tier 1 #3、`.claude/hooks-config.toml` (PreToolUse block_pattern)、`src/hooks-pre-tool-validate*/`
>
> **実行優先度**: 🚀 **Tier 1 High** — Effort S。ユーザー提案の事前防衛策。

#### 設計決定 (案)

- **検出パターン** (regex):
  - `(?:\.\\.claude\\|\\./|^|\s)(?:[\w\-]+\.exe|cli-[\w\-]+\.exe)\s+(?:--help|-h|/\?)\b`
  - exe 名は `cli-*.exe` / `hooks-*.exe` / `check-*.exe` 等を含む全 Rust exe 想定
  - 引数が `--help` / `-h` / `/?` の **単独実行** に限定 (`exe foo --help` のような subcommand help は対象外)
- **ブロック時の代替誘導メッセージ**:
  ```text
  exe の --help は本リポジトリで未実装の可能性があります。
  詳細を見るには次を試してください:
    - ソースを Read: src/<exe-name>/src/main.rs (引数定義は clap struct を確認)
    - 既存 docs を検索: grep -r "<exe-name>" docs/
  ```
- **配置先**: `src/hooks-pre-tool-validate*/` の Bash command validation ロジック (既存 git block と同居)
- **適用範囲**: Bash tool での実行のみ。Read tool 等での参照は対象外

#### 作業計画

- [ ] 既存 `hooks-pre-tool-validate*` の構造を確認 (Rust exe か Python か)
- [ ] regex パターン設計 + テストケース作成 (TP / FP の境界明確化)
  - TP: `cli-merge-pipeline.exe --help`、`./.claude/foo.exe -h`
  - FP: `cargo run --help`、`gh pr view --help`、`exe foo --help` (subcommand)
- [ ] hook に block ロジック追加 + 代替誘導メッセージ実装
- [ ] integration test: 上記 TP / FP ケースで block / pass を assert
- [ ] dogfood: 本 hook 有効状態で `cli-merge-pipeline.exe --help` を実行し block されることを確認
- [ ] 派生プロジェクトに deploy
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- `exe + --help` 系コマンドが Bash tool 経由で block される
- block メッセージで「ソースを Read」フローに誘導される
- 既存の正規 `--help` (cargo / gh / pnpm 等) は誤検出しない
- 今後追加される exe にも自動適用される (regex で exe 名を限定列挙しないため)

#### 詰まっている箇所

- regex の精度: `cli-merge-pipeline.exe` は対象だが `cargo --help` は対象外、という線引きを `.exe` suffix の有無で判定するか exe 名 prefix で判定するかで挙動が変わる。本リポジトリの全 exe を grep して命名規則を確認してから決定
- AI 側の挙動学習: block されたとき AI が代替フロー (ソース Read) に正しく遷移するかの dogfood 観察。失敗するなら block message を強化

---

### 長時間 subprocess の pipe truncate 禁止ルールをグローバル明文化 (PR #109 T3-1 採用) ★ Bundle c

> **動機**: PR #109 SIGPIPE 事故は「AI が長時間 subprocess (cli-merge-pipeline) の出力を `\| head -40` で truncate」したのが直接トリガ。順位 65 (PreToolUse block) が決定論層、本ルールは AI/人間の判断ガイド層。二層防御で hole を減らす。
>
> **本タスクの位置づけ**: Bundle c (PR #109 post-merge-feedback 堅牢化) の知識層。決定論的 block では捕捉しきれないパターン (例: `pnpm push \| tail`、`gh pr view --json reviews \| jq`) も含めて AI に教育的に指示。
>
> **参照**: `.claude/feedback-reports/109.md` Tier 3 #5、`~/.claude/rules/common/development-workflow.md`、`~/.claude/rules/common/git-workflow.md`
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。グローバルルール 1 セクション追加。

#### 設計決定 (案)

- **配置先候補** (着手時に決定):
  - **案 A**: `~/.claude/rules/common/development-workflow.md` の "Bash 実行ガイド" として新セクション追加
  - **案 B**: `~/.claude/rules/common/git-workflow.md` の "gh CLI 使用規則" の隣に "長時間 subprocess の出力扱い" 節を追加
  - 推奨: **案 A** (development-workflow が development pipeline 全般を扱うため整合性高い)
- **記述内容** (案):
  - 長時間 subprocess (`pnpm push` / `pnpm merge-pr` / `cli-*.exe` / takt workflow) を **`\| head` / `\| tail` / `\| tee` で truncate しない**
  - 理由: parent process の SIGPIPE で workflow が abrupt 中断され、`.failed` marker や成果物が silent loss する (PR #109 で実証)
  - 代替策: 出力をファイルに redirect (`> out.log 2>&1`) または `run_in_background` で実行 (Bash tool のオプション) し、後から `tail out.log` 等で確認
  - 例外: 短命な subprocess (`ls`, `cat` 等) や exit code のみが必要な場合は OK
- **既存ルールとの関係**: gh CLI 使用規則 (token 効率) と相補。token 効率は --jq / -q による絞り込み、本ルールは長時間 process の中断回避

#### 作業計画

- [ ] 案 A / B のどちらを採用するか決定 (着手時に grep で類似 rule の配置を確認)
- [ ] 配置先に「長時間 subprocess の出力扱い」セクションを追加 (規則 + 理由 + 代替策 + 例外を 1 ページに集約)
- [ ] PR #109 SIGPIPE 事故を実例として inline 引用 (`docs/adr/adr-030-...md` 参照)
- [ ] 派生プロジェクトで global rule 反映を確認 (rule は global、自動適用)
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- グローバルルールに「長時間 subprocess の pipe truncate 禁止」が codify される
- 次回 AI が `pnpm push \| head` 系を打とうとした時、ルール参照で自己修正できる
- 順位 65 (block_pattern) と整合 (二層防御の上層 = ガイド、下層 = block)

#### 詰まっている箇所

- 「長時間」の定義 ambiguity: `gh pr view` は通常短命だが rate-limit 中は長時間化する。閾値を秒数で明文化するか、特定 exe を列挙するかの判断
- 例外列挙の網羅性: AI が「これは例外だろう」と自己判断する余地を残すと block_pattern (順位 65) との整合性が崩れる可能性

---

### ADR-030 に abrupt 終了時の振る舞いを spec として明記 (PR #109 T3-2 採用) ★ Bundle c

> **動機**: PR #109 で露呈した「ADR-030 の決定論性が SIGPIPE / kill -9 / power loss で破綻する」問題は、ADR 本文で abrupt 終了時の挙動が **spec として明記されていなかった** ことが根本原因。順位 63 / 64 の実装が ADR-030 の "決定論的" の真の意味を closure する形で land する以上、ADR 本文も同タイミングで spec を拡張する必要がある。
>
> **本タスクの位置づけ**: Bundle c (PR #109 post-merge-feedback 堅牢化) の仕様層。順位 63 / 64 の実装と同 PR で land して仕様/実装の整合性を保つ (実装単独で spec ドリフトしない)。
>
> **参照**: `.claude/feedback-reports/109.md` Tier 3 #6、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (試験運用)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。ADR 本文の "失敗マーカーによる recovery" 節を拡張。

#### 設計決定 (案)

- **拡張する節**: ADR-030 の "失敗マーカーによる recovery" を「abrupt 終了 + reaper による多層保証」に拡張
- **追記内容** (案):
  - **L1 (in-process)**: Drop guard / signal trap で `Result::Err` 経路に依存せず `.failed` marker を保証 (順位 63 で実装)
  - **L2 (out-of-process)**: orphan run reaper で `meta.json status=running` 5-15 分放置を検出し marker 補完 + 再起動 (順位 64 で実装)
  - **致命系の挙動明記**: kill -9 / SIGKILL / power loss / OOM Killer → L1 で救済不可、L2 で救済
  - **仕様の SLA 化**: 「post-merge-feedback はマージ後 N 分以内に必ず完了 or .failed marker 化される」を保証ステートメントとして記述
- **試験運用フラグの扱い**: 順位 63 / 64 land 後、本 ADR の "試験運用" フラグを外すか継続するかは dogfood 結果次第。本 task では仕様明記のみ、フラグ判断は別途
- **関連 ADR との cross-link**: ADR-029 (pending file 自動起動) との関係明記、L2 reaper が ADR-029 経路を再利用する旨

#### 作業計画

- [ ] `docs/adr/adr-030-deterministic-post-merge-feedback.md` を読み、現行の "失敗マーカーによる recovery" 節を確認
- [ ] 拡張内容を起草 (L1/L2 の責務分離 + SLA 化 + cross-link)
- [ ] 順位 63 / 64 と同 PR で land する前提で実装と整合
- [ ] CLAUDE.md ADR index の ADR-030 description (試験運用フラグ等) も必要なら更新
- [ ] 本 todo7.md エントリを削除

#### 完了基準

- ADR-030 本文に L1 (in-process Drop guard) + L2 (out-of-process reaper) の責務分離が記述される
- abrupt 終了 (SIGPIPE / kill -9 / power loss / OOM) 時の挙動が spec として明記される
- post-merge-feedback の SLA (= マージ後 N 分以内に完了 or marker 化) がステートメントとして残る

#### 詰まっている箇所

- 試験運用フラグの去就: 順位 63 / 64 で実装が完成しても、dogfood 期間が必要なら試験運用フラグは残す。本 task では仕様明記のみだが、フラグ判断と整合性を取る必要あり
- SLA の妥当性: 順位 64 の閾値 (5-15 分) と同期する必要があり、閾値が決まらないと SLA も書けない (依存関係)

---
