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

---

### rate-limit retry の CronCreate 化 (Bundle b PR-1) ★ Bundle b

> **動機**: PR #104 で CodeRabbit の 47 分 rate-limit を実観測したが、現状の `cli-pr-monitor` は同一プロセス内で `std::thread::sleep` する設計のため `max_duration_secs=600s` (10 分) を超える待機ができず、長時間 rate-limit ではバウンスして `action_required` 通知 → ユーザー手動介入が必要になる。これは「Code Rabbit が rate-limit にかかった場合、解除後に自動的に `@coderabbitai review` を投稿する」という user vision の致命的乖離点。
>
> **本タスクの位置づけ**: Bundle b (CR operation 安定化) の最優先 PR。CronCreate 機構を初導入し、47 分 rate-limit を含む長時間待機を auto-retry 経路に乗せる。Bb-2 (review 完了待ちの CronCreate 化) / Bb-3 (config 拡張) は本 PR で導入する Cron 機構の上に積む。
>
> **参照**: 本セッションでの設計議論 (advisor 経由)、`src/cli-pr-monitor/src/stages/poll.rs:288` `handle_rate_limit_retry`、`docs/adr/adr-018-pr-monitor-takt-migration.md`、`docs/adr/adr-019-coderabbit-review-hybrid-policy.md`、PR #104 (`feat/comment-lint-changed-lines-scope` で 47 min rate-limit 観測)
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。Phase 3 dogfood 完了後着手推奨 (パイプライン改善が落ち着いたタイミングで Cron 機構を導入)。

#### 設計決定 (案)

- **CronCreate 機構**: Claude Code の Cron 機能で reset_at + 60s に one-shot wakeup を仕掛ける。session 非起動時は発火せず (= SessionStart catch-up は Bb-3 で対応)
- **state 拡張**: `PrMonitorState` に `next_wakeup_at: Option<DateTime<Utc>>` / `wakeup_reason: Option<String>` を追加
- **handle_rate_limit_retry の改修**:
  - 現状: `std::thread::sleep(Duration::from_secs(sleep_secs))` で同プロセス内待機
  - 新: `state.next_wakeup_at = reset_at + 60s` を保存 → CronCreate 仕掛け → 即 exit
- **wakeup 発火時の処理**: 1 回だけ gh API 確認 → rate-limit 残存なら `@coderabbitai review` post → state 更新 → CronCreate 再仕掛け (recheck)
- **`max_duration_secs > sleep_secs` 制約の撤廃**: 同プロセス常駐ではなくなるため、長時間待機の budget cap が不要に
- **既存 `max_rate_limit_retries` cap は維持**: 無限ループ防止

#### 作業計画

- [ ] CronCreate API の Rust からの呼び出し方法を調査 (Claude Code の Cron 機構が外部 CLI から呼べるか / hook 経由か)
- [ ] `PrMonitorState` に `next_wakeup_at` / `wakeup_reason` フィールド追加 + serde test
- [ ] `handle_rate_limit_retry` を `state.next_wakeup_at` 保存 + CronCreate 仕掛けに置換 (sleep 削除)
- [ ] wakeup 発火時の entry point 実装 (新 stage `wakeup` or 既存 `monitor` の再 entry)
- [ ] `max_duration_secs > sleep_secs` 制約と関連 `Err` 経路を削除
- [ ] 単体テスト: rate-limit 検出 → state.next_wakeup_at 設定 → exit が正しく行われる
- [ ] 単体テスト: wakeup 経由再起動 → @coderabbitai review post → 次回 wakeup 仕掛け
- [ ] integration test: 47 min 待機シナリオ (test 中はモック時刻で短縮)
- [ ] dogfood 1-2 PR で実 rate-limit シナリオの auto-retry を確認
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) deploy 確認
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- 47 min rate-limit でも auto-retry が動作する (バウンスしない)
- session 起動中は CronCreate で wakeup → @coderabbitai review post → review 再開
- session 非起動時は wakeup スキップ (Bb-3 の SessionStart catch-up に依存)
- 既存 `max_rate_limit_retries` cap が引き続き機能

#### 詰まっている箇所

