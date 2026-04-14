# TODO

## cli-pr-monitor Known Issues (PR #13)

- [x] **改行を含む `--body` が切り詰められる**: `--body` に改行 (`\n` リテラルまたは実改行) を検出した場合、一時ファイルに書き出して `--body-file` に自動変換する方式に変更
- [x] **PR 番号パースが失敗する (pr=None)**: `gh pr create` の stdout 出力 (PR URL) から `parse_pr_number_from_url()` で番号を直接抽出するよう修正。フォールバックとして `get_pr_info()` の多段検索 (gh pr view → jj bookmark + gh pr list --head) も追加
- [x] **`claude -p` の監視ジョブ起動がタイムアウトする**: 根本原因は2つ: (1) `claude -p` が新規セッションを起動し CronCreate タスクがセッション終了と同時に消滅していた → `claude -p --continue` で既存セッションに接続するよう修正 (2) Windows の `cmd /c` 経由の `<` リダイレクトが動作しない → `Command::new("claude")` で直接起動し stdin に書き込む方式に変更。タイムアウトも 120s → 300s に調整
- [x] **`--monitor-only` で jj 環境の PR 検出が失敗する**: `get_pr_info()` を多段フォールバックに改修。Strategy A: `gh pr view` (標準 git)、Strategy B: `get_jj_bookmark()` → `gh pr list --head <bookmark>` (jj 環境)

## CronCreate セッション問題 (PR #16 調査で発見)

- [x] **CronCreate がサブセッションに閉じ込められる**: ~~`pnpm push` 実行時、`review:ai` (`claude -p "/pre-push-review"`) のサブセッションが「最新セッション」となり、後続の `cli-pr-monitor --monitor-only` の `--continue` がサブセッションに接続してしまう。~~ ADR-015 で push-runner が takt ベースに移行されたため、`claude -p` 経由のサブセッション問題は解消。takt がプロセス内で AI レビューを管理するため、CronCreate のセッション分離問題は発生しない。

## 次ステップ: cli-pr-monitor の takt 化

- [ ] **cli-pr-monitor を takt ベースに段階的移行**: daemon ポーリング完了後に takt ワークフローで CodeRabbit 指摘の自動分析を実行。Phase 2 では fix loop による自動修正 + re-push まで一気通貫で処理する構想。push-runner と同様に「機械的ポーリングは Rust、AI 分析は takt」の分離原則を適用する (ADR-015 次ステップ参照)
