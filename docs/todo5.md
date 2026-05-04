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
> **参照**: `.claude/feedback-reports/103.md` (Tier 3 #1 で同根因に別アプローチ提案、本 task で代替)、`.takt/runs/20260503-113700-pre-push-review/reports/supervisor-validation.md` (false positive 構造診断)、[ADR-036: Bundle Z 3 層アーキテクチャ](../docs/adr/adr-036-bundle-z-three-layer-review.md) (PR #97 ベースライン observation を含む、本 task は Bundle Z 3 層では塞げない独立改善)
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
- [ ] 本 todo5.md エントリを削除

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
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- post-PR workflow に「CR review.body 手動スキャン」step が追記される
- 次 1-2 PR の dogfood で本 checklist の実行が観察される
- review body 内の actionable finding が後追い修正にならない (= merge 前に検出される)

#### 詰まっている箇所

- 配置先選定 (本リポジトリ docs/workflow.md vs グローバル `~/.claude/rules/`)。本タスクは本リポジトリ固有の暫定緩和策のため、本リポジトリ docs/ への追記が妥当か
- 手動 checklist は持続性が低い (人間が忘れる) ため、Tier 1 への昇格 (= analyzer 拡張) の優先度判断が dogfood 結果に依存

---

### Document Governance: docs lifecycle 区分明文化 (PR #108 T3-1 採用)

> **動機**: PR #108 のセッションで「`docs/todo*.md` (ephemeral) と ADR / `docs/` (permanent) の lifecycle 区分」「ephemeral artifact から permanent artifact への参照禁止」「計画書 retirement 2-step workflow (entry 削除 → ファイル削除)」等の docs governance ルールが暗黙的に運用された。これらは memory ベース (例: `coding-style.md` の Cross-File Reference Lifecycle セクション) に分散して記録されているが、AI / 人間が一貫した判断を下せるよう **single source of truth** として codify が必要。
>
> **本タスクの位置づけ**: PR #108 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None / ✅ 採用)。Frequency Medium = docs PR 系で繰り返し前提として参照されるため、Effort XS で codify する ROI が高い。Document Governance の集約は将来の Cross-File Reference Lifecycle 違反 / 計画書 retirement 漏れを防ぐ防御層となる。
>
> **参照**: `.claude/feedback-reports/108.md` Tier 3 #1、`~/.claude/rules/common/coding-style.md` の "Cross-File Reference Lifecycle" セクション、PR #108 commit chain (`okwntwwy` = pipeline-token-efficiency.md retire パターン)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule への新セクション追加。

#### 設計決定 (案)

- **配置先**: `~/.claude/rules/common/` 配下、以下の 2 案から着手時に選定:
  - **案 A**: 新規 `docs-governance.md` を作成 (専用ファイル、navigability 高い)
  - **案 B**: 既存 `coding-style.md` の "Cross-File Reference Lifecycle" セクションを拡張 (関連トピック集約、ファイル数増加なし)
  - 推奨: 案 A (Document Governance 関連項目が今後増える前提で navigability を優先)
- **記述する 3 ルール**:
  - **Lifecycle 区分**: `docs/todo*.md` 系列 = ephemeral (entry が完了 / 失効で削除される)、ADR / `docs/<topic>.md` (試験運用フラグなし) = permanent (削除しない、supersede のみ)
  - **Cross-File Reference Lifecycle (既存ルール再掲 + 強化)**: permanent artifact から ephemeral artifact への参照禁止、ephemeral 同士は OK
  - **Retirement 2-step workflow**: 計画書 (試験運用フラグ付き docs/) を retire する場合の標準手順 — (1) 重要決定を ADR 化、(2) 残作業を todo*.md に移管、(3) 参照を更新、(4) ファイル削除
- **既存 rules との関係**:
  - `coding-style.md` の "Cross-File Reference Lifecycle" は本ルールの根拠の一つ → 重複排除のため本ファイル新設時は cross-link
  - 本リポジトリ固有の retirement 例 (PR #108 commit `okwntwwy`) は global rule から本リポジトリ docs/ にリンクで誘導

#### 作業計画

- [ ] 案 A / B のどちらを採用するか決定 (着手時 grep で類似 rule の配置を確認)
- [ ] `~/.claude/rules/common/docs-governance.md` (案 A) または `coding-style.md` 拡張 (案 B) で 3 ルールを codify
- [ ] 既存 `coding-style.md` の "Cross-File Reference Lifecycle" セクションから新ファイル / セクションへの cross-link を追加
- [ ] CLAUDE.md / claude_md_rule からの参照経路を確認 (新ファイル発見可能か)
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) で global rule 反映を確認 (rule は global なので自動適用、deploy 不要)
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- `~/.claude/rules/common/` 配下に Document Governance の 3 ルール (lifecycle 区分 / Cross-File Reference Lifecycle / retirement 2-step) が codify される
- 既存 `coding-style.md` Cross-File Reference Lifecycle セクションと整合
- 次回 docs retirement / lifecycle 判断時に本ルールが参照される (= AI / 人間の判断ぶれ消滅)

#### 詰まっている箇所

- 案 A vs B の選定: 専用ファイル化のメリット (navigability) vs ファイル数抑制 (locality)。Document Governance 関連項目が今後増えるかの予測に依存
- 派生プロジェクト (Python ベース) で global rule の適用範囲がどこまで及ぶか (rule は markdown text のため自動適用想定だが念のため確認)

