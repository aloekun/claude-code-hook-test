# TODO

## hooks-post-pr-monitor Known Issues (PR #13)

- [x] **改行を含む `--body` が切り詰められる**: `--body` に改行 (`\n` リテラルまたは実改行) を検出した場合、一時ファイルに書き出して `--body-file` に自動変換する方式に変更
- [x] **PR 番号パースが失敗する (pr=None)**: `gh pr create` の stdout 出力 (PR URL) から `parse_pr_number_from_url()` で番号を直接抽出するよう修正。フォールバックとして `get_pr_info()` の多段検索 (gh pr view → jj bookmark + gh pr list --head) も追加
- [x] **`claude -p` の監視ジョブ起動がタイムアウトする**: 根本原因は2つ: (1) `claude -p` が新規セッションを起動し CronCreate タスクがセッション終了と同時に消滅していた → `claude -p --continue` で既存セッションに接続するよう修正 (2) Windows の `cmd /c` 経由の `<` リダイレクトが動作しない → `Command::new("claude")` で直接起動し stdin に書き込む方式に変更。タイムアウトも 120s → 300s に調整
- [x] **`--monitor-only` で jj 環境の PR 検出が失敗する**: `get_pr_info()` を多段フォールバックに改修。Strategy A: `gh pr view` (標準 git)、Strategy B: `get_jj_bookmark()` → `gh pr list --head <bookmark>` (jj 環境)