- CronCreate を Rust 外部プロセスから呼び出せるか未確認 (hook 経由 / claude CLI 経由 / 専用 IPC)。調査結果次第で設計を組み直す可能性あり (例: Cron 仕掛けを Rust ではなく hook script で行う)
- session 非起動中の wakeup 振る舞いは Bb-3 (SessionStart catch-up) に委ねるが、移行期 (Bb-1 land 後 Bb-3 land 前) は AI 不在時の rate-limit retry が止まる過渡期になる。Bb-3 を近接 land する想定

---

### review 完了待ちの CronCreate 化 + observer 廃止 (Bundle b PR-2) ★ Bundle b

> **動機**: 現状 `cli-pr-monitor` は 45s 間隔で gh API を polling し CR review 完了を待つ + observer (BG) は state file を 5s 間隔で polling する二重 polling 設計。Claude Code が「常時稼働で polling」する負担を強いている。user vision は「経験則時刻 / GitHub UI 待機時刻に通知」 = polling 完全排除。
>
> **本タスクの位置づけ**: Bundle b PR-2。Bb-1 で導入した CronCreate 機構を review 完了待ちにも展開し、45s polling + 5s observer polling を排除。固定値 wakeup (push+5min, recheck+5min, cap=3) で代替。
>
> **参照**: `src/cli-pr-monitor/src/stages/poll.rs:275` (45s polling)、`src/cli-pr-monitor/src/stages/observe.rs:26` (5s polling)、本セッション設計議論
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。順位 53 (Bb-1) land 後着手。

#### 設計決定 (案)

- **review 完了待ちフロー** (新):
  - push 直後: cli-pr-monitor が state init (`next_wakeup_at = now + initial_review_wait_secs`, `wakeup_reason = "review_check"`) → 即 exit
  - wakeup 発火: 1 回だけ gh API 確認 → 分岐
    - findings あり / APPROVE → `state.action = action_required` / `stop_monitoring_*` → AI 介入トリガ → cron 解除 → exit
    - review なし (CR 検討中) → recheck カウンタ +1、`next_wakeup_at = now + review_recheck_wait_secs` → CronCreate 再仕掛け → exit
    - max_review_rechecks 到達 → `action_required` で「review が想定時間内に完了していない」と通知
- **observer 廃止**: 5s polling を完全削除。terminal state 通知は wakeup 経路の AI 介入トリガで代替 (Claude Code が wakeup で起動して state を読む)
- **45s polling の poll loop 削除**: `poll.rs` を「1 回 check + state 更新 + exit」に短縮
- **既存 `max_duration_secs` の意味変更**: 「single invocation の処理 timeout」に縮小 (= gh api timeout 等の安全装置)

#### 作業計画

- [ ] `poll.rs` の poll loop を「single check + state update + exit」に短縮
- [ ] `observe.rs` を削除 (or stub 化、SessionStart catch-up は Bb-3 で代替)
- [ ] CronCreate 経由の wakeup entry point を Bb-1 と統一
- [ ] config に `initial_review_wait_secs` / `review_recheck_wait_secs` / `max_review_rechecks` を追加
- [ ] 単体テスト: push → state init → exit、wakeup → review check → 分岐
- [ ] integration test: review 完了 / 未完了 / max recheck 到達 の各経路
- [ ] dogfood 数 PR で polling 排除の挙動確認 (ログから poll loop が呼ばれていないこと)
- [ ] 派生プロジェクト deploy 確認
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- `cli-pr-monitor` の poll loop が完全削除 (single check + exit のみ)
- observer の 5s polling が削除
- review 完了 / 未完了 / max recheck 到達 の各経路で意図通りの state 遷移
- 派生プロジェクトでも polling 不在を確認

#### 詰まっている箇所

- 既存の `cli-pr-monitor` を呼ぶ caller (push-runner / pnpm create-pr) との互換性維持。caller は「monitor が完了するまで待つ」前提で呼んでいる可能性 → Bb-2 では monitor が即 exit するため caller 側の挙動再確認が必要
- session 非起動時に wakeup が発火しないケース (= AI が長時間離席した場合の review check が止まる) は Bb-3 の SessionStart catch-up で対応

---

### config 拡張 + SessionStart catch-up (Bundle b PR-3) ★ Bundle b

