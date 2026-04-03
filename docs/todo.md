# TODO

## hooks-post-pr-monitor Known Issues (PR #13)

- [ ] **改行を含む `--body` が切り詰められる**: pnpm がシェル経由で exe を起動するため、改行を含む `--body` 引数が最初の行で切れる。`--body-file` 方式か、exe 内でテンプレート生成する方式に変更する
- [ ] **PR 番号パースが失敗する (pr=None)**: `gh pr create` の stdout 出力（PR URL）から番号を抽出するロジックが動作していない。`gh pr view --json number` へのフォールバックも検討
- [ ] **`claude -p` の監視ジョブ起動がタイムアウトする**: `claude -p` による CronCreate 指示送信が 120 秒でタイムアウトする。タイムアウト値の調整、または非同期起動（バックグラウンド実行）への変更を検討
