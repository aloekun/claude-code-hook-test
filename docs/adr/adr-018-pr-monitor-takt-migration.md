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
