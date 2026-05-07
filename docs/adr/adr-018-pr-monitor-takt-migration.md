# ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止

## ステータス

承認済み (2026-04-15)

Supersedes: ADR-009 (Post-PR Monitor) の daemon + CronCreate アーキテクチャ部分

## コンテキスト

### 問題

ADR-009 で導入した cli-pr-monitor は daemon spawn + CronCreate による「お願いベース」の通知フローを採用していた:

1. **4 段階の間接連携**: daemon → state file → CronCreate → Claude 読み取り → スキル発動。各段階で失敗しうる
2. **CronCreate の信頼性**: Claude Code がセッション状態によっては CronCreate を正しく実行しない場合がある
3. **AI 分析の欠如**: CodeRabbit 指摘は state file に生の findings として保存されるが、深刻度分析や対応方針の提示は Claude の「お願いベース」
4. **ADR-015 との不整合**: push-runner は takt ベースに移行済みだが、pr-monitor は旧アーキテクチャのまま

### ADR-015 の成功パターン

push-runner で確立された「機械的ステップは Rust、AI ステップは takt」の分離原則が有効であることが実証されている。同じパターンを pr-monitor にも適用する。

## 決定

### daemon + CronCreate を廃止し、in-process sequential chain + takt に移行する

**パイプライン構成:**

```text
cli-pr-monitor.exe --monitor-only
  |
  +-- Stage 1: poll_loop (Rust, in-process, blocking)
  |     check-ci-coderabbit.exe を 2分間隔で実行
  |     最大 10分タイムアウト
  |     state file を毎回更新 (debug/observability 用)
  |
  +-- Stage 2: collect_findings (Rust)
  |     action_required or findings ありの場合:
  |       .takt/review-comments.json に書き出し
  |
  +-- Stage 3: run_takt (takt, optional)
  |     pnpm exec takt -w post-pr-review -t "analyze PR review"
  |     review-comments.json を読み、深刻度別レポートを stdout 出力
  |
  +-- Stage 4: print_report (stdout)
```

### 設計原則

1. **機械的ステップは Rust**: ポーリング、state 管理、JSON 書き出しは Rust exe 内で直接実行
2. **AI ステップは takt**: CodeRabbit 指摘の分析・深刻度分類は takt ワークフローで実行
3. **takt はオプショナル**: `pr-monitor-config.toml` に `[takt]` セクションがなければポーリング結果のみ報告
4. **CronCreate 不要**: in-process blocking で完了まで待ち、Bash tool の `run_in_background` で完了通知

### 設定ファイルの分離

`hooks-config.toml` の `[post_pr_monitor]` セクションから `pr-monitor-config.toml` に移行:

```toml
[monitor]
enabled = true
poll_interval_secs = 120
max_duration_secs = 600
check_ci = true
check_coderabbit = true

[takt]
workflow = "post-pr-review"
task = "analyze PR review comments"
extra_args = ["--pipeline", "--skip-git"]
```

## 影響

### 廃止

- `--daemon` フラグ: バックグラウンド daemon モードを削除
- `stages/daemon.rs`: spawn_daemon + run_daemon を削除
- CronCreate 指示の stdout 出力: print_cron_instruction を削除
- `pnpm mark-notified` / `pnpm check-monitor` スクリプト: 不要に
- `hooks-config.toml` の `[post_pr_monitor]` セクション: `pr-monitor-config.toml` に移行

### 維持

- `--monitor-only` フラグ: `pnpm push` チェーンからの呼び出し
- `--mark-notified` フラグ: 後方互換性のため残す（state file は debug 用に残る）
- `check-ci-coderabbit.exe`: ポーリングで使用
- `lib-report-formatter`: Finding 構造体を継続使用
- state file (`pr-monitor-state.json`): debug/observability 用に維持

### 新規追加

- `stages/poll.rs`: in-process 同期ポーリングループ
- `stages/collect.rs`: .takt/review-comments.json 書き出し
- `stages/takt.rs`: takt ワークフロー呼び出し
- `pr-monitor-config.toml`: 専用設定ファイル
- `.takt/workflows/post-pr-review.yaml`: takt ワークフロー (Phase 1: 分析のみ)
- `.takt/facets/instructions/analyze-coderabbit.md`: 分析用 instruction

## 次ステップ (スコープ外)

- **Phase 2: fix loop + re-push**: takt ワークフローに fix ステップを追加し、CodeRabbit 指摘の自動修正 + re-push まで一気通貫で処理
- **push-runner との共通化**: fix loop / report ロジックの共通 takt instruction 化