> **動機**: Bb-1 / Bb-2 で導入した固定値 (initial_review_wait_secs / review_recheck_wait_secs / rate_limit_buffer_secs / max_review_rechecks 等) は user 要望で「変更しやすい設計」が求められた。また、Claude Code session 非起動中に発火しない wakeup を SessionStart で catch-up しないと、AI 離席中の review monitoring が静かに停止する silent loss が発生する。
>
> **本タスクの位置づけ**: Bundle b PR-3。Bb-1 / Bb-2 で導入した内部固定値を `monitor.toml` に切り出し + SessionStart hook 拡張で AI 起動時に pending wakeup を catch-up する。
>
> **参照**: 本セッション user 要望「固定値は後から調整するため、変更しやすい設計にしてください」「次回ユーザー起動時にまとめて処理」、`src/hooks-session-start/`、ADR-030 L2 recovery パターン
>
> **実行優先度**: 💎 **Tier 3** — Effort S。順位 53 / 54 land 後着手。Bb-1 land と Bb-3 land の間は AI 離席中の rate-limit retry が止まる過渡期になるため、近接 land を推奨。

#### 設計決定 (案)

- **monitor.toml の拡張**:
  ```toml
  [monitor]
  initial_review_wait_secs = 300     # push 直後 → 初回 review check
  review_recheck_wait_secs = 300     # review なしの再 check 間隔
  rate_limit_buffer_secs = 60        # reset_at に追加する余裕
  max_review_rechecks = 3            # b) の cap
  max_rate_limit_retries = 3         # 既存
  ```
- **SessionStart catch-up**:
  - `hooks-session-start` が起動時に `state.next_wakeup_at <= now` を確認
  - pending wakeup あり → `cli-pr-monitor wakeup` を即時 invoke (= 通常の wakeup 経路に乗せる)
  - これにより AI 離席で逃した wakeup を起動時に消化
- **既存設定との互換性**: 既存 `poll_interval_secs` / `max_duration_secs` は Bb-2 で意味変化済 (single invocation timeout)、deprecation 警告を出す or 自然消滅させる

#### 作業計画

- [ ] `monitor.toml` に新 config キーを追加 + parser 実装 + default 値テスト
- [ ] Bb-1 / Bb-2 で内部 const として記述した固定値を config 参照に置換
- [ ] `hooks-session-start` に `next_wakeup_at <= now` 検出 → `cli-pr-monitor wakeup` invoke ロジック追加
- [ ] 単体テスト: SessionStart で pending wakeup があれば invoke、なければ no-op
- [ ] integration test: AI 離席シナリオ → 起動時 catch-up
- [ ] dogfood: 実際に Claude Code を一度閉じて再起動した時に pending が処理されること確認
- [ ] 派生プロジェクト deploy 確認
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- 固定値が `monitor.toml` で調整可能
- SessionStart で pending wakeup を catch-up
- AI 離席時の silent loss が解消

#### 詰まっている箇所

- 既存 `poll_interval_secs` / `max_duration_secs` の deprecation 戦略 (即削除 vs 段階廃止)。Bb-2 で意味変化しているため即削除でも問題なさそうだが、派生プロジェクト config との後方互換を考慮する必要あり

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
- [ ] 本 todo5.md エントリを削除

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
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- 結合後の violation 件数が `MAX_VIOLATIONS` 以下であることが test で固定化
- 将来 `find_xxx_violations` を追加した PR で truncate 削除すると test fail で検出される

#### 詰まっている箇所

