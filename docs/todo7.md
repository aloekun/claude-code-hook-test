# TODO (Part 7)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo5.md がファイルサイズ 67KB に到達して Claude Code の読み取り安定性 (50KB 超で不安定化) を損なったため、2026-05-09 に **PR #101〜#109 由来の古い半分のタスクを本ファイルへ分離** した。todo5.md には PR #110 以降のタスクが残存。本ファイルは既存タスクの編集・完了削除専用、新規タスクは追加しない (新規エントリは [docs/todo6.md](todo6.md) へ)。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo6.md の既存エントリは引き続き有効、相互に独立。新セッションでは八つすべてを確認すること (todo.md / todo2-7.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

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

#### 設計決定 (D-6 セッションで確定、2026-05-13)

- **refresh タイミング**: fix step が `convergence_verdict` を emit する直前に refresh (= 次 reviewer iteration が読み始める時点で post-fix 状態)
- **実装方針 (3 案を評価)**:
  - **案 A: takt workflow の reviewer step に precondition step を挟む** — ❌ **不可**。takt v0.35.3 schema (`PieceMovement` / `PieceConfig`) を確認した結果、per-step `before:` / `pre-step:` / `hooks:` field は存在しない。piece レベルの `runtime.prepare` は workflow 開始時 1 回のみ実行され、step 間に挟まらない (`node_modules/.pnpm/takt@0.35.3/node_modules/takt/dist/core/models/piece-types.d.ts` Line 74-98 / `runtime-environment.js` Line 171-191)
  - **案 B: cli-push-runner 側で fix step の終了を検出して diff を更新** — ❌ **scope 不適合**。`stages/takt.rs` は `run_cmd_inherit` で takt を spawn-and-wait するのみ。filesystem watcher で `.takt/runs/<latest>/reports/*.md` の生成を監視する案は ~100-200 行の Rust + race condition 対応が必要で、AI-driven 案で塞げる範囲を超える複雑度
  - **案 C: fix.md instruction に "Pre-completion diff refresh" section を追加** — ✅ **採用**。既存の Bundle Z #B-β `Pre-completion deterministic check (Bundle Z Phase 2 / #B-β)` と同形の precedent あり (`scripts/fix-metrics-check.ps1` を Bash 呼び出しする pattern)。失敗 mode (= AI が refresh を skip) は現状と同等 (no regression)
- **採用案 C の実装**: `.takt/facets/instructions/fix.md` に「Pre-completion diff refresh (REQUIRED)」section を追加 (advisor 推奨)。`jj diff -r @ > .takt/review-diff.txt` を `convergence_verdict` emit 直前に必須実行
- **共有 instruction の影響**: `fix.md` は pre-push-review.yaml と post-pr-review.yaml の両方で使用される。post-pr-review は `.takt/review-diff.txt` を読まないが、refresh は冪等で副作用なし (~1s 程度の `jj diff` invocation cost のみ)
- **派生プロジェクト deploy**: `scripts/deploy-hooks.ts` は exe + `settings.local.json` のみ転送し、`.takt/facets/instructions/*` は派生 (techbook-ledger / auto-review-fix-vc) 各自が管理。よって本変更の自動 propagate は不要 (手動 port が必要だが scope 外、follow-up task)

#### 作業計画

- [x] takt workflow の hook 仕様を確認 → 案 A 不可と確定
- [x] cli-push-runner の takt invocation 構造を確認 → 案 B も scope 不適合と確定
- [x] advisor に方針相談 → 案 C (instruction-level) 採用 + 共有 instruction 影響 / deploy 経路を検証
- [x] `.takt/facets/instructions/fix.md` に「Pre-completion diff refresh (REQUIRED)」section を追加
- [ ] dogfood: D-6 PR push 自体で本 instruction が機能するかを観察 (fix step が refresh を実行 → 次 reviewer iter が post-fix 状態を読むか)
- [ ] dogfood 1〜2 PR で実 6-iter outlier scenario が再発しないことを観測
- [x] Bundle Z #B-β との競合確認: `fix-metrics-check.ps1` invocation は fix step 内部の Bash 実行で完結し、本 task の diff refresh は同 fix step の最終段 Bash 実行で独立。両者は時系列で順序通り走り競合なし
- [ ] 本 todo7.md エントリを削除 (PR D-6 merge + 1-2 PR の dogfood 完了後)

#### 完了基準