## 追記 (2026-04-22): observer モードで並行通知化

### 背景

takt fix の自動修正 + re-push が BG で走る間、Claude Code は state 更新を
リアルタイムで追えず、ユーザーが「未対応レビューをリストアップして」と
重複依頼するケースが発生した。

### 追加

- `cli-pr-monitor --observe` サブコマンドを新設 (read-only 観測パス)
  - `pr-monitor-state.json` を 5 秒間隔ポーリング
  - `action != "continue_monitoring"` を検出したら state 全文を stdout に出して exit
  - `notified=true` はサイレント exit (Claude Code 再起動時の重複防止)
  - 10 分タイムアウトで exit 1 (orphan OK)
- `pnpm observe-pr` / `pnpm mark-notified` スクリプトを復活
- `poll.rs`: iteration を跨いで `notified` フラグを preserve
- `start_monitoring` 冒頭で state を初期化 (新セッション開始時の reset)

### 設計原則 (崩さないこと)

- **主フロー (cli-pr-monitor 本体) は 100% 機械的**に detect → fix → re-push を完了させる。
  observer や Claude Code の判断を gate にしない
- **observer は read-only な side effect**。state file を読むだけで主フローには影響しない

Claude Code が Task A (`pnpm create-pr`) と Task B (`pnpm observe-pr`) を
並行 BG 起動することで、observer は早期に終端状態を報告できる (ADR-022 責務分離原則と整合)。

## 追記 (2026-05-06): Bundle b で CronCreate を park モデルとして再導入 + observer モード撤廃

### 背景

本 ADR (2026-04-15 承認) は「daemon spawn + CronCreate による 4 段間接連携」を廃止する判断だった。当時の問題は **同一プロセス内の指揮系統が長すぎて各段が失敗しうる** ことであり、CronCreate の機能そのものを否定したわけではない。

PR #104 (2026-05 観測) で **47 分の長時間 rate-limit** が観測され、本 ADR 後の `std::thread::sleep` ベース in-process 待機 + `max_duration_secs=600s` (10 分) cap で auto-retry が機能しないケースが顕在化。同プロセス常駐モデルの構造的限界 (Claude Code session 終了で sleep 中の subprocess が kill される / 10 分超で `action_required` 通知に抜ける) が明らかに。

### Bundle b の決定 (2026-05-05、PR #113-115 で land)

「daemon + CronCreate 4 段間接連携 (廃止対象)」とは異なる、**ADR-030 の責務分離パターン (Rust 状態管理 + Claude Code 周期確認) の 3 例目** として CronCreate を再導入:

- **PR #113 (Bb-1)**: rate-limit retry の CronCreate park モデル化。Rust 側は state file (`pr-monitor-state.json`) に `next_wakeup_at_unix` / `wakeup_reason` を保存して exit、Claude Code 側は stdout の `[PR_MONITOR_PARK]` envelope を読んで CronCreate (`durable: true`) で wakeup を予約。in-process sleep を排除し session 終了でも CronCreate で次回起動が保証される
- **PR #114 (Bb-2)**: review 完了待ちの CronCreate park モデル展開。`finalize_initial_review_park` / `finalize_review_recheck_park` を追加し、polling を完全排除。**observer モード (2026-04-22 追記) は本 PR で撤廃** (二重 polling = 45s gh API + 5s observer の解消)
- **PR #115 (Bb-3)**: `[review_recheck]` config 化 + SessionStart catch-up nudge (`hooks-session-start` から `additionalContext` で再起動を促す `[PR_MONITOR_CATCHUP]` 出力)

### CronCreate の重要事実 (廃止判断時に確認しなかった事項)

Bundle b 設計時に再分析して判明した CronCreate の特性:

- **時間制約なし**: 標準 5 フィールド cron 構文 (`MM HH DoM Mon DoW`)。「60 分上限」は **別ツール `ScheduleWakeup` (`/loop` 動的モード) の `clamped to [60, 3600]` 制約** であり、CronCreate には適用されない
- **One-shot**: `recurring: false` で任意の reset 時刻に 1 度だけ wakeup 可能 → 47 分でも 90 分でも cron で直接予約可能
- **session 跨ぎ**: `durable: true` で `.claude/scheduled_tasks.json` に永続化、Claude Code 再起動を跨ぐ — 本 ADR (2026-04-15 時点) で言及した「Claude Code がセッション状態によっては CronCreate を正しく実行しない」リスクは durable 化で構造的に解消
- **recurring の auto-expire**: 7 日で自動消滅 (rate-limit context では十分長い)

### 廃止した 4 段間接連携 vs Bundle b park モデルの違い

