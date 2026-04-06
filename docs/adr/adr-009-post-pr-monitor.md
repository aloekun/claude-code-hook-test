# ADR-009: Post-PR Monitor — push/PR作成後の CI・CodeRabbit 自動監視

## ステータス

承認済み (2026-04-01)

## コンテキスト

public リポジトリでは PR 作成や push 後に CodeRabbit による自動レビューが行われるが、
レビュー完了までに 3〜5 分程度かかるため、手動で待機・確認するのは摩擦が大きい。
同様に CI (GitHub Actions) の成否も push 後にしか確認できない。

これらを自動的に監視し、結果を Claude 経由で報告する仕組みが必要。

### 参考にした既存実装

- shomatan/cc-knowledge の `post-push-monitor.sh` + `check-ci-coderabbit.sh`
  - PostToolUse hook (bash) で `gh pr create` / `git push` を検出
  - CronCreate で定期ポーリング、gh API で CI・CodeRabbit 状態を取得
  - 構造化 JSON で判定結果を返す

ただし、シェルスクリプトは Windows 環境で直接実行できず、テスト可能性も低いため、
本プロジェクトの方針（ADR-001: Rust 実装）に従い Rust exe で再実装する。

### 検討した選択肢

1. **シェルスクリプトをそのまま採用し、Git Bash / WSL で実行**
   - Windows 環境依存が増える。テスト可能性が低い。

2. **Skill 内で Claude が gh コマンドを直接実行**
   - テスト不要だが、判定ロジックが AI 依存になり安定性に欠ける。

3. **Rust exe × 2 (hook トリガー + スタンドアロン checker) + Skill + CronCreate**
   - 判定ロジックが確定的。unit test で網羅可能。Windows ネイティブ。
   - CronCreate で定期ポーリング、Claude はスクリプトの出力を解釈するだけ。

## 決定

**選択肢 3 を採用する。**

### アーキテクチャ

```text
【PR 作成時】
Claude が "gh pr create" を実行しようとする
       │
       ▼
PreToolUse guard (gh-pr-create-guard) がブロック
  └─ 「pnpm pr-create を使ってください」と誘導
       │
       ▼
Claude が "pnpm pr-create -- --title ..." を実行
       │
       ▼
cli-pr-monitor.exe (スタンドアロン)
  ├─ gh pr create を実行（引数を転送）
  ├─ PR番号・リポジトリ情報を gh CLI で取得
  ├─ .claude/pr-monitor-state.json に初期 state 書き出し
  ├─ daemon をスポーン (自身を --daemon で起動)
  └─ stdout に CronCreate セットアップ指示を出力
       │
       ▼
Claude が stdout を読み、CronCreate で定期ジョブ作成 (任意)
  └─ command: cat .claude/pr-monitor-state.json

【既存 PR への push 時】
pnpm push → cli-push-pipeline.exe (テスト + レビュー + push)
       │
       ▼ (push 成功後に && でチェイン)
cli-pr-monitor.exe --monitor-only
  ├─ gh pr view で PR 存在確認
  ├─ PR なし → exit 0 (何もしない)
  └─ PR あり → state file 初期化 + daemon スポーン + stdout 指示

【daemon (バックグラウンド)】
cli-pr-monitor.exe --daemon --state-file <path>
  ├─ check-ci-coderabbit.exe を poll_interval_secs 間隔で実行
  ├─ 結果を pr-monitor-state.json に毎回書き出し
  ├─ 意味的終了: action != "continue_monitoring"
  └─ 安全タイムアウト: max_duration_secs で停止

【CronCreate (任意, UX 最適化レイヤー)】
cat .claude/pr-monitor-state.json
  ├─ Claude が action フィールドに従い行動
  └─ フォールバック: 手動 cat で確認可能
```

### コンポーネント

| コンポーネント | 種別 | 役割 |
|---|---|---|
| `cli-pr-monitor.exe` | スタンドアロン CLI (Rust) | PR 作成 + daemon 起動 + state file 管理 |
| `hooks-pre-tool-validate.exe` | PreToolUse hook (Rust) | `gh-pr-create-guard` で直接の `gh pr create` をブロック |
| `check-ci-coderabbit.exe` | スタンドアロン CLI (Rust) | CI・CodeRabbit 状態チェック → JSON 出力 |
| `post-pr-create-review-check` SKILL.md | Claude Skill | 監視結果の解釈・報告手順 |
| `hooks-config.toml [post_pr_monitor]` | 設定 | ポーリング間隔・監視対象の設定 |

### 判定ロジック

| CI 状態 | CodeRabbit 状態 | 指摘/未解決 | action |
|---------|----------------|------------|--------|
| pending | * | * | `continue_monitoring` |
| * | pending/not_found | * | `continue_monitoring` |
| failure | * | * | `stop_monitoring_failure` |
| * | failure/error | * | `stop_monitoring_failure` |
| success | success | あり | `action_required` |
| success | success | なし | `stop_monitoring_success` |

### 新規指摘の判定

**`created_at > PUSH_TIME` でフィルタ。`commit_id == HEAD` は使わない。**

理由: fix コミット push で HEAD が変わるが、CodeRabbit コメントは前コミットに紐づいたまま。
`commit_id` フィルタでは前コミットの指摘が全て見落とされる。

### 設定 (hooks-config.toml)

```toml
[post_pr_monitor]
enabled = true
poll_interval_secs = 120
max_duration_secs = 600
check_ci = true
check_coderabbit = true
```

## 影響

### Positive

- push/PR 作成後の CI・CodeRabbit 確認を自動化し、開発フローの摩擦を削減
- 判定ロジックが Rust の純粋関数に分離されており、unit test で網羅可能
- 既存のビルド・配布フロー (`pnpm build:all`, `pnpm deploy:hooks`) にそのまま乗る
- `hooks-config.toml` でプロジェクトごとにポーリング間隔・監視対象をカスタマイズ可能

### Negative

- CronCreate は Claude のセッション内機能であり、セッション終了時にジョブも消える
- `check-ci-coderabbit` は gh CLI に依存するため、gh 未インストール環境では動作しない

### 変更履歴

- **2026-04-03**: PostToolUse hook → スタンドアロン exe + PreToolUse guard に変更。
  元の PostToolUse hook 方式（参考: shomatan/cc-knowledge）は additionalContext で
  Claude に CronCreate を「お願い」する設計だったが、Claude が指示に従わないケースがあり
  信頼性が低かった。push-pipeline と同じ「ガード + 専用コマンド + claude -p」パターンに
  統一することで、CronCreate の実行を確実にした。

- **2026-04-05**: `claude -p --resume` → daemon + state file アーキテクチャに変更。
  VSCode 拡張では `~/.claude/sessions/` に CLI セッションのみ登録されるため、
  `claude -p --resume <session_id>` で VSCode セッションにアクセスできないことが判明。
  外部プロセスから Claude セッションに状態を注入する設計自体がアンチパターンであると認識。
  新設計: daemon が外部で監視を完結させ、結果を `.claude/pr-monitor-state.json` に書き出す。
  Claude は state file を読むだけ。CronCreate は UX 最適化レイヤーとして維持（state file の
  cat のみ実行）し、コア機能ではない。フォールバック: 手動 `cat` で確認可能。

## 参考

- ADR-001 — hooks の実装言語として Rust を採用
- ADR-003 — hooks の配置規則とビルド戦略
- ADR-006 — hooks の設定駆動型アーキテクチャ
- ADR-008 — Push Pipeline ハーネスの実装
- [shomatan/cc-knowledge](https://github.com/shomatan/cc-knowledge) — 参考実装 (shell script)