- fix step 完了後の review iteration で `.takt/review-diff.txt` が最新状態を反映
- 6-iter outlier の発生率が **0%** に近づく (PR #103 のような scenario が 3-iter で収束)
- supervisor の live Read 救済が不要になる (= supervisor step は workflow に残るが、false positive 救済責務が消える)

#### 残課題 / dogfood リスク

- AI-driven 案の弱点: fix step の AI が refresh 命令を skip する可能性。Bundle Z #B-β `metrics_check` invocation の実行率を baseline として比較し、refresh 実行率 > 90% を初期目標とする。dogfood で実行率 < 90% なら **案 D (PostToolUse hook ベースの決定論層)** へ escalate を検討
- 派生プロジェクト port: `~/.takt/facets/instructions/fix.md` (global) や techbook-ledger / auto-review-fix-vc の同等ファイルへの転載が follow-up task (本 task scope 外)

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

### `check-ci-coderabbit` に CR review.body parse 機能追加 — outside-diff-range finding の programmatic 検出 (PR #108 T2-1 採用、PR #172 仕組み化方針切替 2026-05-25)

> **動機**: PR #108 で CodeRabbit が `Outside diff range comment` として review body 内に投稿した Minor finding (`docs/todo4.md` line 371/378 の retire 済前提と旧フロー混在) を、takt の `analyze-coderabbit` step が検出漏れした。`analyze-coderabbit` は `pulls/N/comments` (= inline review comment) ベースで動作するため、review.body 内のコメントは parse 対象外。結果、PR #108 で line 371/378 の修正が merge 後 follow-up commit (`vokyspww`) になった。
>
> 当初計画では暫定緩和策として **手動 checklist** (post-PR フローに目視確認 step) を追加する rule 化方針だったが、PR #172 で「rule 化は session 毎に読み込みコストがかかり、人間が忘れる」課題が顕在化。仕組み化 (`check-ci-coderabbit` 拡張で programmatic 検出) に方針切替する (`feedback_pipeline_over_rules.md` 適用)。当初 Tier 1 として位置づけていた analyzer 拡張を本 task で先行実施する形。
>
> **本タスクの位置づけ**: PR #108 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort M / Adoption Risk None)。手動 checklist の根本解決 = 検出漏れを programmatic に消滅させる。手動 step が持続性低い (= 人間が忘れる) ため、CLI 拡張で session 跨いだ品質一定化が確保される。
>
> **参照**: `.claude/feedback-reports/108.md` Tier 2 #1、PR #108 review (`Outside diff range comments` セクション、reviewer comment id 4217897113)、`src/check-ci-coderabbit/src/main.rs` (`parse_findings` 系 + `--list-findings` mode = 順位 45)、`.takt/facets/instructions/analyze-coderabbit.md`、PR #172 (順位 144 hook 化の dogfood 成功事例)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。`check-ci-coderabbit` 既存 crate への parse 機能追加 + analyze-coderabbit 連携。

#### 設計決定 (案)

- **対象 source**: `gh api repos/{owner}/{repo}/pulls/{N}/reviews --jq '.[].body'` で取得する review.body markdown 文字列
- **parse 対象セクション** (CR の出力フォーマットに準拠):
  - `## Outside diff range comments` セクション内の bullet list (file:line 参照 + comment body)
  - `## Caution` / `## Warning` セクション内の bullet (severity-marked findings)
  - 行番号参照のある generic comment (regex: `\b(file|line)\s*[:=]\s*\d+|`L\d+`|`<file>:<line>`)
- **JSON schema 拡張**: 既存 `--list-findings` mode (順位 45) の出力に `source: "inline" | "review_body"` field を追加して同型 findings として扱う:

  ```json
  {
    "findings": [
      {"severity": "minor", "file": "docs/todo4.md", "line": 371, "summary": "...", "source": "review_body"}
    ]
  }
  ```

- **analyze-coderabbit 連携**: 既存 `analyze-coderabbit` step が `--list-findings` 出力を取得する形になっていれば、source field を追加するだけで本 task の出力が自動的に下流に流れる
- **検出時の挙動**: inline findings と同じく severity 評価 → fix commit 追加 → resolve reply の通常 flow に乗る (本 task で flow 自体は変更しない)

#### 作業計画

- [ ] `check-ci-coderabbit` 現状確認 (`--list-findings` mode が 順位 45 として実装済か、未実装なら本 task 着手前に 順位 45 を land)
- [ ] review.body 取得 API (`gh api .../pulls/{N}/reviews`) wrapper 実装 (既存の gh CLI wrapper が `src/check-ci-coderabbit/src/` にあれば再利用)
- [ ] markdown parser: `## Outside diff range comments` / `## Caution` / `## Warning` セクション抽出 + bullet 毎の file:line + body 抽出
- [ ] JSON schema 拡張: `source` field 追加 (既存 schema は inline 想定なので default 値 `"inline"` で後方互換)
- [ ] test 拡充: 実 PR #108 の review.body を fixture 化 + parse 結果が期待 finding を返す test
- [ ] `analyze-coderabbit` 連携検証: source 別の handling が必要か (`outside-diff-range` の重み付けは inline と同等で進める想定)
- [ ] dogfood: 次 1-2 PR の post-pr-review で review.body finding が自動検出されることを観測
- [ ] 派生プロジェクト deploy 検討 (`check-ci-coderabbit.exe` は本リポジトリ exe なので deploy で配布、scope 内)
- [ ] 本 todo7.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `check-ci-coderabbit --list-findings --pr 108` が PR #108 の outside-diff-range finding (line 371/378) を構造化 JSON で返す
- `source` field で inline vs review_body の区別が可能
- `analyze-coderabbit` 連携で merge 前に outside-diff-range finding が actionable として扱われる
- 既存 inline finding 検出に regression なし
- `cargo test -p check-ci-coderabbit` pass

#### 詰まっている箇所

- 順位 45 (`check-ci-coderabbit --list-findings` Rust モード) の land 状況確認が前提。未 land なら本 task 着手前に 順位 45 を先に進める
- CR 側 review.body フォーマットの変更耐性: section header (`## Outside diff range comments`) が CR の出力変更で変わる可能性がある。fail-soft 設計 (parse 失敗時は空 findings で続行 + warn log) で運用継続性を確保
- false positive リスク: 行番号らしき文字列 (`L42` 等) が誤検出される可能性。CR 公式フォーマット section に限定した parse でリスク軽減

---