| 観点 | 廃止 (本 ADR の対象) | Bundle b 再導入 |
|---|---|---|
| アーキテクチャ | daemon spawn → state file → CronCreate → Claude → スキル | Rust exe (in-process sequential) → state file → Claude (`[PR_MONITOR_PARK]` 読み取り) → CronCreate (`durable: true`) → 再 invoke |
| 同期性 | daemon = 並行プロセス (通信失敗しうる) | exe = sequential、Claude が CronCreate 予約後に次回 invoke 待ち (in-process は 1 cycle 完結) |
| 失敗点 | 4 段全てで失敗しうる | Rust exe 1 段で完結、CronCreate は durable でロバスト |
| AI 分析 | お願いベース (失敗無音) | takt 分析 step を経由 (本 ADR の主流 + Bb-2/3 の park signal 経路) |

→ Bundle b は本 ADR の **責務分離原則そのものを継承** しつつ、長時間 rate-limit (>10 分) を構造的に解消する補完層として整合。

### 関連 PR / commit

- PR #113 (Bb-1): rate-limit retry CronCreate park
- PR #114 (Bb-2): review_recheck CronCreate park + observer 撤廃
- PR #115 (Bb-3): config 拡張 + SessionStart catch-up nudge + sanitize() (CR Major #1+#2 fold-in)
- PR #116: Bb-3 post-merge-feedback 採用 3 件 (順位 76/77/78) を todo に登録

## 追記 (2026-05-08): 順位 80 — auto-retry の transient failure scope 明文化

PR #120 (cli-finding-classifier 統合) の dogfood で観測された **「rate-limit auto-retry が `RateLimitOutcome::Posted` 経路で park 予約せず silent exit する」** 事象を契機に、auto-retry の対象 transient failure pattern を本 ADR で明文化。

### 対象 transient failure の分類

| Pattern | Detection 基準 | Auto-retry 状態 | Notes |
|---|---|---|---|
| **Rate limit (待機型)** | `Rate limit exceeded` + 未来 reset_time | ✅ 実装済 (`RateLimitOutcome::Parked`) | reset まで `until_unix_secs` で park、再投稿不要 |
| **Rate limit (即時型)** | `Rate limit exceeded` + 過去 reset_time | ✅ 実装済 (`RateLimitOutcome::Posted` + 順位 80 fix) | `@coderabbitai review` 投稿後、`review_recheck_wait_secs` で park (順位 80 fix で導入、silent exit 防止) |
| **CR 投稿エラー** (`Failed to post review comments`) | walkthrough overlay の error message | ⏳ 未実装 (順位 81、1 観測のみ低頻度) | 頻度が確認できるまで実装を defer (memory: `feedback_no_unenforced_rules` 系の判断) |
| **Wakeup 未予約 fallback** | rate-limit 検出ありで polling 終端時に next_wakeup_at_unix 未設定 | ⏳ 将来候補 | 順位 80 fix で `Posted` 経路は塞がれた、他経路の同型 silent exit が再観測されたら追加 |

### 順位 80 fix の実装ポイント (PR #129 / 2026-05-08)

`finalize_posted_retrigger` (poll.rs) を以下のように変更:

- **Before**: 投稿後 `write_state` のみして `None` を返す → polling 継続 → max_duration timeout で `make_timeout_result` (silent exit)
- **After**: 投稿後 `state.action = "parked_review_recheck"` + `next_wakeup_at_unix = now + review_recheck_wait_secs` を設定し、`format_park_signal` で PARK signal を stdout 出力、`Some(park_poll_result)` を返す → CronCreate wakeup で再開

これにより、rate-limit detection 後の **全経路で必ず PARK signal が emit される** ことが構造的に保証される。

### 順位 81 (CR 投稿エラー) を defer した理由

- **Frequency**: 1 観測 (PR #120 のみ) で systemic 性が未確認
- **Risk**: detection 拡張は false positive リスク (`Failed to post review comments` が本当に transient か、permanent failure かの判別が難しい)
- **Decision**: ユーザー方針 `feedback_no_unenforced_rules` (機械検知不可なら何もしない方がマシ) と整合、**3 PR 観測** の閾値到達まで実装を defer
- **Re-trigger 条件**: 同型の error message が別 PR で 2 件以上観測されたら順位 81 を再活性化

### 関連 PR / commit (本追記)

- PR #120 (Phase 5 land): rate-limit auto-retry の Posted 経路 silent exit 観測の根拠
- 本 PR (順位 80 fix + 順位 82 ADR update + §A-2 P-4 ledger): 本 ADR 追記の land
