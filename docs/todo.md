# TODO

## hooks-post-pr-monitor Known Issues (PR #13)

- [ ] **改行を含む `--body` が切り詰められる**: pnpm がシェル経由で exe を起動するため、改行を含む `--body` 引数が最初の行で切れる。`--body-file` 方式か、exe 内でテンプレート生成する方式に変更する
- [ ] **PR 番号パースが失敗する (pr=None)**: `gh pr create` の stdout 出力（PR URL）から番号を抽出するロジックが動作していない。`gh pr view --json number` へのフォールバックも検討
- [x] **`claude -p` の監視ジョブ起動がタイムアウトする**: 根本原因は2つ: (1) `claude -p` が新規セッションを起動し CronCreate タスクがセッション終了と同時に消滅していた → `claude -p --continue` で既存セッションに接続するよう修正 (2) Windows の `cmd /c` 経由の `<` リダイレクトが動作しない → `Command::new("claude")` で直接起動し stdin に書き込む方式に変更。タイムアウトも 120s → 300s に調整
- [ ] **`--monitor-only` で jj 環境の PR 検出が失敗する**: `gh pr view` が jj の detached HEAD を解決できず PR が見つからない。`gh pr list --head <bookmark>` 方式への変更が必要
