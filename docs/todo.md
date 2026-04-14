# TODO

## cli-pr-monitor Known Issues (PR #13)

- [x] **改行を含む `--body` が切り詰められる**: `--body` に改行 (`\n` リテラルまたは実改行) を検出した場合、一時ファイルに書き出して `--body-file` に自動変換する方式に変更
- [x] **PR 番号パースが失敗する (pr=None)**: `gh pr create` の stdout 出力 (PR URL) から `parse_pr_number_from_url()` で番号を直接抽出するよう修正。フォールバックとして `get_pr_info()` の多段検索 (gh pr view → jj bookmark + gh pr list --head) も追加
- [x] **`claude -p` の監視ジョブ起動がタイムアウトする**: 根本原因は2つ: (1) `claude -p` が新規セッションを起動し CronCreate タスクがセッション終了と同時に消滅していた → `claude -p --continue` で既存セッションに接続するよう修正 (2) Windows の `cmd /c` 経由の `<` リダイレクトが動作しない → `Command::new("claude")` で直接起動し stdin に書き込む方式に変更。タイムアウトも 120s → 300s に調整
- [x] **`--monitor-only` で jj 環境の PR 検出が失敗する**: `get_pr_info()` を多段フォールバックに改修。Strategy A: `gh pr view` (標準 git)、Strategy B: `get_jj_bookmark()` → `gh pr list --head <bookmark>` (jj 環境)

## CronCreate セッション問題 (PR #16 調査で発見)

- [x] **CronCreate がサブセッションに閉じ込められる**: ~~`pnpm push` 実行時、`review:ai` (`claude -p "/pre-push-review"`) のサブセッションが「最新セッション」となり、後続の `cli-pr-monitor --monitor-only` の `--continue` がサブセッションに接続してしまう。~~ ADR-015 で push-runner が takt ベースに移行されたため、`claude -p` 経由のサブセッション問題は解消。takt がプロセス内で AI レビューを管理するため、CronCreate のセッション分離問題は発生しない。

## PR #33 後の改善タスク (優先度順)

- [x] **cli-pr-monitor: jj 環境での PR 作成時 --head 自動補完**: `run_create_pr()` で `--head` 未指定時に `get_jj_bookmarks()` で jj bookmark を自動検出し補完する。monitor-only モードには同等のフォールバック実装済み
- [x] **ADR-016: Claude Code Bash ツールでの長時間コマンド実行戦略**: デフォルト 120s タイムアウトでプロセスが kill される問題。`timeout: 600000` + `run_in_background: true` を長時間コマンドに必須とする方針を ADR として記録
- [x] **ADR-017: takt バージョン固定と検証環境の維持**: takt 0.35.4 で Windows 環境が壊れた実績。キャレットなし固定 + takt-test-vc を検証環境として位置づける方針を ADR として記録
- [x] **post-pr-create-review-check スキル: exe 名更新**: アーキテクチャ図の `hooks-post-pr-monitor.exe` を `cli-pr-monitor.exe` に修正 (ADR-012 命名規約反映漏れ)
- [x] **templates/ に push-runner-config.toml 追加**: 派生プロジェクトへの deploy:hooks で push-runner-config.toml が配布されない問題。テンプレート追加 + deploy-hooks.ts 更新
- [x] **pre-push-review スキルの役割整理**: takt 導入済みプロジェクトでは不要に。takt 未導入の派生プロジェクト向けにフォールバックとして維持

## 次ステップ: cli-pr-monitor の takt 化

- [ ] **cli-pr-monitor を takt ベースに段階的移行**: daemon ポーリング完了後に takt ワークフローで CodeRabbit 指摘の自動分析を実行。Phase 2 では fix loop による自動修正 + re-push まで一気通貫で処理する構想。push-runner と同様に「機械的ポーリングは Rust、AI 分析は takt」の分離原則を適用する (ADR-015 次ステップ参照)

## プロセス改善

- [ ] **マージ後フィードバックの定常化**: PR マージ後に毎回、セッションで得られた知見を整理し「ADR として記録すべきもの」「既存の仕組みに反映すべきもの」をフィードバックとして提示する。post-merge-feedback スキルの拡張、または独立したチェックリストとして運用化を検討
