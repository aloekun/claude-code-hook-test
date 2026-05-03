# TODO (Part 5)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo4.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md / todo3.md / todo4.md の既存エントリは引き続き有効、相互に独立。新セッションでは五つすべてを確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo.md](todo.md#recommended-order-summary) を参照。

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
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- `.claude/custom-lint-rules.toml` に新 rule が追加され `.rs` ファイル内の時刻フィールド strict inequality を検出
- 1〜2 PR で dogfood し false positive がないこと

#### 詰まっている箇所

- false positive の評価 (時刻フィールド以外で legitimate な `>` が誤 block されないか)。着手時に実 codebase でグレップして影響範囲を確認。

---

### 関数長スケーリング検出 oxlint rule (PR #101 T1-4)

> **動機**: PR #101 で `parse_listed_findings` が 60 行となり、CLAUDE.md `coding-style.md` の 50 行ガイドラインを超過 (takt review が W-001 として warning)。ガイドラインは ask-based のため drift 蓄積中。96.md でも類似言及あり (関数長関連 finding)、**複数 PR で繰り返される drift**。oxlint rule で warning 40-50 行 / error 50+ 行を機械検出すれば、書いた瞬間に block されて drift しない。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 1 #4 採用 (高頻度 finding)。Bundle Z #B-α と同じ決定論的防止層。`.oxlintrc.json` + `src/oxlint-rules/` への追加で完結。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 1 #4、`.claude/feedback-reports/96.md`、`~/.claude/rules/common/coding-style.md` (50 行ガイドライン)
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。oxlint plugin への rule 追加。

#### 設計決定 (案)

- **配置先**: `.oxlintrc.json` に rule 設定 + `src/oxlint-rules/` (自作 rule の配置先) に rule 実装
- **閾値 (案)**:
  - warning: 40 行超
  - error: 50 行超 (block)
- **対象**: `.rs` / `.ts` / `.js` (言語間で共通化、ただし AST 抽象差異あり)
- **suppress**: `// oxlint-disable function-length` 行末
- **既存 rule との関係**: 既存 `src/oxlint-rules/` の rule 構造を参照 (custom rules がすでに存在する想定)

#### 作業計画

- [ ] 既存 `src/oxlint-rules/` のディレクトリ構造を確認 (Rust / TS どちらの impl か)
- [ ] 関数長計測 rule を実装 (AST node line range ベース)
- [ ] `.oxlintrc.json` に rule 有効化設定を追加
- [ ] 既存 codebase で 50 行超関数の数を事前調査 (段階的 rollout が必要か判断)
- [ ] dogfood: `parse_listed_findings` (修正前なら error として detected されるはず) を synthetic test で確認
- [ ] 派生プロジェクトへ deploy
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- oxlint rule が `.rs` / `.ts` で関数長 50 行超を error として block
- 既存 codebase で false positive 多発しないこと (1〜2 PR で dogfood)

#### 詰まっている箇所

- 既存 codebase に 50 行超関数が多数残っている場合、段階的に warning → error のロールアウトが必要。事前調査を着手時に実施。
- multi-language 対応の実装複雑度: Rust と TS で AST 抽象が異なるため、別実装か共通 abstraction 層を選定する必要あり。

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
- [ ] 本 todo5.md エントリを削除

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
> **参照**: `.claude/feedback-reports/103.md` (Tier 3 #1 で同根因に別アプローチ提案、本 task で代替)、`.takt/runs/20260503-113700-pre-push-review/reports/supervisor-validation.md` (false positive 構造診断)、[docs/pipeline-token-efficiency.md](pipeline-token-efficiency.md) PR #97 / #103 の 6-iter outlier 観測データ
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
- [ ] 本 todo5.md エントリを削除

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
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- MultiEdit でも変更行外の pre-existing violations が flag されない
- v1 (Edit) の挙動は不変
- Phase 3 (#B-γ) で reviewer の役割が「異常検知」に縮小されると本 task の効果も部分的に縮む可能性 (criterion-based finding がそもそも reviewer から消えるため)。ただし Phase 3 完了前の中間期間 + Phase 3 後も「異常検知」自体は diff を読むので効果は残る。