- 順位 56 (PR #104 T2-1+T2-2 test 拡充) と同 PR で bundle するか別 PR とするか。両者とも test additions、同ファイル同 test module で scope clean、bundle 推奨。

---

### ADR-035: docs 評価ポリシー (PR #107 T3-1 採用)

> **動機**: PR #107 (順位 58 = post-merge-feedback rubric format 拡張) の dogfood で、AI reviewer が code-specific review criteria (mutation / error handling / test coverage / function length / DRY 等) を documentation-only 変更に誤適用するパターンが確認された。これにより docs / facet instruction / planning markdown の編集 PR で false REJECT が発生しやすく、開発体験劣化 + token 浪費の原因となる。
>
> **本タスクの位置づけ**: PR #107 post-merge-feedback Tier 3 #1 採用 (Severity Medium / Frequency Medium / Effort M / Adoption Risk None / ✅ 採用)。`review-security.md` には既に "Docs-only changes: trust boundary criterion" セクションがあり (Bundle Z Phase 3 で導入)、本 task は同パターンを `review-simplicity.md` / `analyze-coderabbit.md` 等の他 reviewer facet にも一貫展開し、global policy として ADR-035 で集約する。
>
> **参照**: `.claude/feedback-reports/107.md` Tier 3 #1、`.takt/facets/instructions/review-security.md` 既存 trust boundary criterion、PR #106 で追加した Phase 3 review facets
>
> **実行優先度**: 💎 **Tier 3** — Effort M。ADR 1 本 + 既存 facet 横断更新 + 派生プロジェクト展開。

#### 設計決定 (案)

##### ADR の位置づけ

- 番号: ADR-035 (順位 58 で言及した ADR-035 Paradigm Shift Guidance は採用見送り済、再利用可能)
- 配置: `docs/adr/adr-035-doc-evaluation-policy.md`
- 試験運用フラグなし (定着しているパターンの formalize)

##### docs-only 変更の判定基準 (ADR で明文化)

- **path 基準**: 編集ファイルが以下のいずれかに完全に収まる:
  - `docs/**`、`*.md` (root README 等を含む)
  - `.takt/facets/instructions/**` (facet instruction = AI prompt、コードではない)
  - `.takt/workflows/**.yaml` の comment / description フィールドのみ変更
  - source code 内の doc comment (`///` / `//!` / `/** */` 等) のみ変更
- **diff 内容基準**: executable code logic への変更なし (= AST 上の関数 body / 制御フロー / 変数宣言が不変)
- 両基準を満たす PR を "docs-only" と判定

##### 適用される評価ポリシー (docs-only PR の場合)

- ✅ **適用する criteria**:
  - 既存 `review-security.md` の trust boundary criterion (auth policy / permission scope / secret handling / API contract 等が変わるか)
  - cross-reference の整合性 (リンク切れ / 廃止された path 参照 / ephemeral artifact への永続参照)
  - markdown 構文 / lint (markdownlint で機械検出される範囲)
- ❌ **適用しない criteria** (false REJECT 源泉):
  - mutation / immutability check
  - error handling / Result / panic safety
  - test coverage / test addition 要求
  - function length / nesting depth / complexity metrics
  - DRY / YAGNI を code logic 視点で適用 (docs hierarchy / 計画文書例外は維持)

##### facet instructions への反映

- `review-simplicity.md`: 既存 "DRY / YAGNI scope" の例外列挙を ADR-035 を引用する形に圧縮
- `review-security.md`: 既存 "Docs-only changes: trust boundary criterion" を ADR-035 を引用 + 拡張
- `analyze-coderabbit.md`: docs-only 判定 + code criteria 除外を新規追加 (PR #107 で発生した CodeRabbit findings の AI 適合性フィルタを支える)

#### 作業計画

- [ ] `docs/adr/adr-035-doc-evaluation-policy.md` を新規作成 (path / diff 内容判定基準 + 適用 / 不適用 criteria リスト + 既存 facet との関係)
- [ ] `CLAUDE.md` の ADR index に ADR-035 を追加
- [ ] `.takt/facets/instructions/review-simplicity.md` を ADR-035 引用に更新 (既存 DRY / YAGNI scope の docs 例外を ADR で参照)
- [ ] `.takt/facets/instructions/review-security.md` を ADR-035 引用に更新 (既存 trust boundary criterion を ADR-035 のもとへ集約)
- [ ] `.takt/facets/instructions/analyze-coderabbit.md` に docs-only 判定 + code criteria 除外を追加
- [ ] dogfood: 次の docs-only PR (例: 完了タスク削除のみの PR) で false REJECT が発生しないこと確認
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) の同 facet に展開
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- ADR-035 が docs-only 評価ポリシーを single source of truth として確立
- review-{simplicity, security} + analyze-coderabbit の 3 facets が ADR-035 を参照、code criteria の docs PR への誤適用が構造的に排除
- docs-only PR の false REJECT 率が 0% 近くに

#### 詰まっている箇所

- "docs-only" の境界判定 (例: facet instruction = AI prompt は code か docs か、yaml の structural 変更 = code か docs か) で AI 解釈が揺れる可能性 → ADR で具体例を 5-10 件列挙して cluster 化を狙う
- 既存 facet との重複削除で意味が変わらないよう注意 (review-security.md trust boundary criterion を ADR-035 に移動した結果、reviewer が docs PR を素通しするバグが発生しないか dogfood で確認)

